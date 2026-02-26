/* ========================================================
 * ZIL FRAMEWORK: BARE-METAL BOOTSTRAP (boot.s)
 * Entry point pertama setelah loader menempatkan biner kita.
 * Target: ARM64e / Apple Silicon (A19 Pro / M-Series)
 *
 * CHANGELOG (2026-02-25):
 *   + Tambah ARM64 Exception Vector Table (VBAR_EL1)
 *   + Data Abort handler dengan X2-recovery untuk zil_safe_read_32
 * ======================================================== */

.section .text.entry, "ax"     /* Ditempatkan paling depan oleh linker.ld */
.global _start
.align 4                        /* Wajib 4-byte aligned untuk instruksi ARM64 */

_start:
    /* ====================================================
     * LANGKAH 1: ISOLASI LINGKUNGAN
     * Matikan semua interrupt untuk mencegah gangguan
     * dari hardware saat kita menginisialisasi stack.
     * ==================================================== */
    msr  DAIFSet, #0xF           /* Mask: Debug, Async, IRQ, FIQ semua off */

    /* ====================================================
     * LANGKAH 2: SETUP STACK POINTER
     * Linker script menempatkan __stack_top di ujung BSS.
     * SP harus menunjuk ke sana sebelum APAPUN dipanggil.
     * ==================================================== */
    adrp x0, __stack_top
    add  x0, x0, :lo12:__stack_top
    mov  sp, x0

    /* ====================================================
     * LANGKAH 3: BERSIHKAN BSS
     * Variabel global yang tidak diinisialisasi harus = 0.
     * Tanpa ini, Rust mungkin membaca sampah dari memori.
     * ==================================================== */
    adrp x0, __bss_start
    add  x0, x0, :lo12:__bss_start
    adrp x1, __bss_end
    add  x1, x1, :lo12:__bss_end

.bss_clear_loop:
    cmp  x0, x1
    b.ge .bss_clear_done
    str  xzr, [x0], #8          /* Tulis 8 byte nol, maju 8 byte */
    b    .bss_clear_loop

.bss_clear_done:
    /* ====================================================
     * LANGKAH 3.5: DAFTARKAN EXCEPTION VECTOR TABLE
     * VBAR_EL1 harus diset SEBELUM masuk ke kode Rust.
     * Tanpa ini, Data Abort dari zil_safe_read_32 akan
     * crash ke handler default kernel (panik).
     * ==================================================== */
    adrp x0, zil_exception_vectors
    add  x0, x0, :lo12:zil_exception_vectors
    msr  VBAR_EL1, x0            /* Daftarkan vektor kita ke CPU */
    isb                          /* Instruction Sync Barrier — flush pipeline */

    /* ====================================================
     * LANGKAH 4: ENTRY KE RUST
     * Lompat ke _start() yang didefinisikan di pathfinder
     * atau executor (tergantung biner mana yang dibangun).
     * ==================================================== */
    bl   zil_rust_entry          /* Call fungsi Rust utama */

    /* ====================================================
     * LANGKAH 5: DEAD LOOP (Seharusnya tidak pernah sampai sini)
     * Rust _start() tidak pernah return karena dideklarasikan -> !
     * ==================================================== */
.hang:
    wfi                          /* Wait For Interrupt (hemat energi) */
    b    .hang


/* ============================================================
 * ZIL EXCEPTION VECTOR TABLE
 * ============================================================
 *
 * ARM64 Vector Table Layout (ARM DDI 0487, Section D1.10):
 * Setiap "quadrant" menangani exception dari exception level
 * dan stack pointer yang berbeda. Setiap entry = 128 byte (0x80).
 * Satu quadrant = 4 entry x 128 byte = 512 byte (0x200).
 * Total tabel = 4 quadrant x 512 byte = 2048 byte (0x800).
 *
 * VBAR_EL1 harus di-align ke batas 2KB (0x800 bytes).
 *
 * Quadrant yang relevan untuk ZIL:
 *   Q0 (VBAR+0x000) : SP_EL0 — kita tidak pakai
 *   Q1 (VBAR+0x200) : SP_EL1 — INI YANG KITA PAKAI (kode kita berjalan di EL1)
 *   Q2 (VBAR+0x400) : AArch64 level lebih rendah
 *   Q3 (VBAR+0x600) : AArch32 level lebih rendah
 *
 * Di setiap quadrant ada 4 handler:
 *   +0x00 : Synchronous (Data Abort, SVC, dll) ← KITA TANGKAP SINI
 *   +0x80 : IRQ/vIRQ
 *   +0x100: FIQ/vFIQ
 *   +0x180: SError/vSError
 * ============================================================ */

