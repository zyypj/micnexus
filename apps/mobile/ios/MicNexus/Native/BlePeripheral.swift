import CoreBluetooth
import Foundation

/// The iPhone as a BLE GATT peripheral (iOS has no classic Bluetooth for
/// apps). The PC is the central: it writes our `rx` characteristic and
/// subscribes to `tx`; both carry the TCP byte stream (docs/PROTOCOL.md).
final class BlePeripheral: NSObject, CBPeripheralManagerDelegate {
  /// A PC subscribed: a new link to serve.
  var onLink: ((BleLink) -> Void)?
  var onStateChange: (() -> Void)?

  private var manager: CBPeripheralManager?
  private let queue: DispatchQueue
  private var tx: CBMutableCharacteristic?
  private var serviceAdded = false
  private var wantedName: String?
  private var links: [UUID: BleLink] = [:]

  init(queue: DispatchQueue) {
    self.queue = queue
    super.init()
  }

  /// Creating the manager is what asks for the Bluetooth permission, so it
  /// happens only once Bluetooth is actually needed (or already allowed).
  func activate() {
    guard manager == nil else { return }
    manager = CBPeripheralManager(
      delegate: self, queue: queue, options: [CBPeripheralManagerOptionShowPowerAlertKey: false])
  }

  var poweredOn: Bool { manager?.state == .poweredOn }
  var authorized: Bool { CBManager.authorization == .allowedAlways }

  /// Advertise with this local name ("MNX*" / "MNX<pc>"), or stop (nil).
  func advertise(_ name: String?) {
    wantedName = name
    if name != nil { activate() }
    apply()
  }

  private func apply() {
    guard let manager, manager.state == .poweredOn, serviceAdded else { return }
    manager.stopAdvertising()
    if let name = wantedName {
      manager.startAdvertising([
        CBAdvertisementDataServiceUUIDsKey: [Proto.bleService],
        CBAdvertisementDataLocalNameKey: name,
      ])
    }
  }

  func peripheralManagerDidUpdateState(_ peripheral: CBPeripheralManager) {
    if peripheral.state == .poweredOn && !serviceAdded {
      let rx = CBMutableCharacteristic(
        type: Proto.bleRx, properties: [.writeWithoutResponse, .write], value: nil, permissions: [.writeable])
      let tx = CBMutableCharacteristic(type: Proto.bleTx, properties: [.notify], value: nil, permissions: [.readable])
      let service = CBMutableService(type: Proto.bleService, primary: true)
      service.characteristics = [rx, tx]
      self.tx = tx
      peripheral.add(service)
    }
    if peripheral.state != .poweredOn {
      serviceAdded = false
      links.values.forEach { $0.remoteClosed() }
      links.removeAll()
    }
    onStateChange?()
  }

  func peripheralManager(_ peripheral: CBPeripheralManager, didAdd service: CBService, error: Error?) {
    serviceAdded = error == nil
    apply()
  }

  func peripheralManager(_ peripheral: CBPeripheralManager, central: CBCentral, didSubscribeTo characteristic: CBCharacteristic) {
    guard characteristic.uuid == Proto.bleTx, let tx else { return }
    peripheral.setDesiredConnectionLatency(.low, for: central)
    links[central.identifier]?.remoteClosed()
    let link = BleLink(central: central, manager: peripheral, tx: tx)
    link.onClosed = { [weak self] in self?.links[central.identifier] = nil }
    links[central.identifier] = link
    onLink?(link)
  }

  func peripheralManager(_ peripheral: CBPeripheralManager, central: CBCentral, didUnsubscribeFrom characteristic: CBCharacteristic) {
    links[central.identifier]?.remoteClosed()
    links[central.identifier] = nil
  }

  func peripheralManager(_ peripheral: CBPeripheralManager, didReceiveWrite requests: [CBATTRequest]) {
    for r in requests where r.characteristic.uuid == Proto.bleRx {
      if let value = r.value { links[r.central.identifier]?.received(value) }
    }
    if let first = requests.first { peripheral.respond(to: first, withResult: .success) }
  }

  func peripheralManagerIsReady(toUpdateSubscribers peripheral: CBPeripheralManager) {
    links.values.forEach { $0.flush() }
  }
}

final class BleLink: Link {
  var onData: ((Data) -> Void)?
  var onClose: (() -> Void)?
  var onClosed: (() -> Void)?
  private let central: CBCentral
  private weak var manager: CBPeripheralManager?
  private let tx: CBMutableCharacteristic
  /// Whole frames waiting, each split in MTU-sized chunks; the first one may
  /// be partly sent (`sent` chunks).
  private var frames: [(audio: Bool, chunks: [Data])] = []
  private var sent = 0
  private var closed = false

  init(central: CBCentral, manager: CBPeripheralManager, tx: CBMutableCharacteristic) {
    self.central = central
    self.manager = manager
    self.tx = tx
  }

  func start() {}

  func received(_ data: Data) {
    if !closed { onData?(data) }
  }

  /// `data` is always one whole frame (Session sends frame by frame).
  func send(_ data: Data) {
    guard !closed else { return }
    let size = max(20, central.maximumUpdateValueLength)
    var chunks: [Data] = []
    var at = data.startIndex
    while at < data.endIndex {
      let end = min(at + size, data.endIndex)
      chunks.append(Data(data[at..<end]))
      at = end
    }
    frames.append((audio: data.first == Proto.audio, chunks: chunks))
    // Real time: when the radio cannot keep up, drop whole audio frames
    // (never one being sent, never a control frame) instead of lagging.
    if frames.count > 60 {
      var i = 1
      while frames.count > 30 && i < frames.count {
        if frames[i].audio { frames.remove(at: i) } else { i += 1 }
      }
    }
    flush()
  }

  func flush() {
    guard let manager else { return }
    while let frame = frames.first {
      while sent < frame.chunks.count {
        if !manager.updateValue(frame.chunks[sent], for: tx, onSubscribedCentrals: [central]) { return }
        sent += 1
      }
      frames.removeFirst()
      sent = 0
    }
  }

  /// iOS cannot drop a central from the peripheral side; the PC closes after BYE.
  func close() {
    remoteClosed()
  }

  func remoteClosed() {
    guard !closed else { return }
    closed = true
    frames.removeAll()
    onClose?()
    onClosed?()
  }
}
