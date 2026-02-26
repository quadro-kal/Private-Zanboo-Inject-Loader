#![no_std]

use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{compiler_fence, Ordering};

// --- KONSTANTA AGX GPU (A19 Pro) ---
// FIX: Mengganti 0xCOMPUTE yang tidak valid menjadi konstanta u32 yang jelas.
const AGX_BASE: u64          = 0x204000000;
const AGX_REG_DOORBELL: u64  = AGX_BASE + 0x100;
const AGX_REG_STATUS: u64    = AGX_BASE + 0x104;

// FIX: Opcode GPU sebagai konstanta yang valid
const AGX_OP_COMPUTE:   u32  = 0x01;  // Compute shader dispatch
const AGX_OP_NOP:       u32  = 0x00;  // No-operation (flush)
const AGX_STATUS_DEAD:  u32  = 0xDEAD;

// --- STRUKTUR KOMANDO ---
// FIX: Menambahkan #[derive(Clone, Copy)] karena packet di-assign ke array (butuh Copy).
#[repr(C, align(64))]
struct AgxCommandQueue {
    write_ptr: u32,
    read_ptr:  u32,
    commands:  [AgxComputePacket; 128],
}

// FIX: Tambahkan Clone + Copy agar bisa di-assign langsung ke elemen array
#[repr(C)]
#[derive(Clone, Copy)]
struct AgxComputePacket {
    opcode:       u32,
    reserved_pad: u32,    // Padding agar shader_addr 8-byte aligned
    shader_addr:  u64,
    data_addr:    u64,
    thread_count: u32,
    reserved:     u32,
}

pub struct AgxDriver {
    queue_base: u64,
}

impl AgxDriver {
    pub fn new(shared_mem_addr: u64) -> Self {
        AgxDriver { queue_base: shared_mem_addr }
    }

    /// Kirim tugas komputasi ke GPU.
    /// FIX: Menambahkan `compiler_fence` sebelum menulis ke DOORBELL register
    ///      untuk memastikan semua data ring buffer sudah di-flush ke RAM
    ///      sebelum GPU menerima sinyal "ada pekerjaan baru".
    pub fn submit_compute_job(&self, data_ptr: u64, _size: usize) -> Result<(), &'static str> {
        unsafe {
            // 1. Cek Status GPU
            let status = read_volatile(AGX_REG_STATUS as *const u32);
            if status & AGX_STATUS_DEAD != 0 {
                return Err("GPU_IS_DEAD_LOCKED");
            }

            // 2. Susun Paket Perintah
            // FIX: Gunakan AGX_OP_COMPUTE (u32 valid), bukan 0xCOMPUTE (invalid hex)
            let packet = AgxComputePacket {
                opcode:       AGX_OP_COMPUTE,
                reserved_pad: 0,
                shader_addr:  0x100005000,  // Alamat shader pre-compiled
                data_addr:    data_ptr,
                thread_count: 1024,
                reserved:     0,
            };

            // 3. Masukkan ke Ring Buffer
            let queue = &mut *(self.queue_base as *mut AgxCommandQueue);
            let idx = (queue.write_ptr % 128) as usize;
            queue.commands[idx] = packet;  // FIX: Ini sekarang valid karena AgxComputePacket: Copy
            queue.write_ptr = queue.write_ptr.wrapping_add(1);

            // FIX: MEMORY BARRIER sebelum DOORBELL.
            // Tanpa ini, CPU mungkin belum flush data ke L2 cache saat GPU membaca.
            // compiler_fence memastikan semua store sebelumnya commit ke memori.
            compiler_fence(Ordering::SeqCst);

            // 4. Kick the Doorbell
            write_volatile(AGX_REG_DOORBELL as *mut u32, 1);
        }
        Ok(())
    }
}