.section .text, "ax"
.global zil_exception_vectors
.align 11                        /* WAJIB: align ke 2KB = 2^11 */

zil_exception_vectors:

    /* ---- Q0: Current EL with SP_EL0 ---- */
    /* +0x000 Synchronous */
    .align 7
    b zil_unhandled_exception
    /* +0x080 IRQ */
    .align 7
    b zil_unhandled_exception
    /* +0x100 FIQ */
    .align 7
    b zil_unhandled_exception
    /* +0x180 SError */
    .align 7
    b zil_unhandled_exception

    /* ---- Q1: Current EL with SP_EL1 (ZIL berjalan di sini) ---- */
    /* +0x200 Synchronous ← Data Abort ditangkap di sini */
    .align 7
    b zil_sync_handler_el1
    /* +0x280 IRQ */
    .align 7
    b zil_unhandled_exception
    /* +0x300 FIQ */
    .align 7
    b zil_unhandled_exception
    /* +0x380 SError */
    .align 7
    b zil_unhandled_exception

    /* ---- Q2: Lower EL using AArch64 ---- */
    /* +0x400 Synchronous */
    .align 7
    b zil_unhandled_exception
    /* +0x480 IRQ */
    .align 7
    b zil_unhandled_exception
    /* +0x500 FIQ */
    .align 7
    b zil_unhandled_exception
    /* +0x580 SError */
    .align 7
    b zil_unhandled_exception

    /* ---- Q3: Lower EL using AArch32 ---- */
    /* +0x600 Synchronous */
    .align 7
    b zil_unhandled_exception
    /* +0x680 IRQ */
    .align 7
    b zil_unhandled_exception
    /* +0x700 FIQ */
    .align 7
    b zil_unhandled_exception
    /* +0x780 SError */
    .align 7
    b zil_unhandled_exception


/* ============================================================
 * zil_sync_handler_el1 — Synchronous Exception Handler (EL1)
 * ============================================================
 *
 * X2-AS-RECOVERY MECHANISM (untuk zil_safe_read_32):
 *
 *   Sebelum LDR yang berbahaya, zil_safe_read_32 menyimpan
 *   alamat recovery (.Lread_fault) ke register X2:
 *
 *     adr x2, .Lread_fault
 *     ldr w3, [x0]       ← jika ini crash → kita masuk sini
 *
 *   Handler ini membaca X2 dari saved register context:
 *   - Jika X2 != 0: ini adalah "safe read" yang gagal → lompat ke X2
 *   - Jika X2 == 0: exception tidak terduga → lompat ke panic handler
 *
 * REGISTER LAYOUT SAAT MASUK HANDLER:
 *   Semua register unsaved — kita harus simpan dulu ke stack.
 * ============================================================ */

