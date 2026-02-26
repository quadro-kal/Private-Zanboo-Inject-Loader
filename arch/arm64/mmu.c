/* ========================================================
 * ZIL FRAMEWORK: MMU UTILITIES (mmu.c)
 * Fungsi-fungsi untuk membaca dan menganalisis
 * Page Table pada Apple Silicon secara read-only.
 * PERINGATAN: Penulisan ke Page Table akan memicu SPTM trap.
 * ======================================================== */
#include <stdint.h>
#include "shared_types.h"
#include "zil_memory_map.h"
#include "regs.h"

/* --------------------------------------------------------
 * zil_get_ttbr1() — Dapatkan base Page Table kernel
 * Ini adalah titik permulaaan untuk resolusi alamat virtual.
 * -------------------------------------------------------- */
uint64_t zil_get_ttbr1(void) {
    return READ_SYSREG(TTBR1_EL1);
}

/* --------------------------------------------------------
 * zil_virt_to_phys_approx() — Resolusi alamat virtual → fisik
 * Melakukan page walk manual melalui 4 level Page Table ARM64.
 *
 * CATATAN: Fungsi ini bersifat read-only dan hanya membaca
 * metadata Page Table Entry (PTE). Tidak ada penulisan.
 * Return: Physical address, atau 0 jika mapping tidak ditemukan.
 * -------------------------------------------------------- */
uint64_t zil_virt_to_phys_approx(uint64_t virt_addr) {
    /* Baca base Translation Table */
    uint64_t ttbr = zil_get_ttbr1();
    if (ttbr == 0) return 0;

    /* ARM64 menggunakan 4 level Page Table (L0 → L3) pada 4KB pages */
    /* Ekstrak index untuk setiap level dari virtual address bits */
    uint64_t l0_idx = (virt_addr >> 39) & 0x1FF;
    uint64_t l1_idx = (virt_addr >> 30) & 0x1FF;
    uint64_t l2_idx = (virt_addr >> 21) & 0x1FF;
    uint64_t l3_idx = (virt_addr >> 12) & 0x1FF;
    uint64_t page_off = virt_addr & 0xFFF;

    /* Level 0 Walk */
    uint64_t *l0_table = (uint64_t *)(ttbr & ~0xFFFULL);
    uint64_t l1_entry = l0_table[l0_idx];
    if (!(l1_entry & 0x1)) return 0;  /* Not valid */

    /* Level 1 Walk */
    uint64_t *l1_table = (uint64_t *)(l1_entry & ~0xFFFULL);
    uint64_t l2_entry = l1_table[l1_idx];
    if (!(l2_entry & 0x1)) return 0;

    /* Level 2 Walk */
    uint64_t *l2_table = (uint64_t *)(l2_entry & ~0xFFFULL);
    uint64_t l3_entry = l2_table[l2_idx];
    if (!(l3_entry & 0x1)) return 0;

    /* Level 3 → Physical Page */
    uint64_t *l3_table = (uint64_t *)(l3_entry & ~0xFFFULL);
    uint64_t phys_page = l3_table[l3_idx] & ~0xFFFULL;

    return phys_page + page_off;
}

/* --------------------------------------------------------
 * zil_is_kernel_mapped() — Cek apakah alamat virtual di-map kernel
 * Berguna sebelum scanner mencoba baca dari alamat tersebut.
 * -------------------------------------------------------- */
int zil_is_kernel_mapped(uint64_t virt_addr) {
    /* Alamat kernel space dimulai dari bit[63:48] = 0xFFFF */
    return ((virt_addr >> 48) == 0xFFFF);
}
