#![no_std]

// Path relatif — engine, state, stats semua berada di modul healing yang sama
use super::state::{DiagnosticJournal, OrganismState};
use super::stats::Telemetry;

/// HealingEngine bertugas memantau operasi memori dan memicu prosedur darurat
/// jika sistem mendeteksi akses ke wilayah memori yang dilarang.
/// DEV 1: Sekarang menyertakan Telemetry untuk mencatat statistik runtime.
pub struct HealingEngine {
    panic_count: u32,
    max_retries: u32,
    /// DEV 1: Telemetry terintegrasi langsung — mencatat setiap scan dan kegagalan
    pub telemetry: Telemetry,
}

impl HealingEngine {
    pub fn new() -> Self {
        Self {
            panic_count: 0,
            max_retries: 3,
            telemetry: Telemetry::new(), // DEV 1: Inisialisasi counter telemetri
        }
    }

    /// Mencatat kegagalan ke DiagnosticJournal dan memperbarui status OrganismState.
    /// DEV 1: Juga increment counter `failed_scans` di Telemetry.
    pub fn record_failure(&mut self, journal: &mut DiagnosticJournal, reason: &'static str) {
        self.panic_count += 1;
        journal.last_error = reason;

        // DEV 1: Catat kegagalan ke telemetri
        self.telemetry.failed_scans += 1;

        // Tentukan tingkat keparahan berdasarkan jumlah kumulatif kegagalan
        journal.state = if self.panic_count >= self.max_retries {
            OrganismState::Compromised
        } else {
            OrganismState::Stressed
        };
    }

    /// DEV 1: Catat keberhasilan scan ke telemetri.
    /// Dipanggil oleh Executor setelah setiap operasi berhasil.
    pub fn record_success(&mut self) {
        self.telemetry.successful_scans += 1;
    }

    /// DEV 1: Catat near-miss panic (kegagalan yang berhasil di-recover).
    /// Digunakan ketika HealingEngine berhasil rollback dari kondisi Compromised.
    pub fn record_near_miss(&mut self) {
        self.telemetry.panic_near_misses += 1;
    }

    /// DEV 1: Coba recover dari kondisi Stressed/Compromised ke Optimal.
    /// Jika berhasil, reset panic_count dan catat near-miss.
    pub fn attempt_recovery(&mut self, journal: &mut DiagnosticJournal) -> bool {
        if self.panic_count > 0 && self.panic_count < self.max_retries {
            self.panic_count = 0;
            self.telemetry.panic_near_misses += 1;
            journal.state     = OrganismState::Recovering;
            journal.last_error = "RECOVERED";
            true
        } else {
            false
        }
    }

    /// Cek apakah sudah melewati batas toleransi kegagalan.
    pub fn is_critical(&self) -> bool {
        self.panic_count >= self.max_retries
    }

    /// Ambil snapshot statistik telemetri saat ini.
    pub fn get_telemetry_snapshot(&self) -> (u32, u32, u32) {
        (
            self.telemetry.panic_near_misses,
            self.telemetry.successful_scans,
            self.telemetry.failed_scans,
        )
    }

    /// Masukkan sistem ke mode tidur dalam menggunakan instruksi WFI ARM64.
    /// WFI = Wait For Interrupt — mematikan clock core hingga ada interrupt.
    /// Jauh lebih hemat daya daripada spin loop kosong.
    pub fn enter_deep_sleep(&self) -> ! {
        unsafe {
            // ARM64 WFI + branch loop: hemat energi, tidak muncul di CPU profiler
            core::arch::asm!(
                "1: wfi",
                "b 1b",
                options(nostack, nomem)
            );
        }
        unreachable!()
    }
}
