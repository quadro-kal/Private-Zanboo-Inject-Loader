#![no_std]

use core::ptr::read_volatile;

// Konstanta: kita melakukan scan pada LOGIC_RAM zone
const LOGIC_RAM_BASE: u64 = 0x100004000;
const LOGIC_RAM_SIZE: u64 = 0xC7FC000; // ~200MB

// Tanda tangan Mach-O header (magic number untuk biner Apple)
const MACHO_MAGIC_64: u32 = 0xFEEDFACF;

// Ukuran window scan per iterasi (1 page Apple Silicon = 16KB)
const SCAN_PAGE_SIZE: u64 = 0x4000;

pub struct MemoryScanner;

impl MemoryScanner {
    pub fn new() -> Self {
        MemoryScanner
    }

    /// MISI UTAMA PATHFINDER: Temukan header Mach-O kernel di memori.
    /// Kernel selalu dimulai dengan magic bytes 0xFEEDFACF.
    /// Kita scan dari atas (alamat tinggi) ke bawah karena kernel
    /// biasanya berada di area memori atas.
    pub fn scan_for_kernel_header(&self) -> Option<u64> {
        // Titik tengah pencarian: alamat kernel pre-KASLR yang diketahui
        let static_base: u64 = 0xFFFFFFF007004000;

        // MED-01 FIX: Perluas scan ke ±64MB (dari ±32MB sebelumnya).
        // iOS 18.x/19.x memakai KASLR slide lebih besar.
        // ARM64 page = 16KB, jadi kita scan per halaman.
        let scan_start = static_base.wrapping_sub(0x4000000); // -64MB
        let scan_end   = static_base.wrapping_add(0x4000000); // +64MB

        let mut addr = scan_start;
        while addr < scan_end {
            if let Some(magic) = self.safe_read_u32(addr) {
                if magic == MACHO_MAGIC_64 {
                    // Verifikasi cpu_type: 0x0100000C = CPU_TYPE_ARM64
                    // (ARM=12=0xC, 64-bit flag = 0x01000000)
                    if let Some(cpu_type) = self.safe_read_u32(addr + 4) {
                        if cpu_type == 0x0100000C {
                            return Some(addr);
                        }
                    }
                }
            }
            addr = addr.wrapping_add(SCAN_PAGE_SIZE);
        }
        None
    }

    /// Scan segmen .text kernel untuk pola instruksi ARM64.
    /// Wildcard `None` dalam pattern = byte apapun cocok.
    pub fn scan_text_segment(&self, kernel_base: u64, pattern: &[Option<u8>]) -> Option<u64> {
        // Segmen .text biasanya ada di 32MB pertama setelah kernel base
        let scan_end = kernel_base.wrapping_add(0x2000000);
        let mut addr = kernel_base;

        while addr < scan_end {
            if self.match_pattern(addr, pattern) {
                return Some(addr);
            }
            addr = addr.wrapping_add(4); // Instruksi ARM64 selalu 4 byte
        }
        None
    }

    /// Pencocokan pola byte dengan wildcard (None = skip byte ini)
    fn match_pattern(&self, addr: u64, pattern: &[Option<u8>]) -> bool {
        for (i, expected) in pattern.iter().enumerate() {
            if let Some(expected_byte) = expected {
                match self.safe_read_u8(addr + i as u64) {
                    Some(b) if b == *expected_byte => continue,
                    _ => return false,
                }
            }
        }
        true
    }

    /// Baca 1 byte dari memori tanpa panic.
    /// Return None jika alamat tidak valid.
    pub fn safe_read_u8(&self, addr: u64) -> Option<u8> {
        if !self.is_aligned_and_valid(addr, 1) {
            return None;
        }
        unsafe { Some(read_volatile(addr as *const u8)) }
    }

    /// Baca 4 byte (u32) dari memori tanpa panic.
    pub fn safe_read_u32(&self, addr: u64) -> Option<u32> {
        if !self.is_aligned_and_valid(addr, 4) {
            return None;
        }
        unsafe { Some(read_volatile(addr as *const u32)) }
    }

    /// Baca 8 byte (u64) dari memori tanpa panic.
    pub fn safe_read_u64(&self, addr: u64) -> Option<u64> {
        if !self.is_aligned_and_valid(addr, 8) {
            return None;
        }
        unsafe { Some(read_volatile(addr as *const u64)) }
    }

    /// Validasi cepat: alamat tidak null, tidak nol, dan aligned.
    fn is_aligned_and_valid(&self, addr: u64, align: u64) -> bool {
        addr != 0 && (addr % align == 0)
    }
}
