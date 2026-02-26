#![no_std]

use core::ptr::{read_volatile, write_volatile};

/// KCallManager — Abstraksi untuk memanggil fungsi kernel sewenang-wenang.
/// Setelah primitif KRW (Kernel Read/Write) diperoleh via exploit,
/// semua operasi kernel disalurkan melalui interface ini.
pub struct KCallManager {
    /// Alamat fungsi kernel yang bisa digunakan sebagai "springboard"
    /// untuk mengeksekusi fungsi lain dengan pivot ROP/JOP sederhana.
    springboard_addr: u64,
    /// Status apakah primitif KRW sudah aktif
    is_active: bool,
}

impl KCallManager {
    pub fn new() -> Self {
        KCallManager {
            springboard_addr: 0,
            is_active: false,
        }
    }

    /// Aktifkan setelah primitif eksploitasi berhasil
    pub fn activate(&mut self, springboard: u64) {
        self.springboard_addr = springboard;
        self.is_active = true;
    }

    /// PRIMITIF KRW: Baca 8 byte dari kernel memory
    pub fn kread(&self, addr: u64, buf: &mut [u8]) {
        if !self.is_active || addr == 0 { return; }
        for (i, byte) in buf.iter_mut().enumerate() {
            unsafe {
                *byte = read_volatile((addr + i as u64) as *const u8);
            }
        }
    }

    /// PRIMITIF KRW: Baca 64-bit pointer dari kernel
    pub fn kread_u64(&self, addr: u64) -> Option<u64> {
        if !self.is_active || addr == 0 { return None; }
        unsafe {
            Some(read_volatile(addr as *const u64))
        }
    }

    /// PRIMITIF KRW: Tulis struct ke kernel memory
    /// PERINGATAN: Operasi ini berisiko memicu PPL/SPTM jika menu ke
    /// wilayah yang diproteksi. Pastikan addr sudah divalidasi.
    pub fn kwrite_struct<T: Sized>(&self, addr: u64, data: &T) {
        if !self.is_active || addr == 0 { return; }
        unsafe {
            let src = data as *const T as *const u8;
            let dst = addr as *mut u8;
            for i in 0..core::mem::size_of::<T>() {
                write_volatile(dst.add(i), *src.add(i));
            }
        }
    }

    /// PRIMITIF KRW: Tulis satu nilai 64-bit ke kernel
    pub fn kwrite64(&self, addr: u64, value: u64) -> Result<(), &'static str> {
        if !self.is_active { return Err("KCALL_NOT_ACTIVE"); }
        if addr == 0 { return Err("KCALL_NULL_ADDR"); }
        unsafe {
            write_volatile(addr as *mut u64, value);
        }
        Ok(())
    }

    /// ALOKASI: Simulasi kalloc di kernel
    /// Implementasi nyata memerlukan kcall ke fungsi kalloc kernel.
    /// Saat ini mengembalikan alamat statis dari LOGIC_RAM untuk testing.
    pub fn kalloc(&self, size: u64) -> Option<u64> {
        if !self.is_active { return None; }
        // Placeholder — implementasi nyata via kcall ke `kalloc_ext`
        // dengan springboard_addr sebagai pivot.
        // Untuk sekarang, gunakan area dinamis di LOGIC_RAM:
        static mut BUMP_PTR: u64 = 0x100400000; // Awal arena alokasi
        unsafe {
            let result = BUMP_PTR;
            BUMP_PTR += size;
            BUMP_PTR = (BUMP_PTR + 0xFFF) & !0xFFF; // Align ke 4KB
            Some(result)
        }
    }
}
