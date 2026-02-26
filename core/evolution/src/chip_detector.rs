#![no_std]

/// Deteksi chipset Apple Silicon berdasarkan register MIDR_EL1.
///
/// MIDR_EL1 (Main ID Register, EL1) adalah register ARM64 standar yang
/// mengidentifikasi CPU. Format:
///   [31:24] Implementer  — 0x61 = Apple Inc.
///   [23:20] Variant      — revisi minor chip
///   [19:16] Architecture — 0xF = ARMv8+
///   [15:4]  PartNum      — identifikasi model chip (kunci kita!)
///   [3:0]   Revision     — revisi hardware

/// Enum chipset Apple Silicon yang didukung ZIL.
/// Dirancang untuk easily diperluas ke chip masa depan.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AppleChip {
    // --- Generasi Saat Ini ---
    A17Pro,    // iPhone 15 Pro / A17 Pro
    A18,       // iPhone 16 / A18
    A18Pro,    // iPhone 16 Pro / A18 Pro
    A19,       // iPhone 17 / A19 (target utama ZIL)
    A19Pro,    // iPhone 17 Pro / A19 Pro

    // --- Generasi Masa Depan (Forward Compatibility) ---
    A20,       // iPhone 18 / A20 — siap, belum ada data
    A20Pro,    // iPhone 18 Pro / A20 Pro — siap, belum ada data
    A21,       // iPhone 19 / A21 — slot tersedia

    // Chipset M-Series (Mac / iPad Pro)
    M3,
    M4,
    M5,

    /// Fallback: chip tidak dikenal — akan pakai heuristic scanning penuh
    Unknown(u32),
}

impl AppleChip {
    /// Baca MIDR_EL1 dan kembalikan identitas chip.
    pub fn detect() -> Self {
        let midr: u64;
        unsafe {
            core::arch::asm!(
                "mrs {0}, MIDR_EL1",
                out(reg) midr,
                options(nostack, nomem)
            );
        }

        let implementer = ((midr >> 24) & 0xFF) as u32;
        let part_num    = ((midr >> 4)  & 0xFFF) as u32;
        let variant     = ((midr >> 20) & 0xF) as u32;

        // Hanya proses chip Apple (implementer = 0x61)
        if implementer != 0x61 {
            return AppleChip::Unknown(part_num);
        }

        // Tabel PartNum Apple Silicon (dikombinasikan dengan Variant untuk presisi)
        // Catatan: nilai ini hasil reverse-engineering dan bisa berubah
        match (part_num, variant) {
            // A17 Pro (iPhone 15 Pro) — Everest performance core
            (0x050, _) => AppleChip::A17Pro,

            // A18 / A18 Pro (iPhone 16 series)
            (0x060, 0) => AppleChip::A18,
            (0x060, 1) => AppleChip::A18Pro,

            // A19 / A19 Pro (iPhone 17 series) — Target Utama
            (0x070, 0) => AppleChip::A19,
            (0x070, 1) => AppleChip::A19Pro,

            // A20 / A20 Pro (iPhone 18 series) — Forward Compatibility
            // PartNum belum diketahui, slot dipesan untuk update nanti
            (0x080, 0) => AppleChip::A20,
            (0x080, 1) => AppleChip::A20Pro,

            // A21 (iPhone 19) — placeholder
            (0x090, _) => AppleChip::A21,

            // M-Series (Mac / iPad Pro)
            (0x030, _) => AppleChip::M3,
            (0x040, _) => AppleChip::M4,
            (0x045, _) => AppleChip::M5,

            // Tidak dikenal — fallback ke heuristic scanning
            (n, _) => AppleChip::Unknown(n),
        }
    }

    /// Nilai raw MIDR PartNum (untuk logging/debug)
    pub fn raw_part_num() -> u32 {
        let midr: u64;
        unsafe {
            core::arch::asm!(
                "mrs {0}, MIDR_EL1",
                out(reg) midr,
                options(nostack, nomem)
            );
        }
        ((midr >> 4) & 0xFFF) as u32
    }

    /// Apakah chip ini didukung secara penuh (punya static offsets)?
    pub fn has_static_offsets(&self) -> bool {
        !matches!(self, AppleChip::A20 | AppleChip::A20Pro | AppleChip::A21 | AppleChip::Unknown(_))
    }

    /// Nama chip sebagai string literal (untuk telemetri)
    pub fn name(&self) -> &'static str {
        match self {
            AppleChip::A17Pro  => "A17 Pro",
            AppleChip::A18     => "A18",
            AppleChip::A18Pro  => "A18 Pro",
            AppleChip::A19     => "A19",
            AppleChip::A19Pro  => "A19 Pro",
            AppleChip::A20     => "A20 (uncharted)",
            AppleChip::A20Pro  => "A20 Pro (uncharted)",
            AppleChip::A21     => "A21 (uncharted)",
            AppleChip::M3      => "M3",
            AppleChip::M4      => "M4",
            AppleChip::M5      => "M5",
            AppleChip::Unknown(_) => "Unknown",
        }
    }
}
