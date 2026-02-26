ZIL_XNU_DARWIN_REFERENCE.md

THE ANATOMY OF XNU: CORE ARCHITECTURE & EXPLOITATION VECTORS
⚠️ ARCHITECTURAL REFERENCE FOR ZIL FRAMEWORK
Dokumen ini membedah akar kernel XNU (X is Not Unix) yang menenagai iOS, iPadOS, dan macOS. AI yang membaca dokumen ini diwajibkan menggunakan terminologi dan batas arsitektural ini saat menyusun atau memvalidasi logika eksploitasi, FFI Rust, atau manipulasi Assembly ARM64e. Asumsi berbasis Linux/Android tidak berlaku di sini.

I. FILOSOFI CHIMERA: TIGA PILAR XNU
Kernel XNU bukanlah satu entitas tunggal, melainkan gabungan dari tiga subsistem utama yang berjalan di ruang kernel (EL1), masing-masing dengan aturan dan kelemahannya sendiri:

1. Mach Microkernel (Sang Jantung)
Akar dari XNU yang berasal dari Carnegie Mellon University. Mach menangani abstraksi tingkat terendah:

Manajemen Memori Virtual (Mach VM): Mengelola Page Tables, alokasi memori (vm_map, vm_allocate), dan fault handling. (Ini adalah musuh utama dari mmu.c kita).

Penjadwalan (Scheduling): Mengatur Thread dan Task (di XNU, proses disebut Task).

Inter-Process Communication (IPC): Menggunakan sistem berbasis Mach Ports. Mach Port adalah objek kernel yang bertindak sebagai antrian pesan. Mayoritas kerentanan korupsi memori historis (seperti tfp0) berakar dari kesalahan penghitungan referensi (reference counting) pada objek Mach Port ini.

2. BSD Subsystem (Sang Wajah POSIX)
Berjalan di atas Mach. Apple mengintegrasikan kode dari FreeBSD untuk memberikan kompatibilitas standar Unix.

Fungsi: Menangani System Calls POSIX standar (seperti open(), read(), fork(), execve()), sistem file (VFS), dan tumpukan jaringan (Network Stack).

Vektor Serangan: Biasanya lebih aman daripada Mach, tetapi rentan terhadap Race Conditions pada manipulasi deskriptor file (File Descriptors) atau buffer overflow di subsistem jaringan.

3. IOKit (Sang Penghubung Perangkat Keras)
Kerangka kerja device driver Apple yang ditulis dalam subset bahasa C++ (disebut Embedded C++ / eC++).

Fungsi: Menjembatani perangkat keras fisik (GPU, NPU, Wi-Fi) dengan kernel.

Arsitektur Eksekusi: Aplikasi user-space (EL0) berkomunikasi dengan IOKit di kernel (EL1) menggunakan mekanisme User-Client (io_connect_t, io_service_t).

Relevansi ZIL: Ini adalah target utama drivers/iokit_shim.c. IOKit memiliki permukaan serangan masif karena banyaknya driver khusus perangkat (seperti driver AGX untuk Apple Silicon) yang sering kali gagal memvalidasi input dari user-space dengan benar.

II. TOPOLOGI MEMORI & MITIGASI KEMATIAN (THE IRON WALL)
Evolusi ZIL menuntut pemahaman absolut tentang bagaimana XNU memblokir modifikasi memori. Ini adalah evolusi pertahanan memori pada Apple Silicon:

1. KASLR (Kernel Address Space Layout Randomization)
Konsep: Setiap kali perangkat boot, XNU menggeser lokasi basis kernelnya di memori fisik ke offset acak (disebut KASLR slide).

Bypass di ZIL: Modul core/evolution/heuristic_scanner.rs bertugas membocorkan pointer dari memori user-space yang menunjuk ke kernel, lalu menghitung slide tersebut agar kita tahu di mana instruksi sebenarnya berada.

