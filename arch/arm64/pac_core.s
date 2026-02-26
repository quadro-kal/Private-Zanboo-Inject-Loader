/* ========================================================
 * ZIL FRAMEWORK: PAC ASSEMBLY CORE (pac_core.s)
 * Instruksi assembly murni untuk Pointer Authentication.
 * Fungsi-fungsi ini dideklarasikan di include/pac_defs.h
 * Target: ARM64e dengan fitur PAC aktif (A12+)
 * ======================================================== */

.section .text, "ax"
.align 4

/* --------------------------------------------------------
 * zil_sign_ptr_da(uint64_t ptr, uint64_t context) -> uint64_t
 * Sign pointer dengan Data Key A menggunakan context modifier.
 * x0 = ptr, x1 = context, return di x0
 * -------------------------------------------------------- */
.global zil_sign_ptr_da
zil_sign_ptr_da:
    pacda  x0, x1               /* Sign x0 dengan Key DA, modifier x1 */
    ret

/* --------------------------------------------------------
 * zil_auth_ptr_da(uint64_t ptr, uint64_t context) -> uint64_t
 * Authenticate & strip PAC dari pointer data.
 * PERHATIAN: Jika auth gagal, CPU trigger exception!
 * x0 = ptr, x1 = context, return di x0
 * -------------------------------------------------------- */
.global zil_auth_ptr_da
zil_auth_ptr_da:
    autda  x0, x1               /* Auth x0 dengan Key DA, modifier x1 */
    ret

/* --------------------------------------------------------
 * zil_sign_ptr_ia(uint64_t ptr, uint64_t context) -> uint64_t
 * Sign function pointer dengan Instruction Key A.
 * x0 = ptr, x1 = context, return di x0
 * -------------------------------------------------------- */
.global zil_sign_ptr_ia
zil_sign_ptr_ia:
    pacia  x0, x1               /* Sign x0 dengan Key IA, modifier x1 */
    ret

/* --------------------------------------------------------
 * zil_strip_ptr(uint64_t ptr) -> uint64_t
 * Strip PAC tag tanpa validasi (setara XPACLRI).
 * Aman untuk membaca alamat asli tanpa triggering fault.
 * x0 = ptr, return di x0 (stripped)
 * -------------------------------------------------------- */
.global zil_strip_ptr
zil_strip_ptr:
    xpacd  x0                   /* FIX: XPACD (strip Data PAC) bukan XPACI.
                                 * Fungsi ini dipakai di konteks data pointer
                                 * (PACDA/AUTDA). XPACI hanya strip IA/IB key. */
    ret

/* --------------------------------------------------------
 * zil_safe_read_32(uint64_t address, uint32_t *out) -> uint8_t
 * Baca memori 32-bit dengan penanganan Data Abort.
 * Return 1 jika sukses, 0 jika alamat tidak valid.
 * -------------------------------------------------------- */
.global zil_safe_read_32
zil_safe_read_32:
    adr    x2, .Lread_fault
    ldr    w3, [x0]
    str    w3, [x1]
    mov    x0, #1
    ret
.Lread_fault:
    mov    x0, #0
    ret

/* --------------------------------------------------------
 * FIX 4: zil_wfi_loop() -> !
 * Infinite loop hemat energi menggunakan instruksi WFI.
 * Pathfinder memanggil ini menggantikan loop kosong `loop {}`.
 * WFI (Wait For Interrupt) membuat CPU idle hingga ada interrupt,
 * jauh lebih hemat baterai dibanding busy-loop.
 * -------------------------------------------------------- */
.global zil_wfi_loop
zil_wfi_loop:
.Lwfi_spin:
    wfi                          /* CPU sleep hingga interrupt */
    b      .Lwfi_spin            /* Kembali tidur jika ada spurious wake */