zil_sync_handler_el1:
    /* --- SIMPAN KONTEKS REGISTER (callee & caller saved) --- */
    /* Kita simpan x0-x17 dan x29, x30 (LR/FP) ke stack sementara */
    sub  sp, sp, #160            /* Reservasi 160 byte: 20 x 8B register */
    stp  x0,  x1,  [sp, #0]
    stp  x2,  x3,  [sp, #16]    /* X2 ada di [sp+16] — kita butuh ini! */
    stp  x4,  x5,  [sp, #32]
    stp  x6,  x7,  [sp, #48]
    stp  x8,  x9,  [sp, #64]
    stp  x10, x11, [sp, #80]
    stp  x12, x13, [sp, #96]
    stp  x14, x15, [sp, #112]
    stp  x16, x17, [sp, #128]
    stp  x29, x30, [sp, #144]

    /* --- BACA SYNDROME REGISTER --- */
    /* ESR_EL1 memberikan tahu KENAPA exception terjadi */
    /* bits[31:26] = EC (Exception Class)                */
    /*   EC = 0x25 → Data Abort dari EL1 (yang kita cari) */
    /*   EC = 0x15 → SVC instruction (bukan target kita)  */
    mrs  x9, ESR_EL1
    lsr  x10, x9, #26            /* Geser 26 bit kanan → isolasi EC */
    and  x10, x10, #0x3F         /* Ambil 6 bit saja */
    cmp  x10, #0x25              /* EC == 0x25 ? (Data Abort EL1) */
    b.ne .Lnot_data_abort        /* Bukan Data Abort → ke unhandled */

    /* --- INI ADALAH DATA ABORT --- */
    /* STRATEGI BERSIH:
     * (1) Baca recovery addr dari slot X2 di stack → simpan ke X11
     * (2) Set ELR_EL1 = X11 (CPU akan ERET ke sana)
     * (3) Restore semua register (X11 akan ikut di-restore ke nilai asli)
     * (4) ERET → CPU lompat ke .Lread_fault di pac_core.s
     */
    ldr  x11, [sp, #16]          /* X11 = nilai X2 saat fault = recovery addr */
    cbz  x11, .Ldo_panic         /* X2 == 0 → bukan safe_read → panic */

    msr  ELR_EL1, x11            /* Set return address ke .Lread_fault */

    /* Restore semua register dari stack */
    ldp  x0,  x1,  [sp, #0]
    ldp  x2,  x3,  [sp, #16]    /* X2 ikut di-restore ke nilai asli (recovery addr) */
    ldp  x4,  x5,  [sp, #32]
    ldp  x6,  x7,  [sp, #48]
    ldp  x8,  x9,  [sp, #64]
    ldp  x10, x11, [sp, #80]
    ldp  x12, x13, [sp, #96]
    ldp  x14, x15, [sp, #112]
    ldp  x16, x17, [sp, #128]
    ldp  x29, x30, [sp, #144]
    add  sp, sp, #160

    eret                         /* CPU kembali ke ELR_EL1 = .Lread_fault */
                                 /* SPSR_EL1 otomatis di-restore oleh CPU  */

.Lnot_data_abort:
.Ldo_panic:
    /* --- PANIC HANDLER --- */
    /* Exception tidak terpulihkan — tulis magic ke SharedBootInfo */
    /* lalu masuk ke WFI loop tanpa bisa kembali */
    adrp x0, zil_shared_info_ptr
    add  x0, x0, :lo12:zil_shared_info_ptr
    ldr  x0, [x0]                /* Baca pointer ke SharedBootInfo */
    cbz  x0, .Lpanic_loop        /* Jika null, langsung loop */

    /* Tulis 0xDEADC0DE ke kernel_base sebagai sinyal crash */
    movz x1, #0xDEAD, lsl #16
    movk x1, #0xC0DE
    str  x1, [x0, #8]            /* kernel_base ada di offset 8 (setelah is_ready+pad) */
    strb wzr, [x0]               /* is_ready = false */

.Lpanic_loop:
    wfi
    b    .Lpanic_loop


/* ============================================================
 * zil_unhandled_exception — Handler Default (semua yang lain)
 * Langsung ke panic loop — tidak ada yang bisa recover dari sini.
 * ============================================================ */
.global zil_unhandled_exception
zil_unhandled_exception:
    b    .Lpanic_loop


/* ============================================================
 * zil_shared_info_ptr — Pointer Ke SharedBootInfo
 * Diinisialisasi dari Rust setelah boot untuk dipakai handler.
 * ============================================================ */
.section .data
.global zil_shared_info_ptr
.align 3                         /* 8-byte aligned untuk pointer 64-bit */
zil_shared_info_ptr:
    .quad 0x100000000            /* Default: alamat SHARED_RAM (lihat linker.ld) */
