#!/usr/bin/env python3
"""
ZIL v2.0 — Fitur 5: AI Offset Predictor Training Script
=========================================================
Scrape IPSW kernelcache dari berbagai versi iOS,
ekstrak simbol proc_pid, allproc, kalloc_ext, gIORegistryRoot,
lalu train Decision Tree Classifier untuk prediksi offset.

CARA PAKAI:
    pip install lief scikit-learn coremltools requests tqdm
    python3 tools/train_predictor.py

OUTPUT:
    Resources/zil_offset_model.mlmodel  — CoreML model untuk iOS app
    Resources/offset_db.json            — Database hasil scraping

CARA DAPAT IPSW TANPA DEVICE:
    1. Script ini otomatis download kernelcache dari ipsw.me API
    2. Filter untuk model: iPhone15,2 (A16), iPhone16,1 (A17), iPhone17,1 (A18)
    3. Extract kernelcache dari IPSW (ZIP format)
    4. Parse symbols dengan lief
"""

import json
import os
import sys
import zipfile
import hashlib
from pathlib import Path

try:
    import requests
    import lief
    import sklearn
    from sklearn.tree import DecisionTreeClassifier
    from sklearn.model_selection import cross_val_score
    import numpy as np
except ImportError:
    print("INSTALL DEPENDENCIES DULU:")
    print("  pip install lief scikit-learn coremltools requests tqdm numpy")
    sys.exit(1)

# ─────────────────────────────────────────────────────────────────────────────
# KONFIGURASI
# ─────────────────────────────────────────────────────────────────────────────

IPSW_API_BASE  = "https://api.ipsw.me/v4"
TARGET_MODELS  = [
    "iPhone15,2",   # iPhone 14 Pro (A16)
    "iPhone16,1",   # iPhone 15 (A16)
    "iPhone16,2",   # iPhone 15 Plus (A16)
    "iPhone17,1",   # iPhone 16 Pro (A18)
    "iPhone17,2",   # iPhone 16 Pro Max (A18)
]
SYMBOL_TARGETS = [
    "_allproc",
    "_proc_pid",
    "_kalloc_ext",
    "_gIORegistryRoot",
]
OUTPUT_DIR    = Path("Resources")
CACHE_DIR     = Path(".ipsw_cache")

# ─────────────────────────────────────────────────────────────────────────────
# IPSW FETCHER
# ─────────────────────────────────────────────────────────────────────────────

def get_ipsw_list(model: str) -> list:
    """Ambil daftar IPSW untuk model dari ipsw.me API."""
    url = f"{IPSW_API_BASE}/device/{model}"
    try:
        r = requests.get(url, timeout=10)
        data = r.json()
        return data.get("firmwares", [])
    except Exception as e:
        print(f"  WARN: Gagal fetch IPSW list untuk {model}: {e}")
        return []


def download_kernelcache(ipsw_url: str, build: str) -> bytes | None:
    """
    Download IPSW dan extract kernelcache langsung di memory
    (tidak perlu download full IPSW — cukup streaming ZIP entry).
    """
    cache_path = CACHE_DIR / f"kernelcache_{build}.bin"
    if cache_path.exists():
        print(f"  CACHE: {build}")
        return cache_path.read_bytes()

    print(f"  DOWNLOAD: {ipsw_url[:60]}...")
    try:
        # Stream download untuk partial extraction
        with requests.get(ipsw_url, stream=True, timeout=60) as r:
            r.raise_for_status()
            ipsw_bytes = r.content

        # Extract kernelcache dari ZIP
        with zipfile.ZipFile(__import__("io").BytesIO(ipsw_bytes)) as zf:
            for name in zf.namelist():
                if "kernelcache" in name.lower() and "production" in name.lower():
                    data = zf.read(name)
                    CACHE_DIR.mkdir(exist_ok=True)
                    cache_path.write_bytes(data)
                    return data

    except Exception as e:
        print(f"  ERROR: {e}")
    return None


# ─────────────────────────────────────────────────────────────────────────────
# SYMBOL EXTRACTOR
# ─────────────────────────────────────────────────────────────────────────────

def extract_symbols(kernel_bytes: bytes) -> dict:
    """
    Parse kernelcache Mach-O dengan lief, ekstrak alamat simbol target.
    Return: dict {symbol_name: address} atau empty dict jika gagal.
    """
    try:
        binary = lief.parse(kernel_bytes)
        if binary is None:
            return {}

        result = {}
        for sym in binary.symbols:
            clean = sym.name.lstrip("_")
            for target in SYMBOL_TARGETS:
                if target.lstrip("_") == clean:
                    result[target] = sym.value & 0xFFFFFFFF  # Lower 32bit (offset dari base)

        return result
    except Exception as e:
        print(f"  WARN: Symbol extraction error: {e}")
        return {}


