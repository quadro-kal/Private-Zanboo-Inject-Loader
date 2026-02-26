import Foundation

/// ValidationLayer — Gerbang keamanan antara UI dan Rust Core.
/// Setiap parameter yang dikirim user ke ZIL harus melewati file ini.
/// Mencegah crash di Rust akibat input yang cacat atau berbahaya.

// --- TIPE DATA HASIL VALIDASI ---

enum ValidationResult<T> {
    valid(value: T)
    invalid(reason: String)
}

// --- KONSTANTA BATAS AMAN ---

private let MIN_PAYLOAD_SIZE: Int = 4
private let MAX_PAYLOAD_SIZE: Int = 10 * 1024 * 1024  // 10MB (batas NPU arena)
private let FORBIDDEN_ADDR_LOW:  UInt64 = 0x200000000
private let FORBIDDEN_ADDR_HIGH: UInt64 = 0x300000000

// --- KELAS VALIDASI UTAMA ---

public class ZilValidator {

    /// Validasi array byte payload sebelum dikirim ke NPU/GPU.
    /// Return: data yang sudah dibersihkan, atau pesan error.
    public static func validatePayload(_ data: Data) -> ValidationResult<Data> {
        if data.count < MIN_PAYLOAD_SIZE {
            return .invalid(reason: "Payload terlalu kecil (min \(MIN_PAYLOAD_SIZE) byte)")
        }
        if data.count > MAX_PAYLOAD_SIZE {
            return .invalid(reason: "Payload terlalu besar (max \(MAX_PAYLOAD_SIZE / 1024 / 1024)MB)")
        }
        return .valid(value: data)
    }

    /// Validasi alamat memori sebelum dikirim sebagai target operasi.
    /// Menolak alamat yang jatuh di zona SPTM/EL2 yang dilindungi.
    public static func validateAddress(_ addr: UInt64) -> ValidationResult<UInt64> {
        if addr == 0 {
            return .invalid(reason: "Alamat null tidak diperbolehkan")
        }
        // Cek apakah masuk ke zona terlarang EL2 (akan memicu SPTM trap)
        if addr >= FORBIDDEN_ADDR_LOW && addr < FORBIDDEN_ADDR_HIGH {
            return .invalid(reason: "Alamat 0x\(String(addr, radix: 16)) berada di zona SPTM yang terlarang")
        }
        return .valid(value: addr)
    }

    /// Validasi CDHash string sebelum diserahkan ke CS Bypasser.
    /// CDHash harus berupa hex string 40 karakter (20 bytes SHA-256 truncated).
    public static func validateCDHash(_ hash: String) -> ValidationResult<[UInt8]> {
        let clean = hash.trimmingCharacters(in: .whitespaces).lowercased()
        guard clean.count == 40 else {
            return .invalid(reason: "CDHash harus 40 karakter hex (20 byte). Diterima: \(clean.count)")
        }
        guard clean.allSatisfy({ $0.isHexDigit }) else {
            return .invalid(reason: "CDHash mengandung karakter non-hex")
        }
        // Konversi ke byte array
        var bytes: [UInt8] = []
        var idx = clean.startIndex
        while idx < clean.endIndex {
            let next = clean.index(idx, offsetBy: 2)
            if let byte = UInt8(clean[idx..<next], radix: 16) {
                bytes.append(byte)
            }
            idx = next
        }
        return .valid(value: bytes)
    }
}