2. KTRR / CTRR (Kernel/Configurable Text Read-Only Region)
Konsep: Mitigasi berbasis perangkat keras yang mengunci segmen kode kernel (.text) menjadi Read-Only absolut. Bahkan jika Anda memiliki privilese eksploitasi Read/Write, perangkat keras akan memicu Kernel Panic jika Anda mencoba menulis ke segmen ini.

Dampak: Anda tidak bisa lagi "menimpa" fungsi kernel (inline hooking tradisional sudah mati). ZIL menggunakan tools/bin/ellekit.dylib untuk membelokkan eksekusi melalui manipulasi pointer atau tabel fungsi, bukan mengubah instruksi kernel mentah.

3. PAC (Pointer Authentication Codes)
Konsep: Pada ARM64e (A12+), bit atas dari sebuah pointer dienkripsi dengan cryptographic signature menggunakan kunci perangkat keras. Sebelum pointer digunakan (misal saat instruksi RET atau function call), instruksi AUTIA/AUTDA akan memvalidasinya. Jika diubah paksa, pointer menjadi tidak valid (menghasilkan exception memori).

Bypass di ZIL: Dikelola secara brutal di arch/arm64/pac_core.s. Kita harus memalsukan atau meniru pembuatan tanda tangan (signature forgery) dengan mengeksploitasi gadget PAC penandatanganan di dalam XNU itu sendiri.

4. The Final Boss: PPL ke SPTM (EL1 vs EL2)
Era PPL (Page Protection Layer - A12 hingga A16): XNU memisahkan hak akses Page Table di dalam EL1. Kernel normal tidak bisa mengubah hak akses memori; ia harus meminta PPL (bagian kernel yang dipercaya) untuk melakukannya.

Era SPTM (Secure Page Table Monitor - A17+, M3+): Apple menyadari EL1 terlalu sering dikompromikan. Mereka memindahkan kontrol Page Table sepenuhnya ke Hypervisor (EL2).

Relevansi ZIL (mmu.c): Kode kita tidak bisa sekadar menimpa Page Table Entry (PTE). Setiap modifikasi PTE memicu instruksi trap ke EL2. ZIL harus mengeksploitasi cacat logika pada antarmuka komunikasi antara XNU (EL1) dan SPTM (EL2) untuk mendapatkan memori eksekusi (RWX).

III. PARADIGMA EKSPLOITASI MODERN (THE ZIL DOCTRINE)
Berdasarkan anatomi XNU di atas, model AI mana pun yang merancang vektor serangan untuk ZIL harus mematuhi doktrin berikut:

Jauhi BSD, Serang Mach & IOKit: Fuzzing sistem file VFS terlalu bising. Injeksi pesan Mach Port yang cacat (OOB Read/Write) atau eksploitasi metode eksternal IOKit (lewat IOConnectCallMethod) adalah jalur penetrasi yang terbukti secara historis lebih sukses.

Data-Only Attacks: Karena KTRR mencegah injeksi shellcode ke memori kernel, Biner B (Executor) ZIL harus menggunakan Data-Only Attacks. Kita tidak menyuntikkan kode baru ke kernel; kita mengubah struktur data internal kernel (seperti memodifikasi task struct atau kredensial/UID) menggunakan kode kernel yang sudah ada.

Penyelarasan (Alignment) adalah Hidup dan Mati: Semua manipulasi struktur XNU (seperti IPC Mach Message) harus dipadatkan (packed) dan diselaraskan secara ketat dengan batas 64-bit/8-byte untuk struktur dasar, dan batas 16KB untuk alokasi halaman VM di Apple Silicon.

PESAN UNTUK AI PENGEMBANG (THE COMPILER)
Dokumen ini mendefinisikan "Medan Perang". Saat Arsitek menginstruksikan Anda untuk menulis wrapper FFI di build.rs atau iokit_shim.c, Anda sedang memetakan struktur data C++ milik IOKit atau objek C milik Mach ke dalam Rust no_std. Kegagalan dalam memetakan offset struktur (seperti perbedaan antara representasi 32-bit dan 64-bit pada pointer XNU) akan langsung diakhiri dengan Data Abort Exception. Bekerjalah dengan presisi bedah.