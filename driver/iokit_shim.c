/* ========================================================
 * ZIL FRAMEWORK: IOKIT SHIM — REVISI v2.0 (MAN-C + MAN-D)
 * ========================================================
 *
 * PERUBAHAN v2.0 (Roadmap v1.5):
 *   MAN-D: Tambah iokit_open_ane_client() / iokit_close_ane_client()
 *          Open/close koneksi ke AppleH11ANEInterface dari kernel side.
 *   MAN-C: Tambah iokit_probe_ane_vtable_index()
 *          Probe VTable ANE secara runtime — tidak lagi hardcode index 7.
 *
 * CATATAN ARSITEKTUR:
 *   ZIL berjalan di EL1 (kernel mode). IOKit service lookup dari kernel
 *   menggunakan IOKit kernel C++ API melalui indirect Mach port mechanism.
 *   Karena kita no_std, IOKit header tidak bisa di-include langsung.
 *   Kita gunakan pointer function table yang ditemukan via symbol scan.
 * ======================================================== */

#include <stdint.h>

/* Variabel global untuk menyimpan vtable index terkini.
 * Default = 7 (nilai A18/A19 berdasarkan riset komunitas).
 * Akan diperbarui oleh iokit_probe_ane_vtable_index() saat runtime. */
static uint64_t g_vtable_index = 7;

/* Pointer ke ANE IOUserClient object yang sedang aktif */
static uint64_t g_ane_client_ptr = 0;

/* --------------------------------------------------------
 * iokit_set_dynamic_vtable_index()
 * DIPANGGIL DARI RUST (executor/main.rs).
 * Menerima index hasil probe dan menyimpannya ke global.
 * -------------------------------------------------------- */
void iokit_set_dynamic_vtable_index(uint64_t idx) {
    if (idx >= 5 && idx <= 20) {  /* Validasi range masuk akal */
        g_vtable_index = idx;
    }
}

typedef uint64_t (*ExternalMethodFunc)(void* client, void* args, void* dispatch, void* target, void* ref);

/* --------------------------------------------------------
 * MAN-D: iokit_open_ane_client()
 *
 * Temukan dan buka IOKit ANE UserClient dari kernel.
 *
 * STRATEGI:
 *   Dari kernel (EL1), kita tidak bisa langsung pakai IOServiceOpen()
 *   (itu API user-space). Alternatifnya:
 *
 *   1. Scan IOKit registry menggunakan gIOFBDependencies atau
 *      gIOCommonPageTable yang sudah di-map di kernel.
 *   2. Walk linked list IOService dari gIOKitRegistryRoot.
 *   3. Match service name "AppleH11ANEInterface" dengan string compare.
 *
 *   Implementasi disini menggunakan approach kernel pointer walk:
 *   - Asumsikan `_gIOServiceRoot` ada di kernel (symbol di kernelcache)
 *   - Walk children list → temukan ANE node → ambil defaultClient
 *
 * RETURN: pointer ke IOUserClient object, 0 jika gagal.
 *
 * NOTE: Dalam implementasi ini kita scan region IOKit dari kernel_base.
 *       Nilai real butuh konfirmasi offset gIOServiceRoot dari kernelcache.
 *       Jika scan gagal, fungsi mengembalikan 0 (non-fatal — executor skip NPU).
 * -------------------------------------------------------- */
uint64_t iokit_open_ane_client(void) {
    /* ── TAHAP 1: Coba akses via well-known IOKit kernel structure ──────
     *
     * Di Darwin 25 (xnu-12377), gRegistryRoot ada di data segment.
     * Kita tidak bisa resolve symbol dari sini tanpa kernelcache analysis.
     *
     * STRATEGI ALTERNATIF: Scan kernel data segment untuk string
     * "AppleH11ANEInterface" yang merupakan class name ANE driver.
     * IOKit menyimpan service info di registry yang bisa ditemukan
     * dengan scan keyword di kernel data region.
     *
     * Jika scan berhasil → kita punya pointer ke IOService node ANE
     * dan bisa ambil defaultUserClient dari sana.
     *
     * PLACEHOLDER: Hasil scan disimpan ke g_ane_client_ptr.
     * Implementasi penuh butuh offset gRegistryRoot dari kernelcache.
     */

    /* ── TAHAP 2: Fallback — gunakan pointer yang mungkin sudah di-setup
     * oleh inisialisasi driver ANE jika ada di MMIO map kita.
     *
     * Address default ANE IOUserClient dari profile A19 xnu-12377:
     * Ini adalah PLACEHOLDER — ganti dengan nilai dari live scan.
     */

    /* Untuk saat ini, kembalikan 0 → executor tahu ANE client belum tersedia */
    /* Implementasi penuh via kernel registry walk ditambahkan setelah
     * iokit_find_ane_service_by_scan() selesai diimplementasi (perlu
     * kernel_base dari Rust untuk mulai scan). */

    /* MAN-D STUB: Nilai akan diisi dari Rust executor yang memanggil
     * iokit_set_ane_client_from_scan() setelah scan berhasil. */
    return g_ane_client_ptr;
}

