/* ========================================================
 * ZIL FRAMEWORK: MASTER LINKER SCRIPT
 * Target: Aarch64-apple-darwin (A19 Pro Bare-metal)
 * ======================================================== */

ENTRY(_start)

/* --- PEMBAGIAN ZONA FISIK (PETA TANAH) --- */
MEMORY
{
    /* 1. KOTAK SURAT (16 KB)
     * Area komunikasi absolut antara Pathfinder dan Executor. 
     * Dialokasikan persis 1 Page Apple Silicon (16384 bytes).
     */
    SHARED_RAM (rw)  : ORIGIN = 0x100000000, LENGTH = 16K
    
    /* 2. THE BRAIN / LOGIC SPACE (~200 MB minus 16KB)
     * Tempat tinggal Rust, PAC Logic, dan NPU Engine.
     */
    LOGIC_RAM  (rwx) : ORIGIN = 0x100004000, LENGTH = 200M - 16K
    
    /* 3. THE MUSCLE / TOOLING SPACE (200 MB)
     * Zona karantina untuk NPU Weights, BusyBox, dan ElleKit.
     */
    TOOL_RAM   (rw-) : ORIGIN = 0x10C800000, LENGTH = 200M
}

/* --- PEMETAAN SEGMEN KODE KE ZONA FISIK --- */
SECTIONS
{
    /* Memaksa struktur SharedBootInfo berada di alamat 0x100000000 */
    .shared_info : {
        KEEP(*(.shared_info))
    } > SHARED_RAM

    /* KODE EKSEKUSI (Instructions) */
    .text : ALIGN(16384) {
        __text_start = .;
        /* Memaksa titik masuk boot.s/pac_core.s berada di paling depan */
        KEEP(*(.text.entry)) 
        *(.text .text.*)
        __text_end = .;
    } > LOGIC_RAM

    /* DATA READ-ONLY (String, Konstanta Hash) */
    .rodata : ALIGN(16384) {
        __rodata_start = .;
        *(.rodata .rodata.*)
        __rodata_end = .;
    } > LOGIC_RAM

    /* DATA GLOBAL (Read/Write) */
    .data : ALIGN(16384) {
        __data_start = .;
        *(.data .data.*)
        __data_end = .;
    } > LOGIC_RAM

    /* BSS & STACK ALLOCATION */
    .bss : ALIGN(16384) {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        
        /* * STRATEGI STACK EKSPLISIT 
         * Di bare-metal, OS tidak mengurus stack Anda. 
         * Kita wajib memesan 2MB ruang kosong di ujung RAM untuk stack CPU.
         */
        . = ALIGN(16384);
        __stack_bottom = .;
        . = . + 2M; 
        __stack_top = .;
        
        __bss_end = .;
    } > LOGIC_RAM

    /* ZONA TOOLING (Tidak dimuat oleh Linker secara langsung) */
    .tooling_arena (NOLOAD) : ALIGN(16384) {
        __npu_arena_start = .;
        /* Alokasi 10MB khusus untuk Bobot NPU (64-byte aligned implicitly by 16K) */
        . = . + 10M; 
        __npu_arena_end = .;
        
        /* Sisa 190MB untuk BusyBox dan Tweak */
        __busybox_start = .;
    } > TOOL_RAM

    /* PENGHANCURAN ARTEFAK SAMPAH 
     * Clang sering memasukkan metadata C++ (eh_frame) yang tidak berguna 
     * di kernel dan hanya menghabiskan kuota 200MB kita. Kita buang.
     */
    /DISCARD/ : {
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.comment)
        *(.note*)
    }
}