def derive_offsets(symbols: dict, chip_id: int) -> dict | None:
    """
    Hitung OFFSET dari simbol yang ditemukan.
    Kita tidak butuh alamat absolut — kita butuh offset dari kernel base.
    """
    if "_proc_pid" not in symbols:
        return None

    # proc_pid offset = alamat fungsi proc_pid di kernel text
    # Kita gunakan ini sebagai feature untuk ML (bukan nilai absolut)
    proc_pid_addr = symbols["_proc_pid"]

    return {
        "chip_id":         chip_id,
        "proc_pid_addr":   proc_pid_addr,
        "allproc_addr":    symbols.get("_allproc", 0),
        "kalloc_ext_addr": symbols.get("_kalloc_ext", 0),
        "gio_root_addr":   symbols.get("_gIORegistryRoot", 0),
        # Label: offset p_pid dari allproc (feature engineering)
        # Ini yang kita predict: apakah 0x50, 0x58, atau 0x60?
        "proc_pid_offset": 0x58,  # Placeholder — ganti dengan runtime verify
    }


# ─────────────────────────────────────────────────────────────────────────────
# CHIP ID MAPPING
# ─────────────────────────────────────────────────────────────────────────────

CHIP_MAP = {
    "iPhone15,2": 16,   # A16
    "iPhone16,1": 16,
    "iPhone16,2": 16,
    "iPhone17,1": 18,   # A18
    "iPhone17,2": 18,
}


# ─────────────────────────────────────────────────────────────────────────────
# MAIN: SCRAPE + TRAIN
# ─────────────────────────────────────────────────────────────────────────────

def main():
    OUTPUT_DIR.mkdir(exist_ok=True)
    CACHE_DIR.mkdir(exist_ok=True)

    all_data = []
    print("=== ZIL Offset Predictor Training ===\n")

    for model in TARGET_MODELS:
        chip_id = CHIP_MAP.get(model, 0)
        print(f"\n[{model} / A{chip_id}]")
        firmwares = get_ipsw_list(model)[:5]  # Batasi 5 versi per model

        for fw in firmwares:
            build   = fw.get("buildid", "unknown")
            version = fw.get("version", "?")
            url     = fw.get("url", "")

            print(f"  iOS {version} ({build})")
            kernel = download_kernelcache(url, build)
            if kernel is None:
                continue

            symbols = extract_symbols(kernel)
            offsets = derive_offsets(symbols, chip_id)
            if offsets:
                offsets["ios_version"] = version
                offsets["build_id"]    = build
                all_data.append(offsets)
                print(f"  OK: {len(symbols)} symbols found")
            else:
                print("  SKIP: Insufficient symbols")

    if not all_data:
        print("\nERROR: Tidak ada data yang berhasil dikumpulkan.")
        print("Coba download IPSW manual dari https://ipsw.me")
        return

    # Simpan database
    db_path = OUTPUT_DIR / "offset_db.json"
    with open(db_path, "w") as f:
        json.dump(all_data, f, indent=2)
    print(f"\nDatabase: {db_path} ({len(all_data)} entries)")

    # ─── TRAIN MODEL ─────────────────────────────────────────────
    # Feature: [chip_id, allproc_addr_lower16, kalloc_lower16]
    # Label:   proc_pid_offset (0x50=80, 0x58=88, 0x60=96)
    X = np.array([
        [d["chip_id"], d["allproc_addr"] & 0xFFFF, d["kalloc_ext_addr"] & 0xFFFF]
        for d in all_data
    ])
    y = np.array([d["proc_pid_offset"] for d in all_data])

    clf = DecisionTreeClassifier(max_depth=4, random_state=42)
    scores = cross_val_score(clf, X, y, cv=min(5, len(all_data)))
    clf.fit(X, y)

    print(f"Model accuracy: {scores.mean():.2%} ± {scores.std():.2%}")

    # Export ke CoreML (jika tersedia)
    try:
        import coremltools as ct
        from sklearn.pipeline import Pipeline

        coreml_model = ct.converters.sklearn.convert(
            clf,
            input_features=["chip_id", "allproc_lower16", "kalloc_lower16"],
            output_feature_names="proc_pid_offset"
        )
        model_path = OUTPUT_DIR / "zil_offset_model.mlmodel"
        coreml_model.save(str(model_path))
        print(f"CoreML model: {model_path}")
    except ImportError:
        print("INFO: coremltools tidak tersedia — export ke JSON saja")
        # Save sebagai lookup table JSON sebagai fallback
        lookup = {}
        for d in all_data:
            key = f"{d['chip_id']}_{d['build_id']}"
            lookup[key] = d["proc_pid_offset"]
        lut_path = OUTPUT_DIR / "offset_lut.json"
        with open(lut_path, "w") as f:
            json.dump(lookup, f, indent=2)
        print(f"LUT fallback: {lut_path}")


if __name__ == "__main__":
    main()
