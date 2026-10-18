import CoreBluetooth
import Foundation
import Network

/// Wire format shared with Windows and Android (docs/PROTOCOL.md).
enum Proto {
  static let version = 1
  /// TCP port this phone listens on (Wi‑Fi, and USB through usbmux).
  static let phonePort: UInt16 = 47820
  static let bonjourType = "_micnexus._tcp"
  static let bleService = CBUUID(string: "A3C8E0F2-5B17-4D9A-8E61-0C4F2B9D7A16")
  /// PC → phone (write without response).
  static let bleRx = CBUUID(string: "A3C8E0F2-5B17-4D9A-8E61-0C4F2B9D7A17")
  /// Phone → PC (notify).
  static let bleTx = CBUUID(string: "A3C8E0F2-5B17-4D9A-8E61-0C4F2B9D7A18")

  static let hello: UInt8 = 1
  static let welcome: UInt8 = 2
  static let pending: UInt8 = 3
  static let audio: UInt8 = 4
  static let mute: UInt8 = 5
  static let ping: UInt8 = 6
  static let pong: UInt8 = 7
  static let bye: UInt8 = 8
  static let info: UInt8 = 10
  static let stats: UInt8 = 11

  static func encode(_ kind: UInt8, _ payload: Data) -> Data {
    let len = min(payload.count, 0xFFFF)
    var out = Data(capacity: 3 + len)
    out.append(kind)
    out.append(UInt8(len & 0xFF))
    out.append(UInt8(len >> 8))
    out.append(payload.prefix(len))
    return out
  }

  static func json(_ kind: UInt8, _ object: [String: Any]) -> Data {
    encode(kind, (try? JSONSerialization.data(withJSONObject: object)) ?? Data())
  }

  static func le32(_ v: UInt32) -> Data { withUnsafeBytes(of: v.littleEndian) { Data($0) } }
  static func le64(_ v: UInt64) -> Data { withUnsafeBytes(of: v.littleEndian) { Data($0) } }

  static func readLe64(_ d: Data) -> UInt64 {
    var v: UInt64 = 0
    for (i, b) in d.prefix(8).enumerated() { v |= UInt64(b) << (8 * UInt64(i)) }
    return v
  }
}

struct Frame {
  let kind: UInt8
  let payload: Data

  var object: [String: Any] {
    (try? JSONSerialization.jsonObject(with: payload)) as? [String: Any] ?? [:]
  }
}

/// Turns a byte stream (TCP chunks, BLE notifications) back into frames.
struct FrameParser {
  private var buffer = Data()

  mutating func push(_ bytes: Data) -> [Frame] {
    buffer.append(bytes)
    var frames: [Frame] = []
    while buffer.count >= 3 {
      let start = buffer.startIndex
      let len = Int(buffer[start + 1]) | Int(buffer[start + 2]) << 8
      guard buffer.count >= 3 + len else { break }
      frames.append(Frame(kind: buffer[start], payload: Data(buffer[(start + 3)..<(start + 3 + len)])))
      buffer = Data(buffer[(start + 3 + len)...])
    }
    return frames
  }
}

/// A byte pipe to a PC: TCP (Wi‑Fi, USB) or the two BLE characteristics.
protocol Link: AnyObject {
  var onData: ((Data) -> Void)? { get set }
  var onClose: (() -> Void)? { get set }
  func start()
  func send(_ data: Data)
  func close()
}

final class TcpLink: Link {
  var onData: ((Data) -> Void)?
  var onClose: (() -> Void)?
  private let connection: NWConnection
  private let queue: DispatchQueue
  private var closed = false

  init(_ connection: NWConnection, queue: DispatchQueue) {
    self.connection = connection
    self.queue = queue
  }

  /// usbmux tunnels arrive from the loopback interface.
  var isLoopback: Bool {
    if case let .hostPort(host, _) = connection.endpoint {
      switch host {
      case .ipv4(let a): return a.isLoopback
      case .ipv6(let a): return a.isLoopback || (a.asIPv4?.isLoopback ?? false)
      default: return false
      }
    }
    return false
  }

  var remoteAddress: String {
    if case let .hostPort(host, _) = connection.endpoint {
      switch host {
      case .ipv4(let a): return "\(a)"
      case .ipv6(let a): return "\(a)"
      case .name(let n, _): return n
      @unknown default: return "?"
      }
    }
    return "?"
  }

  func start() {
    connection.stateUpdateHandler = { [weak self] state in
      switch state {
      case .failed, .cancelled: self?.finish()
      default: break
      }
    }
    connection.start(queue: queue)
    receive()
  }

  private func receive() {
    connection.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { [weak self] data, _, done, error in
      guard let self else { return }
      if let data, !data.isEmpty { self.onData?(data) }
      if done || error != nil { self.finish() } else { self.receive() }
    }
  }

  func send(_ data: Data) {
    guard !closed else { return }
    connection.send(content: data, completion: .contentProcessed { [weak self] error in
      if error != nil { self?.finish() }
    })
  }

  func close() {
    connection.cancel()
    finish()
  }

  private func finish() {
    guard !closed else { return }
    closed = true
    onClose?()
  }
}
