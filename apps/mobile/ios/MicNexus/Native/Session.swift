import Foundation

protocol SessionEvents: AnyObject {
  /// The PC introduced itself (INFO): nil accepts, otherwise the BYE reason.
  func decide(_ s: Session, transport: String) -> String?
  func sessionPending(_ s: Session)
  func sessionWelcome(_ s: Session, codec: String, rate: Int, frameMs: Int)
  func sessionMuteRequest(_ s: Session, muted: Bool)
  func sessionStats(_ s: Session, bufferMs: Int, rttMs: Int)
  /// `reason`: the PC's BYE reason, or "network" / "timeout" / "local" / "declined".
  func sessionEnded(_ s: Session, reason: String)
}

/// One connection to a PC: INFO → HELLO → (PENDING) → WELCOME, then audio
/// out, pings, mute both ways, BYE. Everything runs on the engine queue.
final class Session {
  let link: Link
  let transport: String
  let address: String
  private(set) var pcId = ""
  private(set) var pcName = "PC"
  private(set) var pcBusy = false
  private(set) var welcomed = false
  private(set) var rttMs = -1
  /// Set when another link to the same PC takes over (Wi‑Fi → cable).
  var replaced = false

  private weak var events: SessionEvents?
  private let queue: DispatchQueue
  private let hello: () -> [String: Any]
  private var parser = FrameParser()
  private var gotInfo = false
  private var lastRx = Date()
  private var started = Date()
  private var timer: DispatchSourceTimer?
  private var ended = false
  private var closeReason: String?

  static let timeout: TimeInterval = 6

  init(link: Link, transport: String, address: String, queue: DispatchQueue, events: SessionEvents, hello: @escaping () -> [String: Any]) {
    self.link = link
    self.transport = transport
    self.address = address
    self.queue = queue
    self.events = events
    self.hello = hello
  }

  func start() {
    link.onData = { [weak self] data in self?.receive(data) }
    link.onClose = { [weak self] in self?.end() }
    link.start()
    let t = DispatchSource.makeTimerSource(queue: queue)
    t.schedule(deadline: .now() + 1, repeating: 1)
    t.setEventHandler { [weak self] in self?.tick() }
    t.resume()
    timer = t
  }

  private func tick() {
    if !gotInfo, Date().timeIntervalSince(started) > 10 { return close("timeout") }
    if Date().timeIntervalSince(lastRx) > Session.timeout { return close("timeout") }
    guard gotInfo else { return }
    let now = UInt64(ProcessInfo.processInfo.systemUptime * 1000)
    send(Proto.ping, Proto.le64(now) + Proto.le32(UInt32(max(rttMs, 0))))
  }

  private func receive(_ data: Data) {
    lastRx = Date()
    for f in parser.push(data) {
      if ended { return }
      if !gotInfo {
        guard f.kind == Proto.info else { return close("local") }
        gotInfo = true
        let info = f.object
        pcId = info["id"] as? String ?? ""
        pcName = info["name"] as? String ?? "PC"
        pcBusy = info["busy"] as? Bool ?? false
        if let reason = events?.decide(self, transport: transport) {
          send(Proto.bye, (try? JSONSerialization.data(withJSONObject: ["reason": reason])) ?? Data())
          queue.asyncAfter(deadline: .now() + 0.15) { [weak self] in self?.close("declined") }
          return
        }
        send(Proto.hello, (try? JSONSerialization.data(withJSONObject: hello())) ?? Data())
        continue
      }
      switch f.kind {
      case Proto.pending:
        events?.sessionPending(self)
      case Proto.welcome:
        let w = f.object
        welcomed = true
        events?.sessionWelcome(
          self, codec: w["codec"] as? String ?? "pcm16", rate: w["rate"] as? Int ?? 48000, frameMs: w["frameMs"] as? Int ?? 10)
      case Proto.mute:
        if let b = f.payload.first { events?.sessionMuteRequest(self, muted: b == 1) }
      case Proto.pong:
        if f.payload.count >= 8 {
          let now = UInt64(ProcessInfo.processInfo.systemUptime * 1000)
          rttMs = Int(now &- Proto.readLe64(f.payload))
        }
      case Proto.stats:
        events?.sessionStats(self, bufferMs: f.object["bufferMs"] as? Int ?? 0, rttMs: rttMs)
      case Proto.bye:
        closeReason = f.object["reason"] as? String ?? "user"
        link.close()
        return
      default:
        break
      }
    }
  }

  func send(_ kind: UInt8, _ payload: Data) {
    guard !ended else { return }
    link.send(Proto.encode(kind, payload))
  }

  func sendMute(_ muted: Bool) {
    send(Proto.mute, Data([muted ? 1 : 0]))
  }

  /// Says goodbye, then closes once the frame had time to leave.
  func bye(_ reason: String) {
    send(Proto.bye, (try? JSONSerialization.data(withJSONObject: ["reason": reason])) ?? Data())
    closeReason = "local"
    queue.asyncAfter(deadline: .now() + 0.15) { [weak self] in self?.link.close() }
  }

  func close(_ reason: String) {
    if closeReason == nil { closeReason = reason }
    link.close()
  }

  private func end() {
    guard !ended else { return }
    ended = true
    timer?.cancel()
    timer = nil
    events?.sessionEnded(self, reason: closeReason ?? "network")
  }
}
