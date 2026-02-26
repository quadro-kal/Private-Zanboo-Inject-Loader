/* ========================================================
 * ZIL FRAMEWORK: NPU PROBE DEFINITIONS (npu_probe.h)
 * Header untuk VTable probe constants — MAN-C Roadmap v1.5
 * ======================================================== */
#ifndef ZIL_NPU_PROBE_H
#define ZIL_NPU_PROBE_H

#include <stdint.h>

/* -------------------------------------------------------
 * RENTANG SCAN VTABLE ANE
 * Slot VTable externalMethod ANE biasanya ada di index 5~20.
 * Lebih kecil dari 5 = destructor/init (berbahaya jika dipanggil)
 * Lebih besar dari 20 = overestimate, kemungkinan bukan ANE method
 * ------------------------------------------------------- */
#define ANE_VTABLE_PROBE_MIN    5
#define ANE_VTABLE_PROBE_MAX   20

/* Minimum panjang cluster pointer valid yang dianggap "match" ANE methods.
 * ANE biasanya punya 3-4 method berurutan: alloc, submit, free, status.
 * Cluster < 3 slot diabaikan. */
#define ANE_VTABLE_MIN_CLUSTER  3

/* Fallback index jika probe gagal — nilai konservatif dari riset komunitas.
 * A17/A18: 6, A19/A19Pro: 7. Gunakan 7 sebagai default terbaru. */
#define ANE_VTABLE_FALLBACK_IDX 7

/* -------------------------------------------------------
 * HASIL PROBE
 * Struct untuk menyimpan hasil pemindaian VTable ANE.
 * ------------------------------------------------------- */
typedef struct {
    uint64_t alloc_buffer_idx;    /* Index: allocateComputeBuffer */
    uint64_t submit_work_idx;     /* Index: submitWorkload (alloc + 1) */
    uint64_t free_buffer_idx;     /* Index: releaseComputeBuffer (alloc + 2) */
    uint8_t  probe_succeeded;     /* 1 = probe berhasil, 0 = pakai fallback */
} AneVtableProbeResult;

/* -------------------------------------------------------
 * DEKLARASI FUNGSI (diimplementasi di iokit_shim.c)
 * ------------------------------------------------------- */

/**
 * Probe VTable ANE IOUserClient untuk menemukan index selector
 * allocateComputeBuffer secara runtime.
 *
 * @param client_ptr   Pointer ke IOUserClient object ANE
 * @param ktext_start  Awal kernel __TEXT (biasanya kernel_base)
 * @param ktext_end    Akhir kernel __TEXT (biasanya kernel_base + 32MB)
 * @return Index VTable hasil probe, atau ANE_VTABLE_FALLBACK_IDX jika gagal
 */
uint64_t iokit_probe_ane_vtable_index(uint64_t client_ptr,
                                       uint64_t ktext_start,
                                       uint64_t ktext_end);

/**
 * Buka koneksi ke ANE IOUserClient dari kernel.
 * Hanya bisa dipanggil setelah root diraih (cr_uid = 0).
 *
 * @return Pointer ke IOUserClient object, 0 jika gagal
 *
 * CARA MENDAPAT ALAMAT REAL (tanpa device):
 *   1. Download IPSW dari https://ipsw.me → pilih iOS 19.x untuk A19
 *   2. Extract kernelcache: unzip iPhone_*.ipsw && find . -name kernelcache*
 *   3. Dekompresi: jtool2 --decompress kernelcache.production.iphone17
 *   4. Cari symbol: jtool2 -S kernelcache.decompressed | grep -i ANEInterface
 *   5. Atau gunakan joker (macOS): joker -k AppleH11ANEInterface.kext
 *   6. Di Windows: gunakan Python + lief library untuk parse Mach-O
 *      pip install lief
 *      python3 -c "import lief; k=lief.parse('kernelcache'); \
 *                  [print(s.name,hex(s.value)) for s in k.symbols \
 *                   if 'ANE' in s.name or 'kalloc' in s.name.lower()]"
 */
uint64_t iokit_open_ane_client(void);

/**
 * Tutup koneksi ANE dan bersihkan state global.
 * @param client_ptr Handle yang dikembalikan iokit_open_ane_client()
 */
void iokit_close_ane_client(uint64_t client_ptr);

/**
 * Set pointer ANE client dari Rust (setelah kernel scan berhasil).
 * Dipakai ketika Rust menemukan ANE client ptr via HeuristicScanner.
 * @param client_ptr Pointer ke IOUserClient object ANE
 */
void iokit_set_ane_client_from_scan(uint64_t client_ptr);

#endif /* ZIL_NPU_PROBE_H */
