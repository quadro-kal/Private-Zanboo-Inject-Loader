#![no_std]

/// OrganismState mendefinisikan kondisi kesehatan dari ZIL saat beroperasi di dalam kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganismState {
    Optimal,      // Berjalan normal, semua offset valid.
    Stressed,     // Terjadi beberapa Data Abort yang berhasil ditangani.
    Compromised,  // Operasi kritis gagal tetapi sistem masih stabil.
    Recovering,   // Sedang melakukan rollback atau re-scanning.
}

/// DiagnosticJournal mencatat status terakhir dan pesan kesalahan untuk membantu debugging bare-metal.
pub struct DiagnosticJournal {
    pub state: OrganismState,
    pub last_error: &'static str,
}

impl Default for DiagnosticJournal {
    fn default() -> Self {
        Self {
            state: OrganismState::Optimal,
            last_error: "NONE",
        }
    }
}
