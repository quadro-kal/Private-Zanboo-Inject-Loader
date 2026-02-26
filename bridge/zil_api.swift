import Foundation

/// ZilAPI — Antarmuka publik ZIL Framework untuk lapisan UI.
/// Ini adalah titik masuk SATU-SATUNYA bagi kode di atas layer ini.
/// Semua panggilan harus melewati Validator sebelum diteruskan ke Rust.

// --- BINDING KE RUST (FFI DECLARATIONS) ---
// Fungsi-fungsi ini diimplementasikan di executor/main.rs dan dikompilasi
// sebagai staticlib. Swift akan memanggilnya via C bridging header.

@_silgen_name("zil_rust_status")
func zilRustStatus() -> UInt32

@_silgen_name("zil_rust_inject_cdhash")
func zilRustInjectCDHash(_ hashPtr: UnsafePointer<UInt8>, _ count: UInt64) -> Int32

@_silgen_name("zil_rust_dispatch_payload")
func zilRustDispatchPayload(_ dataPtr: UnsafePointer<UInt8>, _ size: UInt64) -> Int32

// RESIDUAL 3 FIX: Tambah binding ke fungsi telemetri Rust
@_silgen_name("zil_rust_get_telemetry")
func zilRustGetTelemetry(
    _ outNearMisses: UnsafeMutablePointer<UInt32>,
    _ outSuccesses:  UnsafeMutablePointer<UInt32>,
    _ outFailures:   UnsafeMutablePointer<UInt32>
)

// --- SNAPSHOT TELEMETRI (Value Type) ---
public struct ZilTelemetrySnapshot {
    public let nearMisses:   UInt32
    public let successes:    UInt32
    public let failures:     UInt32
    public var successRate:  Int {
        let total = Int(successes) + Int(failures)
        guard total > 0 else { return 100 }
        return (Int(successes) * 100) / total
    }
    public var isDegraded: Bool { successRate < 50 }
}

// --- KODE STATUS OPERASI ---

public enum ZilStatus: UInt32 {
    case idle       = 0
    case scanning   = 1
    case escalating = 2
    case ready      = 3
    case error      = 0xFF
}

// --- KELAS API UTAMA ---

public class ZilAPI {

    public static func getStatus() -> ZilStatus {
        let raw = zilRustStatus()
        return ZilStatus(rawValue: raw) ?? .error
    }

    // RESIDUAL 3 FIX: API untuk mendapatkan snapshot telemetri dari Rust
    public static func getSystemHealth() -> ZilTelemetrySnapshot {
        var nm: UInt32 = 0, sc: UInt32 = 0, fl: UInt32 = 0
        zilRustGetTelemetry(&nm, &sc, &fl)
        return ZilTelemetrySnapshot(nearMisses: nm, successes: sc, failures: fl)
    }

    /// Injeksikan CDHash ke kernel TrustCache.
    /// Input divalidasi terlebih dahulu oleh ZilValidator.
    public static func injectCDHash(_ hashString: String) -> Result<Void, String> {
        switch ZilValidator.validateCDHash(hashString) {
        case .invalid(let reason):
            return .failure("VALIDASI GAGAL: \(reason)")

        case .valid(let hashBytes):
            let result = hashBytes.withUnsafeBufferPointer { ptr in
                zilRustInjectCDHash(ptr.baseAddress!, UInt64(hashBytes.count))
            }
            return result == 0 ? .success(()) : .failure("RUST_ERR: code \(result)")
        }
    }

    /// Kirim payload ke NPU/GPU untuk eksekusi stealth.
    /// Payload divalidasi ukuran dan formatnya sebelum dikirim.
    public static func dispatchPayload(_ data: Data) -> Result<Void, String> {
        switch ZilValidator.validatePayload(data) {
        case .invalid(let reason):
            return .failure("VALIDASI GAGAL: \(reason)")

        case .valid(let cleanData):
            let result = cleanData.withUnsafeBytes { ptr in
                zilRustDispatchPayload(
                    ptr.baseAddress!.assumingMemoryBound(to: UInt8.self),
                    UInt64(cleanData.count)
                )
            }
            return result == 0 ? .success(()) : .failure("RUST_ERR: code \(result)")
        }
    }
}
