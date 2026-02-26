#![no_std]

use core::ptr::{read_volatile, write_volatile};

// Base address Apple Neural Engine (ANE) — dari pola Apple Silicon
// M1/M2/A15+ menempatkan ANE di kisaran alamat ini.
const ANE_BASE: u64     = 0x26A000000;
const ANE_CTRL_REG: u64 = ANE_BASE + 0x00;    // Control register
const ANE_STATUS_REG: u64 = ANE_BASE + 0x04;  // Status: 0x1 = ready
const ANE_CMD_REG: u64  = ANE_BASE + 0x08;    // Command queue head
const ANE_DOORBELL: u64 = ANE_BASE + 0x10;    // Kick register

const ANE_CMD_RUN_MODEL: u32 = 0x01;

/// NpuEngine — Kontroler tingkat rendah untuk Apple Neural Engine (ANE).
/// Tujuan: mengirim model (yang berisi payload) ke NPU untuk dieksekusi
/// di luar jangkauan pengawasan CPU/SPTM.
pub struct NpuEngine {
    base_addr: u64,
    is_ready: bool,
}

impl NpuEngine {
    pub fn new() -> Self {
        let mut engine = NpuEngine {
            base_addr: ANE_BASE,
            is_ready: false,
        };
        engine.is_ready = engine.probe_hardware();
        engine
    }

    /// Cek apakah NPU merespons pada alamat yang kita kira benar.
    fn probe_hardware(&self) -> bool {
        unsafe {
            let status = read_volatile(ANE_STATUS_REG as *const u32);
            // 0 = off, 0xDEAD = wrong addr, nilai lain = kemungkinan ready
            status != 0 && status != 0xFFFFFFFF && status != 0xDEADBEEF
        }
    }

    /// Aktifkan NPU dengan menetapkan control register.
    pub fn power_on(&self) -> bool {
        if !self.is_ready { return false; }
        unsafe {
            write_volatile(ANE_CTRL_REG as *mut u32, 0x01); // ON
            // Beri waktu singkat (loop kecil sebagai delay baremetal)
            for _ in 0..1000 { core::hint::spin_loop(); }
            let st = read_volatile(ANE_STATUS_REG as *const u32);
            st & 0x01 != 0 // bit 0 = powered
        }
    }

    /// Kirim model buffer ke NPU untuk dieksekusi.
    /// `model_phys_addr`: Alamat fisik buffer model di TOOL_RAM.
    /// `model_size`:      Ukuran buffer dalam byte.
    pub fn dispatch_model(&self, model_phys_addr: u64, model_size: u32) -> Result<(), &'static str> {
        if !self.is_ready {
            return Err("NPU_DISPATCH_FAIL: Hardware tidak ditemukan");
        }

        unsafe {
            // Tulis alamat model ke command register
            write_volatile(ANE_CMD_REG as *mut u64, model_phys_addr);

            // Memory barrier sebelum kick untuk memastikan data sudah di-flush ke RAM
            core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

            // Kick doorbell: beritahu NPU "ada model baru!"
            write_volatile(ANE_DOORBELL as *mut u32, ANE_CMD_RUN_MODEL);
        }
        Ok(())
    }
}
