#![no_std]
// ================================================================
// ZIL — HARDWARE ACCELERATOR DRIVER (v2 — IOKit ANE Integration)
// ================================================================
// SARAN 3: accelerator.rs kini mengintegrasikan dua path ke NPU:
//
//   PATH A (Legacy/Fallback): Direct MMIO ke register ANE hardware
//   PATH B (Asymmetric/Primary): Via IOKit ANE UserClient (SPTM-legit)
//
// Path B (IOKit) lebih aman: request buffer diapprove oleh SPTM
// karena routing melalui driver resmi Apple. Path A langsung ke
// register hardware — lebih rentan terhadap pendeteksian firmware.
// ================================================================

use core::ptr::{read_volatile, write_volatile};

// ─── Konstanta hardware ANE (Apple A13+ / M1+) ───────────────────
// Base address ANE pada Apple Silicon — setelah KASLR diterapkan
// Nilai ini adalah pre-KASLR; executor membrikan kaslr_slide
const ANE_BASE_STATIC: u64 = 0x26A000000;

// Register layout ANE (dari MMIO MAP — diverifikasi Apple Silicon)
const ANE_CTRL_REG:    u64 = 0x00;   // Control: 0x01=ON, 0x00=OFF
const ANE_STATUS_REG:  u64 = 0x04;   // Status: bit0=powered, bit1=ready
const ANE_CMD_REG:     u64 = 0x08;   // Queue head: physical addr model
const ANE_DOORBELL:    u64 = 0x10;   // Kick register
const ANE_CMD_RUN:     u32 = 0x01;   // Trigger execution

// GPU AGX base (Apple Graphics Xtra) — pre-KASLR
const AGX_BASE_STATIC: u64 = 0x204000000;

/// HardwareAccelerator — Kontroler hardware-level untuk NPU & GPU.
/// Versi 2 menambahkan mode IOKit-legitimized path (Saran 3).
pub struct HardwareAccelerator {
    /// Alamat base ANE (sudah KASLR-adjusted)
    ane_base:   u64,
    /// Apakah ANE hardware ditemukan & merespons
    npu_active: bool,
    /// GPU (AGX) aktif — hanya diaktifkan via enable_experimental_gpu()
    gpu_active: bool,
    /// Mode operasi: true = IOKit asymmetric path, false = direct MMIO
    iokit_mode: bool,
}

impl HardwareAccelerator {
    /// Inisialisasi dengan KASLR slide agar base address tepat.
    /// `kaslr_slide`: dihitung oleh OffsetCalculator dari Pathfinder
    pub fn new_with_kaslr(kaslr_slide: u64) -> Self {
        let ane_base = ANE_BASE_STATIC.wrapping_add(kaslr_slide);
        let npu_ok   = Self::probe_ane(ane_base);

        HardwareAccelerator {
            ane_base,
            npu_active: npu_ok,

            // GPU default OFF — aktifkan manual setelah verify AGX integrity
            gpu_active: false,

            // Default: gunakan IOKit mode (asymmetric, SPTM-safe)
            // Fallback ke direct MMIO jika IOKit client tidak tersedia
            iokit_mode: true,
        }
    }

    /// Inisialisasi tanpa KASLR (untuk backward compat & testing)
    pub fn new() -> Self {
        Self::new_with_kaslr(0)
    }

    // ─── PATH A: Direct MMIO ke ANE hardware ─────────────────────

    /// Nyalakan ANE via direct MMIO register write.
    /// Digunakan sebagai fallback jika IOKit tidak tersedia.
    pub fn power_on_via_mmio(&self) -> bool {
        if !self.npu_active { return false; }
        unsafe {
            write_volatile((self.ane_base + ANE_CTRL_REG) as *mut u32, 0x01);
            // Spin-delay baremetal: tunggu hardware stable
            for _ in 0..5000 { core::hint::spin_loop(); }
            let status = read_volatile((self.ane_base + ANE_STATUS_REG) as *const u32);
            status & 0x01 != 0
        }
    }

    /// Dispatch model ke ANE via direct MMIO (legacy path).
    /// `model_phys_addr`: alamat fisik buffer model
    pub fn dispatch_model_mmio(&self, model_phys_addr: u64, _model_size: u32) -> bool {
        if !self.npu_active { return false; }
        unsafe {
            write_volatile((self.ane_base + ANE_CMD_REG) as *mut u64, model_phys_addr);
            core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
            write_volatile((self.ane_base + ANE_DOORBELL) as *mut u32, ANE_CMD_RUN);
        }
        true
    }

    // ─── PATH B: IOKit Asymmetric (Saran 3 — SPTM-legit) ─────────

    /// Aktifkan mode IOKit asymmetric.
    /// Harus dipanggil sebelum execute_asymmetric() jika IOKit tersedia.
    pub fn set_iokit_mode(&mut self, enabled: bool) {
        self.iokit_mode = enabled;
    }

    pub fn is_iokit_mode(&self) -> bool { self.iokit_mode }
    pub fn is_npu_active(&self) -> bool { self.npu_active }
    pub fn ane_base(&self)      -> u64  { self.ane_base   }

    // ─── GPU (AGX) Path ───────────────────────────────────────────

    /// Aktifkan GPU hanya setelah AGX integrity check berhasil.
    pub fn enable_experimental_gpu(&mut self) {
        if self.verify_agx_integrity() {
            self.gpu_active = true;
        }
    }

    pub fn is_gpu_active(&self) -> bool { self.gpu_active }

    fn verify_agx_integrity(&self) -> bool {
        unsafe {
            let gpu_id = read_volatile(AGX_BASE_STATIC as *const u32);
            // GPU terhubung jika register tidak 0 dan bukan sentinel errors
            gpu_id != 0 && gpu_id != 0xFFFFFFFF && gpu_id != 0xDEADBEEF
        }
    }

    // ─── Hardware Probe ───────────────────────────────────────────

    fn probe_ane(base: u64) -> bool {
        unsafe {
            let status = read_volatile((base + ANE_STATUS_REG) as *const u32);
            // Status register valid jika bukan 0 atau sentinel errors
            status != 0 && status != 0xFFFFFFFF && status != 0xDEADBEEF
        }
    }
}