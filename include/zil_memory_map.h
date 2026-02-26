/* ========================================================
 * ZIL FRAMEWORK: MEMORY MAP CONSTANTS
 * Kontrak batas wilayah memori absolut.
 * Semua driver C dan modul Rust harus mengacu ke sini.
 * ======================================================== */
#ifndef ZIL_MEMORY_MAP_H
#define ZIL_MEMORY_MAP_H

/* -------------------------------------------------------
 * ZONA 1: SHARED RAM — Kotak Surat Pathfinder ↔ Executor
 * ------------------------------------------------------- */
#define ZIL_SHARED_BASE   (0x100000000ULL)  /* Awal zona komunikasi  */
#define ZIL_SHARED_SIZE   (0x4000ULL)       /* Ukuran: 16 KB (1 Page) */
#define ZIL_SHARED_END    (ZIL_SHARED_BASE + ZIL_SHARED_SIZE)

/* -------------------------------------------------------
 * ZONA 2: LOGIC RAM — Kode & Data Rust + C
 * ------------------------------------------------------- */
#define ZIL_LOGIC_BASE    (0x100004000ULL)  /* Setelah Shared RAM    */
#define ZIL_LOGIC_SIZE    (0xC7FC000ULL)    /* ~200MB - 16KB         */
#define ZIL_LOGIC_END     (ZIL_LOGIC_BASE + ZIL_LOGIC_SIZE)

/* -------------------------------------------------------
 * ZONA 3: TOOL RAM — NPU Weights, BusyBox, Tweak
 * ------------------------------------------------------- */
#define ZIL_TOOL_BASE     (0x10C800000ULL)  /* Awal zona tooling     */
#define ZIL_TOOL_SIZE     (0xC800000ULL)    /* 200MB                 */
#define ZIL_NPU_ARENA     (ZIL_TOOL_BASE)   /* 10MB pertama untuk NPU */
#define ZIL_NPU_ARENA_SZ  (0xA00000ULL)     /* 10MB                  */
#define ZIL_BUSYBOX_BASE  (ZIL_TOOL_BASE + ZIL_NPU_ARENA_SZ)

/* -------------------------------------------------------
 * HARDWARE MMIO — Apple Silicon Coprocessors
 * ------------------------------------------------------- */
#define AGX_GPU_BASE      (0x204000000ULL)  /* AGX Graphics Engine   */
#define ANE_NPU_BASE      (0x26A000000ULL)  /* Apple Neural Engine   */

/* Definisi zona yang DILARANG KERAS ditulisi (akan memicu SPTM trap) */
#define ZIL_FORBIDDEN_EL2_LOW   (0x200000000ULL)
#define ZIL_FORBIDDEN_EL2_HIGH  (0x300000000ULL)

/* Makro validasi: cek apakah addr berada di zona yang aman */
#define ZIL_ADDR_IS_SAFE(addr) \
    ((addr) < ZIL_FORBIDDEN_EL2_LOW || (addr) >= ZIL_FORBIDDEN_EL2_HIGH)

#endif /* ZIL_MEMORY_MAP_H */
