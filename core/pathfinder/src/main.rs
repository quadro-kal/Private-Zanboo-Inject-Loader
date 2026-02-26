#![no_std]
#![no_main]

extern crate zil_core;

use zil_core::memory::scanner::MemoryScanner;

// --- KONFIGURASI MEMORI ---
const SHARED_INFO_PTR: *mut SharedBootInfo = 0x100000000 as *mut SharedBootInfo;
const AGX_BASE_GUESS: u64 = 0x204000000;
const STATIC_KERNEL_BASE: u64 = 0xFFFFFFF007004000;

// FIX INC-MANUAL-02: SharedBootInfo sekarang tanpa packed, layout explicit aligned.
// Harus identik dengan ZilSharedBootInfo di shared_types.h:
//   offset 0  : is_ready    (1B)
//   offset 1-7: _pad        (7B)
//   offset 8  : kernel_base (8B)
//   offset 16 : kernel_slide(8B)
//   offset 24 : gpu_integrity (4B)
//   offset 28 : device_id   (4B)
//   offset 32 : our_pid     (4B)
//   offset 36 : _padding    (4B)
//   TOTAL: 40 bytes, fully aligned.
#[repr(C)]
pub struct SharedBootInfo {
    pub is_ready:      bool,
    pub _pad:          [u8; 7],   // padding agar kernel_base aligned di offset 8
    pub kernel_base:   u64,
    pub kernel_slide:  u64,
    pub gpu_integrity: u32,
    pub device_id:     u32,
    pub our_pid:       u32,
    pub _padding:      u32,
}

extern "C" {
    fn zil_safe_read_32(address: u64, out_value: *mut u32) -> u8;
    
    // FIX 4: Deklarasi WFI sebagai fungsi eksternal dari pac_core.s
    // agar Rust bisa memanggil instruksi ARM64 hemat energi ini.
    fn zil_wfi_loop() -> !;
}

/// Baca PID proses Pathfinder menggunakan syscall Darwin yang reliable.
///
/// STRATEGI:
///   Tahap 1 [PRIMARY]: Syscall `getpid()` via ARM64 SVC #0x80.
///     Darwin/iOS syscall convention: X16 = nomor syscall, SVC #0x80.
///     Nomor syscall getpid = 20 (dari XNU bsd/kern/syscalls.master).
///     Ini adalah cara paling reliable dan tidak bergantung pada offset internal.
///
///   Tahap 2 [FALLBACK]: Baca dari SharedBootInfo.device_id area
///     jika syscall menghasilkan nilai tidak valid.
///
///   Tahap 3 [FINAL]: Return PID 1 sebagai debug trace.
fn read_our_pid() -> u32 {
    unsafe {
        // TAHAP 1: getpid() via Darwin syscall
        // X16 = syscall number (Darwin convention)
        // X0  = return value (PID)
        // SVC #0x80 = Darwin userspace syscall trap
        let pid: u64;
        core::arch::asm!(
            "mov x16, #20",    // getpid = syscall 20 (bsd/kern/syscalls.master)
            "svc #0x80",       // Darwin userspace syscall trap
            out("x0") pid,
            options(nostack)
        );

        let pid_u32 = pid as u32;
        if pid_u32 > 1 && pid_u32 < 0xFFFF {
            return pid_u32;
        }

        // TAHAP 2: Fallback — SharedBootInfo.device_id sebagai hint
        let shared_ptr = 0x100000000u64 as *const u32;
        let device_hint = core::ptr::read_volatile(shared_ptr.add(6));
        if device_hint > 1 && device_hint < 0xFFFF {
            return device_hint;
        }

        // TAHAP 3: Final fallback — PID 1 untuk debug trace
        1
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // 1. DAPATKAN AKSES KE PAPAN PENGUMUMAN
    let shared_mem = unsafe { &mut *SHARED_INFO_PTR };
    
    // Inisialisasi awal
    shared_mem.is_ready      = false;
    shared_mem.gpu_integrity = 0;
    shared_mem.device_id     = 0xA19;
    shared_mem.our_pid       = read_our_pid(); // DEV: tulis PID kita ke shared RAM

    // 2. MISI 1: PENGINTAIAN KASLR
    let scanner = MemoryScanner::new();
    if let Some(actual_base) = scanner.scan_for_kernel_header() {
        shared_mem.kernel_base  = actual_base;
        shared_mem.kernel_slide = actual_base.wrapping_sub(STATIC_KERNEL_BASE);
    } else {
        shared_mem.kernel_base  = 0;
        shared_mem.kernel_slide = 0;
    }

    // 3. MISI 2: PENGINTAIAN HARDWARE (GPU MMIO PROBING)
    let mut dummy_val: u32 = 0;
    let is_gpu_readable = unsafe { 
        zil_safe_read_32(AGX_BASE_GUESS, &mut dummy_val) 
    };

    shared_mem.gpu_integrity = if is_gpu_readable == 1 && dummy_val != 0xFFFFFFFF {
        1 // Lampu hijau ke Biner B
    } else {
        0 // Lampu merah — GPU tidak merespons
    };

    // 4. MISI SELESAI: TANDAI SIAP
    shared_mem.is_ready = true;

    // 5. FIX 4: GHOST MODE — Masuk ke WFI loop hemat energi
    // Pathfinder tidak boleh return, dan loop kosong boros CPU.
    // Solusi: gunakan instruksi WFI (Wait For Interrupt) dari ARM.
    unsafe { zil_wfi_loop() }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    let shared_mem = unsafe { &mut *SHARED_INFO_PTR };
    shared_mem.is_ready    = false;
    shared_mem.kernel_base = 0xDEADDEAD; // Magic = PATHFINDER_CRASH
    loop {}
}