/* ========================================================
 * ZIL FRAMEWORK: ARM64 REGISTER DEFINITIONS
 * Definisi register sistem ARM64e yang relevan untuk
 * eksploitasi kernel Apple Silicon.
 * ======================================================== */
#ifndef ZIL_REGS_H
#define ZIL_REGS_H

/* -------------------------------------------------------
 * SYSTEM REGISTERS — Baca via MRS, tulis via MSR
 * ------------------------------------------------------- */

/* Exception Level Register — Baca level eksekusi saat ini */
#define CURRENTEL       "CurrentEL"

/* TTBR: Translation Table Base Register (Page Table Root) */
#define TTBR0_EL1       "TTBR0_EL1"  /* Page table user-space   */
#define TTBR1_EL1       "TTBR1_EL1"  /* Page table kernel-space */

/* SCTLR: System Control Register — toggle MMU, cache, dll */
#define SCTLR_EL1       "SCTLR_EL1"

/* TCR: Translation Control Register — ukuran VA bits */
#define TCR_EL1         "TCR_EL1"

/* ESR: Exception Syndrome Register — penyebab exception */
#define ESR_EL1         "ESR_EL1"

/* FAR: Fault Address Register — alamat yang menyebabkan fault */
#define FAR_EL1         "FAR_EL1"

/* MAIR: Memory Attribute Indirection Register */
#define MAIR_EL1        "MAIR_EL1"

/* APState/APIAKey: PAC Keys (read-only dari EL1) */
#define APIAKEYLO_EL1   "APIAKeyLo_EL1"
#define APIAKEYHI_EL1   "APIAKeyHi_EL1"
#define APDAKEYLO_EL1   "APDAKeyLo_EL1"
#define APDAKEYHI_EL1   "APDAKeyHi_EL1"

/* -------------------------------------------------------
 * SCTLR_EL1 Bit Flags
 * ------------------------------------------------------- */
#define SCTLR_M    (1ULL << 0)   /* MMU Enable                    */
#define SCTLR_A    (1ULL << 1)   /* Alignment check Enable        */
#define SCTLR_C    (1ULL << 2)   /* Data Cache Enable             */
#define SCTLR_I    (1ULL << 12)  /* Instruction Cache Enable      */
#define SCTLR_WXN  (1ULL << 19)  /* Write implies Execute Never   */
#define SCTLR_EnIA (1ULL << 31)  /* PAC Instruction Key A Enable  */
#define SCTLR_EnDA (1ULL << 27)  /* PAC Data Key A Enable         */

/* -------------------------------------------------------
 * MAKRO AKSES REGISTER
 * ------------------------------------------------------- */
#define READ_SYSREG(reg) \
    ({ uint64_t _v; __asm__ volatile("mrs %0, " reg : "=r"(_v)); _v; })

#define WRITE_SYSREG(reg, val) \
    do { __asm__ volatile("msr " reg ", %0" :: "r"((uint64_t)(val))); } while(0)

#define ISB() __asm__ volatile("isb" ::: "memory")
#define DSB() __asm__ volatile("dsb sy" ::: "memory")
#define DMB() __asm__ volatile("dmb sy" ::: "memory")

#endif /* ZIL_REGS_H */
