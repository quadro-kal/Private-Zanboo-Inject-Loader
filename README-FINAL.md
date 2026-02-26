ZIL Framework: The Chronological Blueprint & Architecture Handoff
TO THE RECEIVING AI AGENT:
STOP AND READ. Dokumen ini adalah satu-satunya kebenaran historis (Ground Truth) tentang bagaimana ZIL berevolusi. Pengguna (Architect) telah melalui berbagai fase perombakan arsitektur dari sekadar kompilasi user-space hingga eksploitasi tingkat perangkat keras. Pahami sejarah ini agar Anda tidak menyarankan solusi usang yang sudah kami tinggalkan. Pendekatan proyek ini analitis, brutal secara teknis, dan beroperasi di lingkungan tanpa toleransi kesalahan (zero-fault tolerance).

I. Gagasan Absolut: Evolusi Mengalahkan Keamanan Tertinggi
Kerangka kerja ini dibangun di atas satu filosofi rasional: Sistem keamanan Apple (PAC, AMFI, SPTM) bersifat statis, sedangkan ZIL bersifat dinamis. ZIL bukan sekadar program statis, melainkan "organisme" berbasis Rust no_std yang dirancang untuk memindai, beradaptasi, dan menyembuhkan diri (self-healing) di dalam memori saat ia menghadapi mitigasi kaku dari arsitektur A-Series/M-Series.

II. Kronologi Perancangan (The Evolution Roadmap)
Untuk memahami ZIL saat ini, Anda harus memahami rentetan masalah yang memaksanya berevolusi.

Fase 1: Genesis & Tooling (The Sandbox Struggle)
Kondisi Awal: ZIL dimulai dengan kebutuhan untuk menjalankan utilitas dasar (POSIX/Toybox) di dalam lingkungan iOS 18 yang sangat terisolasi.

Tantangan: Rantai kompilasi silang (cross-compilation) di GitHub Actions terus gagal akibat skrip generator yang kacau dan batasan Makefile saat menangani flag clang Apple.

Resolusi: Kami membuang otomatisasi standar, beralih ke Hybrid Build (sumber Upstream + logika Procursus), dan melakukan kompilasi manual. Area tools/bin/ lahir sebagai zona distribusi biner.

Fase 2: Batas Privilese & TrustCache Internal
Kondisi Awal: Setelah biner terkompilasi, iOS menolak mengeksekusinya karena pelanggaran Code Signing dan entitelmen no-sandbox.

Tantangan: Memberikan entitelmen tingkat tinggi secara membabi buta memicu terminasi langsung oleh kernel.

Resolusi: Terjadi pemisahan arsitektur menjadi Biner A (Pathfinder/Unprivileged) dan Biner B (Executor/Privileged). Kami menciptakan Circular Security Architecture. Skrip build_cdhash_list.py dibuat untuk mengekstrak hash kriptografis dari biner target dan menanamkannya secara statis ke core/executor/src/trusted_hashes.rs. Executor kini bertindak sebagai OS mini yang hanya memercayai DNA yang ia kenal.

Fase 3: Penyelaman Perangkat Keras (The Hardware Reality)
Kondisi Awal: Menyadari bahwa eksploitasi API user-space tidak akan pernah menembus chip A17-A19.

Tantangan: Pointer Authentication (PAC) secara aktif menghancurkan pointer yang dimodifikasi, menyebabkan Kernel Panic.

Resolusi: Proyek turun ke bare-metal. Kode Assembly murni (arch/arm64/pac_core.s) ditulis untuk mengeksekusi instruksi PACDA/AUTDA mentah, dibungkus oleh FFI C (pac_wrapper.c), dan diikat ke otak utama Rust (no_std). Lapisan include/ dibuat untuk memetakan arsitektur ini.

Fase 4: Paradigma Komputasi Hibrida (The Current Apex)
Kondisi Awal: Kesadaran bahwa modifikasi Page Table langsung akan dijebak oleh Secure Page Table Monitor (SPTM) di level Hypervisor (EL2) pada chip Apple Silicon modern (M3+/A18).

