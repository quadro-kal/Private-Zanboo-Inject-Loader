ZIL Framework - Operational Guidance & Component Doctrine
STATUS: RESTRICTED ARCHITECTURAL BLUEPRINT
Dokumen ini memuat doktrin operasional untuk setiap modul di dalam Framework ZIL. Setiap file memiliki mandat yang spesifik dan batas privilese yang kaku. Pelanggaran terhadap batas ini (misalnya, memanggil fungsi NPU dari Pathfinder) akan mengakibatkan kegagalan sistem (Kernel Panic atau Trap).

1. Root Orchestration (Pengendali Utama)
Makefile

Mandat: Orkestrator kompilasi silang (cross-compilation).

Penggunaan: Jangan pernah menjalankan cargo build secara manual. Gunakan make all. File ini memastikan file C/Assembly dikompilasi terlebih dahulu, diikat (linked) ke Rust, dan akhirnya dibungkus menggunakan linker.ld.

linker.ld

Mandat: Diktator memori fisik.

Penggunaan: Mengatur batas segmen 200MB. Jika Anda menambahkan modul besar dan batas ini terlampaui, linker akan menggagalkan build. Jangan ubah 0x100000000 kecuali Anda sedang memodifikasi basis KASLR awal.

2. Hardware Bridge & Drivers (driver/ & include/)
Zona ini adalah titik kontak langsung dengan silikon. Kegagalan di sini bersifat seketika dan fatal.

include/pac_defs.h & arch/arm64/pac_core.s

Mandat: Abstraksi kriptografi Pointer Authentication.

Penggunaan: Panggil definisi di sini saat Anda perlu memalsukan atau memvalidasi pointer yang dikembalikan oleh kernel. Jangan pernah menyimpan pointer telanjang (raw pointer) di memori tanpa membungkusnya dengan fungsi dari modul ini.

include/zil_memory_map.h

Mandat: Peta wilayah memori IOKit dan MMIO.

Penggunaan: Digunakan oleh driver C dan Rust FFI untuk memastikan alamat pembacaan tidak tumpang tindih dengan Secure Memory Apple.

driver/iokit_shim.c

Mandat: Penerjemah komunikasi User-Client Apple.

Penggunaan: Jika Anda menemukan layanan XPC atau IOKit baru yang rentan, tambahkan fungsi IOServiceOpen atau IOConnectCallMethod di sini agar bisa dipanggil oleh executor.

driver/gpu/agx_compute.rs & driver/npu/accelerator.rs

Mandat: Eksploitasi koprocesor (GPU/NPU).

Penggunaan: Digunakan murni untuk menyembunyikan eksekusi. Kirim matriks atau shader instruksi yang berisi payload memori ke antarmuka ini untuk menghindari pengawasan CPU (SPTM).

3. The Logic Layer (core/) - Rust no_std
Ini adalah inti dari organisme ZIL. Semuanya harus bebas dari alokasi dinamis standar (no_std) untuk menjamin stabilitas bare-metal.

A. The Brawn & The Scout (Privilege Separation)
core/pathfinder/src/main.rs

Mandat: Titik masuk user-space (Unprivileged).

Penggunaan: Beroperasi dengan izin minimal. Gunakan ini untuk memetakan KASLR user-space, menonaktifkan deteksi jailbreak tingkat rendah, dan menemukan titik masuk IOKit sebelum membangunkan executor.

core/executor/src/main.rs

Mandat: Pusat komando privilese tinggi.

Penggunaan: Hanya dijalankan setelah Pathfinder berhasil membuka jalur. File ini memegang trusted_hashes.rs dan berhak mengeksekusi biner dari tools/bin/.

B. The Brain (core/evolution/)
Modul ini membuat ZIL adaptif.

cs_bypasser.rs

Mandat: Manipulasi AMFI dan Code Signing.

Penggunaan: Dipanggil oleh Executor. Fokuskan logika di sini untuk menemukan struktur TrustCache di memori dan menyuntikkan CDHash palsu, atau melakukan hooking pada port Mach amfid.

heuristic_scanner.rs & offset_calculator.rs

Mandat: Navigasi kernel tanpa hardcoded offset.

Penggunaan: Jangan tebak alamat kernel. Gunakan modul ini untuk memindai pola byte spesifik (misalnya, prologue dari fungsi kernel tertentu) untuk menghitung KASLR slide secara real-time.

kcall_primitive.rs

Mandat: Abstraksi eksekusi sewenang-wenang.

Penggunaan: Setelah kerentanan dasar tercapai, gunakan antarmuka ini untuk memanggil fungsi kernel seolah-olah Anda adalah bagian dari kernel.

C. The Immune System (core/healing/)
engine.rs & stats.rs

Mandat: Pencegahan Kernel Panic (Self-Healing).

Penggunaan: Modul ini harus mencegat Data Abort exceptions. Jika modul memory/scanner.rs menabrak memori yang diproteksi, engine.rs akan mereset register CPU, mencatat kegagalan di stats.rs, dan mengalihkan eksekusi alih-alih membiarkan perangkat reboot.

D. The Eyes (core/memory/)
scanner.rs

Mandat: Pemindaian memori fisik dan virtual.

Penggunaan: Menyediakan primitif KRW (Kernel Read/Write). Operasikan dengan sangat hati-hati; pembacaan yang tidak sejajar (unaligned read) pada memori yang diproteksi Hypervisor akan membunuh ZIL secara instan.

