/* ========================================================
 * ZIL FRAMEWORK: SHARED TYPE DEFINITIONS (FFI CONTRACT)
 * Berisi tipe data yang wajib konsisten antara C dan Rust.
 * ======================================================== */
#ifndef ZIL_SHARED_TYPES_H
#define ZIL_SHARED_TYPES_H

#include <stdint.h>
#include <stdbool.h>

/* --- KOTAK SURAT: Shared Memory Struct (harus identik dengan Rust SharedBootInfo) ---
 *
 * LAYOUT EKSPLISIT (tanpa packed — aligned untuk ARM64 aman):
 *   offset 0  : is_ready    (bool = 1B)
 *   offset 1~7: _pad        (7B padding — supaya kernel_base aligned di 8)
 *   offset 8  : kernel_base (uint64_t = 8B)
 *   offset 16 : kernel_slide(uint64_t = 8B)
 *   offset 24 : gpu_integrity (uint32_t = 4B)
 *   offset 28 : device_id   (uint32_t = 4B)
 *   offset 32 : our_pid     (uint32_t = 4B)
 *   offset 36 : _padding    (uint32_t = 4B — align ke 8B boundary)
 *   TOTAL: 40 bytes, fully aligned.
 *
 * FIX: Sebelumnya menggunakan __attribute__((packed)) yang menyebabkan
 * kernel_base berada di offset 1 (TIDAK aligned). Ini bisa menyebabkan
 * Data Abort di ARM64 ketika SCTLR_EL1.A aktif, dan menyebabkan
 * layout mismatch dengan Rust #[repr(C)] SharedBootInfo.
 */
typedef struct {
    bool     is_ready;           /* offset 0  — flag selesai dari Pathfinder */
    uint8_t  _pad[7];            /* offset 1  — padding agar kernel_base di offset 8 */
    uint64_t kernel_base;        /* offset 8  — kernel base post-KASLR */
    uint64_t kernel_slide;       /* offset 16 — KASLR slide */
    uint32_t gpu_integrity;      /* offset 24 — checksum GPU state */
    uint32_t device_id;          /* offset 28 — MIDR PartNum chip */
    uint32_t our_pid;            /* offset 32 — PID dari Pathfinder */
    uint32_t _padding;           /* offset 36 — align ke 8B boundary */
} ZilSharedBootInfo;             /* sizeof = 40 bytes */

/* --- PRIMITIF POINTER (64-bit ARM64) --- */
typedef uint64_t VirtAddr;   /* Virtual Address (user/kernel space)  */
typedef uint64_t PhysAddr;   /* Physical Address (MMIO, raw hardware) */
typedef uint64_t KernelPtr;  /* Pointer ke struktur kernel            */

/* --- TIPE ERROR UNIVERSAL --- */
typedef int32_t ZilResult;
#define ZIL_OK       (0)
#define ZIL_ERR_PAC  (-1)   /* Pointer Authentication Code gagal     */
#define ZIL_ERR_PPL  (-2)   /* Page Protection Layer menolak akses   */
#define ZIL_ERR_SPTM (-3)   /* Hypervisor (SPTM/EL2) menolak akses  */
#define ZIL_ERR_OOM  (-4)   /* Gagal alokasi memori kernel           */

#endif /* ZIL_SHARED_TYPES_H */
