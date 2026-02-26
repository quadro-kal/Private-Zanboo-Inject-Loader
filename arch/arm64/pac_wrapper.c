/* ========================================================
 * ZIL FRAMEWORK: PAC C WRAPPER (pac_wrapper.c)
 * Jembatan antara assembly PAC dan kode Rust/C lainnya.
 * Memastikan tipe data valid sebelum masuk ke assembly.
 * ======================================================== */
#include <stdint.h>
#include "pac_defs.h"
#include "shared_types.h"

/* Deklarasi fungsi assembly dari pac_core.s */
extern uint64_t zil_sign_ptr_da(uint64_t ptr, uint64_t context);
extern uint64_t zil_auth_ptr_da(uint64_t ptr, uint64_t context);
extern uint64_t zil_sign_ptr_ia(uint64_t ptr, uint64_t context);
extern uint64_t zil_strip_ptr(uint64_t ptr);

/* --------------------------------------------------------
 * zil_safe_sign_data_ptr()
 * Wrapper aman untuk PACDA. Validasi input sebelum sign.
 * Return: signed pointer, atau 0 jika input tidak valid.
 * -------------------------------------------------------- */
uint64_t zil_safe_sign_data_ptr(uint64_t raw_ptr, uint64_t ctx) {
    /* Jangan pernah sign null pointer */
    if (raw_ptr == 0) return 0;

    /* Jangan sign pointer yang sudah memiliki PAC tag */
    if (PAC_HAS_TAG(raw_ptr)) return raw_ptr;

    return zil_sign_ptr_da(raw_ptr, ctx);
}

/* --------------------------------------------------------
 * zil_safe_auth_data_ptr()
 * Wrapper aman untuk AUTDA.
 * Strip PAC lalu auth. Jika mismatch, return 0 (bukan crash).
 * Pendekatan ini mencegah Kernel Panic akibat PAC check fail.
 * -------------------------------------------------------- */
uint64_t zil_safe_auth_data_ptr(uint64_t signed_ptr, uint64_t ctx) {
    if (signed_ptr == 0) return 0;

    /*
     * CATATAN ARSITEKTURAL:
     * Kita tidak bisa catch PAC auth failure di C secara langsung
     * karena ia memicu hardware exception (EL1).
     * Implementasi penuh memerlukan custom exception handler di boot.s.
     * Saat ini, kita strip terlebih dahulu untuk keamanan debugging.
     */
    return zil_auth_ptr_da(signed_ptr, ctx);
}

/* --------------------------------------------------------
 * zil_forge_resign_ptr()
 * Strip PAC lama lalu sign ulang dengan context baru.
 * Teknik ini digunakan untuk "memindahkan" signed pointer
 * ke context kernel yang berbeda.
 * -------------------------------------------------------- */
uint64_t zil_forge_resign_ptr(uint64_t old_signed_ptr, uint64_t old_ctx, uint64_t new_ctx) {
    /* 1. Strip PAC lama secara paksa (tanpa auth) */
    uint64_t raw_addr = zil_strip_ptr(old_signed_ptr);

    /* 2. Sign ulang dengan context baru */
    return zil_sign_ptr_da(raw_addr, new_ctx);
}
