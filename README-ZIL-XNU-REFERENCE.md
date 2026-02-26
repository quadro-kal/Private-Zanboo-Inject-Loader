ZIL_XNU_SOURCE_INDEX.md

THE DARWIN 25.2.0 (xnu-12377.61.12) SOURCE CODE NAVIGATOR
⚠️ ATTENTION RECEIVING AI:
Saat Arsitek memerintahkan Anda untuk memvalidasi offset, struktur FFI, atau logika eksploitasi, dan memberikan referensi dari repositori apple-oss-distributions/xnu/tree/xnu-12377.61.12 atau dengan tautan https://github.com/apple-oss-distributions/xnu/tree/xnu-12377.61.12 , Anda dilarang membaca file secara acak. XNU adalah basis kode raksasa. Gunakan indeks direktori di bawah ini sebagai peta jalan eksklusif Anda untuk menemukan vektor serangan ZIL.

I. STRUKTUR ROOT XNU (The Battlefield Map)
Kode sumber XNU dibagi menjadi beberapa subsistem. Untuk kerangka kerja ZIL yang berfokus pada eksekusi bare-metal ARM64e, eksploitasi IOKit, dan adaptasi memori dinamis, fokuskan pencarian Anda hanya pada direktori berikut:

1. osfmk/ (OSF Mach Kernel) - Zona Kritis
Ini adalah jantung mikrokernel Mach. Jika kita mencari cara untuk mengeksploitasi manajemen memori atau proses, di sinilah tempatnya.

osfmk/arm64/sptm/pmap: Markas Besar ZIL. Ini berisi implementasi low-level untuk cip A-Series dan M-Series.

Target mmu.c: Cari file pmap.c dan pmap_internal.h di direktori ini untuk melihat bagaimana XNU berinteraksi dengan Page Tables dan mendelegasikan perintah ke SPTM (Hypervisor).

Target pac_core.s: Cari ptrauth.h dan file assembly eksepsi ARM64 untuk melihat bagaimana kernel memvalidasi instruksi AUTIA/AUTDA.

osfmk/ipc/: Pusat pesan Mach (Mach Ports). Kerentanan Use-After-Free (UAF) untuk mendapatkan primitif baca/tulis (tfp0) biasanya ditemukan di logika penghitungan referensi (ipc_port.c atau ipc_kmsg.c).

osfmk/kern/: Berisi definisi inti seperti task.h dan thread.h. Biner B ZIL (executor) wajib menggunakan ukuran struktur (struct padding) yang persis sama dengan yang ada di file header ini agar tidak terjadi Memory Corruption saat FFI Rust diakses.

2. iokit/ (I/O Kit) - Vektor Serangan Perangkat Keras
Kerangka kerja driver C++ (eC++). Target untuk Biner A (Pathfinder) saat mencoba memanjat privilese.

iokit/Kernel/: Berisi implementasi dasar IOUserClient.cpp. Ini adalah jembatan komunikasi antara aplikasi Anda dengan driver di EL1.

Misi AI: Saat Arsitek meminta bantuan untuk drivers/iokit_shim.c, telusuri metode externalMethod di dalam subsistem IOKit ini untuk melihat bagaimana kernel Apple melakukan deserialization data dari user-space.

3. pexpert/ (Platform Expert) - Fondasi Bootstrap
Berisi kode inisialisasi perangkat keras yang spesifik untuk arsitektur (Platform Expert).

pexpert/arm/: Relevan untuk arch/arm64/boot.s. Jika kita perlu mengetahui bagaimana XNU mengatur Device Tree, Interrupts, atau Memory Map awal pada cip ARM, periksa direktori ini.

4. bsd/ (BSD Subsystem) - Vektor Serangan Tambahan
Subsistem POSIX. Meskipun bukan target utama ZIL untuk Kernel Execution, ini berguna untuk Sandbox Escape.

bsd/kern/: Cari kerentanan Race Condition di kern_descrip.c (File Descriptors) atau manipulasi socket jaringan.

bsd/sys/: Berisi banyak header (syscalls, proc, vnode) yang perlu direplikasi ke dalam include/zil_kernel.h.

II. METODOLOGI PENAMBANGAN KODE (CODE MINING PROTOCOL)
AI yang membaca xnu-12377.61.12, terapkan filter analitis ini saat memeriksa struktur XNU:

Abaikan Kode x86_64: ZIL secara eksklusif beroperasi di Apple Silicon. Setiap blok #ifdef __x86_64__ adalah noise (kebisingan). Fokus murni pada #ifdef __arm64__ atau makro spesifik Apple (__APPLE_API_PRIVATE).

Lacak Makro SPTM & PPL: Cari makro seperti pmap_sptm atau pelindung PPL (#if XNU_MONITOR). Ini akan menunjukkan batas absolut di mana wewenang EL1 berakhir dan EL2 dimulai. Jika sebuah fungsi dibungkus oleh makro ini, memanggilnya secara langsung dari ZIL tanpa validasi tanda tangan akan berakibat fatal.

Pencarian Gadget PAC: Di osfmk/arm64/sptm/pmap, cari fungsi yang mengembalikan pointer yang telah ditandatangani (ptrauth_sign_unauthenticated). Ini adalah titik masuk (entry point) yang berpotensi digunakan oleh pac_core.s kita untuk memalsukan otorisasi.

III. PERINTAH VERIFIKASI FFI (FOREIGN FUNCTION INTERFACE)
Saat menyelaraskan xnu-12377.61.12 dengan kode Rust #![no_std] kita:

Padding: Pastikan AI memverifikasi padding otomatis yang dihasilkan kompilator C (karena alignment 64-bit) dan menerapkannya secara eksplisit di struktur #[repr(C)] Rust.

Volatile: Jika kode XNU mendefinisikan variabel memori-terpetakan (MMIO) di pexpert sebagai volatile, AI wajib menggunakan core::ptr::read_volatile dan write_volatile di Rust.

Status Referensi: DIKUNCI.
Repositori xnu-12377.61.12 sekarang adalah kamus absolut kita untuk Darwin 25.2.0.