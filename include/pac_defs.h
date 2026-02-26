/* ========================================================
 * ZIL FRAMEWORK: PAC (Pointer Authentication Code) DEFINITIONS
 * Header ini mendefinisikan konstanta dan makro untuk
 * menangani instruksi PAC di ARM64e (A17/A18/A19 Pro).
 * ======================================================== */
#ifndef ZIL_PAC_DEFS_H
#define ZIL_PAC_DEFS_H

#include <stdint.h>

/* -------------------------------------------------------
 * KONSTANTA PAC KEY (Konteks Apple XNU)
 * Apple menggunakan key berbeda untuk kode vs data:
 * IA/IB = Instruction A/B (untuk function pointers)
 * DA/DB = Data A/B (untuk data pointers)
 * ------------------------------------------------------- */
#define PAC_KEY_IA  0   /* Instruction Auth Key A (paling umum) */
#define PAC_KEY_IB  1   /* Instruction Auth Key B               */
#define PAC_KEY_DA  2   /* Data Auth Key A                      */
#define PAC_KEY_DB  3   /* Data Auth Key B                      */

/* -------------------------------------------------------
 * MASK: Bit-bit mana saja yang berisi PAC signature
 *
 * ARM64e / Apple TBI (Top Byte Ignore) reality:
 *   bits[63:56] — PAC tag (8 bit, bisa overlay dengan TBI zone)
 *   bits[55]    — Sign-extension (canonical address bit)
 *   bits[54:0]  — Alamat asli
 *
 * PAC_STRIP_MASK yang benar harus clear bits[63:55] (PAC+sign),
 * menyisakan hanya bits[54:0] sebagai raw virtual address.
 *
 * FIX: Nilai lama 0x0000FFFFFFFFFFFF mempertahankan bits[55:48]
 * yang merupakan zone PAC Apple → hasil strip korup di pointer
 * yang memakai full 48-bit VA range.
 * ------------------------------------------------------- */
#define PAC_STRIP_MASK  (0x007FFFFFFFFFFFFFull)  /* bits[54:0] — VA asli */
#define PAC_TAG_MASK    (0xFF80000000000000ull)  /* bits[63:55] — PAC+sign */

/* Strip PAC dari sebuah pointer (dapatkan alamat aslinya) */
#define PAC_STRIP(ptr)  ((uint64_t)(ptr) & PAC_STRIP_MASK)

/* Cek apakah pointer memiliki PAC tag */
#define PAC_HAS_TAG(ptr) (((uint64_t)(ptr) & PAC_TAG_MASK) != 0)

/* -------------------------------------------------------
 * DEKLARASI FFI: Fungsi dari pac_core.s
 * Bungkus instruksi assembly mentah PACDA/PACDB/AUTDA/AUTIA
 * ------------------------------------------------------- */
extern uint64_t zil_sign_ptr_da(uint64_t ptr, uint64_t context);
extern uint64_t zil_auth_ptr_da(uint64_t ptr, uint64_t context);
extern uint64_t zil_sign_ptr_ia(uint64_t ptr, uint64_t context);
extern uint64_t zil_strip_ptr(uint64_t ptr);    /* XPACD — strip Data PAC */

/* Fungsi dari pac_core.s yang dipakai Pathfinder */
extern uint8_t  zil_safe_read_32(uint64_t address, uint32_t *out_value);

#endif /* ZIL_PAC_DEFS_H */
