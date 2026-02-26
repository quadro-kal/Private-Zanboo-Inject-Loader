#![no_std]

/// Telemetry menyimpan data statistik performa dan deteksi kegagalan.
/// Digunakan HealingEngine untuk monitoring runtime.
/// Data dapat diekspos ke Swift UI via `zil_rust_get_telemetry()` FFI.
pub struct Telemetry {
    pub panic_near_misses: u32,  // Recovery berhasil dari kondisi Stressed
    pub successful_scans:  u32,  // Total operasi yang berhasil
    pub failed_scans:      u32,  // Total kegagalan yang tercatat
}

impl Telemetry {
    pub const fn new() -> Self {
        Self {
            panic_near_misses: 0,
            successful_scans:  0,
            failed_scans:      0,
        }
    }

    /// Hitung rasio keberhasilan dalam persen (0–100).
    /// Berguna untuk monitoring dashboard di Swift UI.
    pub fn success_rate(&self) -> u32 {
        let total = self.successful_scans + self.failed_scans;
        if total == 0 { return 100; } // Belum ada data = asumsikan sehat
        (self.successful_scans * 100) / total
    }

    /// Apakah sistem sedang dalam kondisi kritis?
    /// Kriteria: lebih dari 50% operasi gagal dalam window terakhir.
    pub fn is_degraded(&self) -> bool {
        self.success_rate() < 50
    }

    /// Reset semua counter (misalnya setelah recovery berhasil).
    pub fn reset(&mut self) {
        self.panic_near_misses = 0;
        self.successful_scans  = 0;
        self.failed_scans      = 0;
    }
}