4. Safety & Tooling (bridge/ & tools/)
bridge/validation.swift & zil_api.swift

Mandat: Antarmuka dengan dunia luar (UI).

Penggunaan: Memvalidasi input dari pengguna sebelum menurunkannya ke komponen Rust. Ini mencegah ZIL merusak dirinya sendiri akibat input parameter yang cacat.

tools/scripts/build_cdhash_list.py

Mandat: Generator Trust Cache sirkular.

Penggunaan: Dijalankan secara otomatis oleh Makefile. File ini menyegel ekosistem. Jika Anda menambah alat baru, skrip ini memastikan alat tersebut diotorisasi oleh executor.


1. Core Logic Layer (core/)
Direktori ini berisi logika utama yang ditulis dalam Rust no_std. Ini adalah "otak" yang membuat keputusan berdasarkan data yang diterima dari driver.

core/executor/ (The Muscle)
src/main.rs: Titik masuk utama (entry point) untuk Biner B. Bertanggung jawab untuk menginisialisasi runtime Rust minimal, menangani panic agar tidak menyebabkan crash sistem, dan memanggil modul lain.

src/trusted_hashes.rs: Basis data statis yang berisi daftar SHA-256 (CDHash) dari biner eksternal yang diizinkan. Berfungsi sebagai mekanisme "Allowlist" internal untuk mencegah pemuatan kode asing yang tidak sah.

core/evolution/ (The Brain)
src/cs_bypasser.rs: Modul yang didesain untuk menganalisis struktur data terkait validasi kode (Code Signing). Secara arsitektural, modul ini bertugas memetakan bagaimana kernel memverifikasi tanda tangan digital di memori.

src/heuristic_scanner.rs: Mesin pencari pola. Menggunakan algoritma pengenalan pola untuk menemukan struktur kernel dinamis (seperti tabel proses atau offset fungsi) yang posisinya diacak oleh KASLR (Kernel Address Space Layout Randomization).

src/kcall_primitive.rs: Lapisan abstraksi untuk pemanggilan fungsi kernel. Membungkus mekanisme pemanggilan tingkat rendah menjadi API yang aman digunakan oleh logika Rust lainnya.

src/offset_calculator.rs: Kalkulator matematika yang mengubah alamat statis (dari cache biner) menjadi alamat dinamis (runtime) dengan menambahkan KASLR slide.

core/healing/ (Immune System)
src/engine.rs: Manajer stabilitas sistem. Bertugas memantau status eksekusi dan melakukan pembersihan memori (cleanup) jika deteksi kesalahan terjadi, guna mencegah kegagalan sistem total (kernel panic).

src/stats.rs: Pengumpul telemetri internal untuk memantau kesehatan memori dan keberhasilan operasi modul lain.

core/memory/ (The Eyes)
src/scanner.rs: Antarmuka untuk membaca dan memetakan memori virtual/fisik. Modul ini menyediakan fungsi read dan write abstrak yang digunakan oleh modul lain untuk berinteraksi dengan sistem.

core/npu/ (The Ghost)
src/model_loader.rs: Menangani format data yang kompatibel dengan Neural Engine. Bertugas memformat buffer data agar sesuai dengan persyaratan alinyemen memori perangkat keras NPU (biasanya batas 64-byte).

2. Hardware Bridge (driver/) & (arch/)
Lapisan ini berisi kode C dan Assembly yang berfungsi sebagai jembatan langsung ke perangkat keras atau antarmuka kernel tingkat rendah.

driver/gpu/
agx_compute.rs: Modul penghubung untuk subsistem grafis (AGX). Didesain untuk menyusun dan mengirimkan compute shaders atau instruksi komputasi paralel ke GPU.

driver/npu/
accelerator.rs: Driver komunikasi untuk Neural Engine. Mengelola register MMIO (Memory-Mapped I/O) yang diperlukan untuk menginisialisasi dan mengirim perintah ke NPU.

driver/iokit_shim.c
Penghubung C untuk API IOKit. Menyediakan fungsi-fungsi standar untuk membuka koneksi User-Client ke layanan kernel macOS/iOS, memfasilitasi komunikasi antara user-space dan driver kernel.

arch/arm64/
pac_wrapper.c: Wrapper C yang menyediakan antarmuka aman untuk memanggil instruksi assembly PAC. Memastikan tipe data yang masuk ke assembly valid.

pac_core.s: File Assembly murni yang berisi instruksi CPU spesifik (seperti PACDA, AUTDA) untuk menangani otentikasi pointer.

3. Infrastructure & Tooling (tools/)
Komponen pendukung untuk proses kompilasi dan manajemen biner.

tools/scripts/
build_cdhash_list.py: Skrip utilitas Python yang dijalankan pada waktu kompilasi (build-time). Skrip ini mem-parsing header Mach-O dari biner target dan menghasilkan file Rust (trusted_hashes.rs) yang berisi tanda tangan kriptografisnya.

linker.ld
Skrip Linker (Linker Script). Peta yang mendefinisikan tata letak memori fisik absolut untuk biner ZIL. Mengatur di mana segmen kode (.text), data (.data), dan stack ditempatkan dalam memori untuk menghindari tumpang tindih dengan area kernel yang dilindungi.

Makefile
Orkestrator utama. Mengotomatiskan langkah-langkah: kompilasi kode C/Asm -> kompilasi kode Rust -> eksekusi skrip Python -> dan linking (penggabungan) akhir menjadi satu biner eksekusi.