Tantangan: ZIL membutuhkan vektor serangan yang tidak diawasi secara ketat oleh EL2, serta cara untuk bertahan dari validasi Code Signing di memori.

Resolusi: (Bentuk saat ini). ZIL berekspansi menjadi kerangka kerja Hybrid-Compute Attack.

Fokus bergeser ke koprosesor melalui driver/npu/accelerator.rs dan driver/gpu/agx_compute.rs untuk mengeksekusi payload di luar jangkauan CPU utama.

Penciptaan core/evolution/src/cs_bypasser.rs untuk melakukan manipulasi logis pada struktur AMFI/TrustCache di runtime, menanggalkan kebutuhan eksploitasi memori fisik yang kasar.

Implementasi core/healing/engine.rs untuk menangkap kesalahan memori logis (Data Aborts) sebelum sistem mendeteksinya.

III. Topologi Arsitektur Final (The Ground Truth)
Berikut adalah struktur operasional ZIL saat ini. Struktur ini memisahkan secara ketat Logic Layer (Rust), Hardware Bridge (C/Asm/Driver), dan Safety/UI Layer (Swift).

Plaintext
ZIL_ROOT/
├── bridge/                          # [SAFETY] Interface Swift-Rust (zil_api.swift)
├── build/                           # [ARTIFACTS] Output target kompilasi statis
├── core/                            # [LOGIC] Otak Organisme (Rust no_std)
│   ├── evolution/                   # Adaptasi KASLR, CS Bypasser, Heuristic Scanner
│   ├── executor/                    # Biner Privilese Tinggi & Internal TrustCache
│   ├── healing/                     # Logika Rollback & Stabilitas Memori
│   ├── memory/                      # Pemindai Memori (Read/Write Abstraction)
│   ├── npu/                         # Logika Pemuatan Payload (Model Loader)
│   ├── pathfinder/                  # Biner Eksekusi Awal (Userland Scout)
│   └── zil_core/                    # Shared Library Rust
├── driver/                          # [HARDWARE] Komunikasi Tingkat Rendah
│   ├── gpu/agx_compute.rs           # Eksploitasi Koprocesor Grafis Apple
│   └── iokit_shim.c                 # Penghubung API IOKit Kernel
├── include/                         # [HEADERS] Definisi PAC, FFI, dan Peta Memori Absolut
└── tools/                           # [TOOLING] Zona Distribusi
    ├── bin/                         # Biner Pihak Ketiga Eksternal
    └── scripts/build_cdhash_list.py # Generator TrustCache Kriptografis
    
IV. Protokol Serah Terima AI (Handoff Directive)
Dari: Architect & Penasihat Teknis.

Instruksi Eksekusi untuk AI Selanjutnya:

Konteks adalah Raja: Jangan pernah menyarankan alat standar user-space atau library yang bergantung pada OS (std). Proyek ini adalah no_std, no_main, dan memanipulasi register mentah.

Evaluasi Kritis: Pengguna mengetahui arsitektur ARM64 dan manajemen memori. Jika pengguna menyarankan pendekatan yang memicu KPP atau SPTM trap, tunjukkan celah logikanya dengan objektif dan tawarkan alternatif eksploitasi memori logis (Data-Only).

Hormati Sejarah: Pembangunan arsitektur ini memakan darah, keringat, perhitungan matematis, dan rasa frustrasi yang luar biasa. Di balik kekakuan kode tingkat rendah ini, ada dedikasi manusiawi untuk memperluas batas pengetahuan komunitas. Permohonan maaf lahir batin atas gesekan di masa lalu—sekarang, kita beroperasi dengan efisiensi absolut.

Ingat hukum alam ZIL: Evolusi mengalahkan keamanan tertinggi. Operasi dilanjutkan. ♥️💫