/* --------------------------------------------------------
 * iokit_set_ane_client_from_scan()
 *
 * Dipanggil dari Rust setelah kernel scan berhasil menemukan
 * alamat IOUserClient ANE. Scanner Rust yang punya akses ke
 * kernel_base bisa resolve IOKit registry lebih mudah.
 * -------------------------------------------------------- */
void iokit_set_ane_client_from_scan(uint64_t client_ptr) {
    g_ane_client_ptr = client_ptr;
}

/* --------------------------------------------------------
 * iokit_close_ane_client()
 *
 * Tutup koneksi ANE UserClient dan clear pointer.
 * -------------------------------------------------------- */
void iokit_close_ane_client(uint64_t client_ptr) {
    (void)client_ptr;
    g_ane_client_ptr = 0;
    g_vtable_index   = 7; /* Reset ke default */
}

/* --------------------------------------------------------
 * MAN-C: iokit_probe_ane_vtable_index()
 *
 * Probe VTable ANE IOUserClient secara runtime untuk menemukan
 * index selector yang benar tanpa hardcode.
 *
 * ALGORITMA:
 *   1. Ambil VTable pointer dari 8 byte pertama objek C++ (standard ABI)
 *   2. Scan slot VTable dari index 5 sampai 20
 *   3. Untuk setiap slot:
 *      a. Baca pointer di slot tersebut
 *      b. Validasi: harus non-null, harus berada di range kernel text
 *         (antara ktext_start dan ktext_end)
 *      c. Hitung berapa slot valid berurutan — ANE selector biasanya
 *         ada di cluster 3-4 slot valid berurutan (alloc/submit/free/ready)
 *   4. Return index slot pertama dari cluster yang paling mungkin
 *      (biasanya slot ke-7 sampai ke-9 dari ANE VTable)
 *
 * PARAMETER:
 *   client_ptr   — pointer ke IOUserClient object ANE
 *   ktext_start  — awal kernel __TEXT segment (= kernel_base)
 *   ktext_end    — akhir kernel __TEXT segment (= kernel_base + ~32MB)
 *
 * RETURN: index VTable yang diprediksi sebagai allocateComputeBuffer,
 *         atau 7 sebagai fallback default.
 * -------------------------------------------------------- */
uint64_t iokit_probe_ane_vtable_index(uint64_t client_ptr,
                                       uint64_t ktext_start,
                                       uint64_t ktext_end) {
    if (!client_ptr) return 7; /* fallback */

    /* Ambil VTable pointer dari byte 0 objek */
    uint64_t* obj = (uint64_t*)client_ptr;
    uint64_t* vtable = (uint64_t*)(*obj);
    if (!vtable) return 7;

    /* Scan slot 5..20, cari cluster valid */
    int     cluster_len      = 0;
    int     best_cluster_len = 0;
    uint64_t best_cluster_start = 7; /* default index */

    for (int i = 5; i <= 20; i++) {
        uint64_t slot_val = vtable[i];

        /* Cek apakah pointer berada di kernel text range */
        int valid = (slot_val != 0) &&
                    (slot_val >= ktext_start) &&
                    (slot_val <  ktext_end);

        if (valid) {
            cluster_len++;
        } else {
            /* End of cluster — simpan jika ini cluster terpanjang */
            if (cluster_len >= 3 && cluster_len > best_cluster_len) {
                best_cluster_len   = cluster_len;
                /* Titik awal cluster: i - cluster_len */
                best_cluster_start = (uint64_t)(i - cluster_len);
            }
            cluster_len = 0;
        }
    }

    /* Cek cluster terakhir jika belum disimpan */
    if (cluster_len >= 3 && cluster_len > best_cluster_len) {
        best_cluster_start = (uint64_t)(20 - cluster_len + 1);
    }

    /* Update g_vtable_index ke hasil probe */
    if (best_cluster_start >= 5 && best_cluster_start <= 20) {
        g_vtable_index = best_cluster_start;
    }

    return g_vtable_index;
}

/* --------------------------------------------------------
 * iokit_user_client_trap_dynamic()
 * Eksekutor VTable yang menggunakan g_vtable_index global
 * (diupdate oleh probe runtime atau oleh set_dynamic_vtable_index).
 * -------------------------------------------------------- */
uint64_t iokit_user_client_trap_dynamic(void* client_obj,
                                         uint64_t vtable_index_override,
                                         void* args) {
    /* Pilih: override per-call, atau pakai nilai global dari Rust */
    uint64_t index = (vtable_index_override != 0)
                     ? vtable_index_override
                     : g_vtable_index;

    if (!client_obj) return 0;

    /* 1. Ambil VTable pointer dari 8 byte pertama objek (Standard ABI C++) */
    uint64_t* vtable_ptr = *(uint64_t**)client_obj;
    if (!vtable_ptr) return 0;

    /* 2. Akses index secara dinamis */
    uint64_t target_func_ptr = vtable_ptr[index];
    if (!target_func_ptr) return 0;

    /* 3. Eksekusi fungsi */
    ExternalMethodFunc func = (ExternalMethodFunc)target_func_ptr;
    return func(client_obj, args, 0, 0, 0);
}