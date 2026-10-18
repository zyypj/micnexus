import AVFoundation
import Foundation
import Network
import UIKit

/// Everything the iPhone does, independent of the UI (keeps working in the
/// background through the audio session and the Live Activity buttons):
///
/// - listens on TCP 47820 and advertises `_micnexus._tcp` over Bonjour, with
///   `want` (the PC it wants now) in the TXT record;
/// - learns the PCs from their INFO (they introduce themselves every few s);
/// - is a BLE peripheral for the PC to connect to;
/// - runs the Session, the Capture and the Live Activity;
/// - waits up to 60 s for the PC to come back when the link drops.
///
/// All state lives on `queue`.
final class Engine: NSObject, SessionEvents {
  static let shared = Engine()

  enum Status: String { case idle, connecting, pending, connected, reconnecting }

  struct Target {
    var pcId: String?
    var name: String
    var transport: String
    var address: String
  }

  struct Pc {
    var id: String
    var name: String
    var wifi: String?
    var busy: Bool
    var seen: Date
  }

  let queue = DispatchQueue(label: "micnexus.engine", qos: .userInitiated)
  private let defaults = UserDefaults.standard

  // State (queue-confined).
  private(set) var status = Status.idle
  private(set) var target: Target?
  private(set) var muted = false
  private(set) var codec: String?
  private(set) var rttMs = -1
  private(set) var bufferMs = -1
  private(set) var error: String?
  private(set) var connectedAt: Date?
  private(set) var foreground = false

  private var pcs: [String: Pc] = [:]
  private var session: Session?
  /// Connections that have not been decided yet (retained until they end).
  private var incoming: [ObjectIdentifier: Session] = [:]
  private var capture: Capture?
  private var userDisconnected = false
  private var reconnectUntil: Date?
  private var listener: NWListener?
  private var advertisedTxt: [String: String] = [:]
  private lazy var ble = BlePeripheral(queue: queue)
  private var rate = 48000
  private var frameMs = 10
  private var started = false

  /// UI bridge (MicNexusModule).
  var onState: (() -> Void)?
  var onPcs: (() -> Void)?
  var onLevel: ((Float) -> Void)?

  // ------------------------------------------------------------- settings --

  var deviceId: String {
    if let id = defaults.string(forKey: "deviceId") { return id }
    let id = UUID().uuidString.lowercased()
    defaults.set(id, forKey: "deviceId")
    return id
  }

  var deviceName: String {
    let custom = defaults.string(forKey: "deviceName") ?? ""
    return custom.isEmpty ? systemName : custom
  }

  var autoConnect: Bool {
    get { defaults.object(forKey: "autoConnect") as? Bool ?? true }
    set { defaults.set(newValue, forKey: "autoConnect") }
  }
  var noiseSuppression: Bool {
    get { defaults.object(forKey: "noiseSuppression") as? Bool ?? true }
    set { defaults.set(newValue, forKey: "noiseSuppression") }
  }
  var gain: Float {
    get { defaults.object(forKey: "gain") as? Float ?? 1 }
    set { defaults.set(min(4, max(0.5, newValue)), forKey: "gain") }
  }
  var lastPcId: String? { defaults.string(forKey: "lastPcId") }
  private var lastTransport: String? { defaults.string(forKey: "lastTransport") }

  var micPermission: Bool { AVAudioApplication.shared.recordPermission == .granted }

  // --------------------------------------------------------------- start --

  /// UIDevice is main-thread only: read once in start().
  private var systemName = "iPhone"

  func start() {
    systemName = UIDevice.current.name
    queue.async {
      guard !self.started else { return }
      self.started = true
      self.ble.onLink = { [weak self] link in self?.accept(link: link, transport: "bluetooth", address: "Bluetooth") }
      self.ble.onStateChange = { [weak self] in self?.notifyState() }
      if self.ble.authorized { self.ble.activate() }
      self.startListener()
      let prune = DispatchSource.makeTimerSource(queue: self.queue)
      prune.schedule(deadline: .now() + 2, repeating: 2)
      prune.setEventHandler { [weak self] in self?.prunePcs() }
      prune.resume()
      self.pruneTimer = prune
    }
    Task { @MainActor in LiveActivityController.shared.endAll() }
  }

  private var pruneTimer: DispatchSourceTimer?

  func setForeground(_ value: Bool) {
    queue.async {
      self.foreground = value
      if value, self.listener == nil || self.listenerFailed { self.startListener() }
      self.updateAdvertising()
      self.notifyState()
    }
  }

  private var listenerFailed = false

  private func startListener() {
    listener?.cancel()
    listenerFailed = false
    do {
      let params = NWParameters.tcp
      params.allowLocalEndpointReuse = true
      let l = try NWListener(using: params, on: NWEndpoint.Port(rawValue: Proto.phonePort)!)
      l.service = NWListener.Service(name: deviceName, type: Proto.bonjourType, domain: nil, txtRecord: txtRecord())
      l.newConnectionHandler = { [weak self] connection in
        guard let self else { return connection.cancel() }
        let link = TcpLink(connection, queue: self.queue)
        self.accept(link: link, transport: link.isLoopback ? "usb" : "wifi", address: link.isLoopback ? "USB" : link.remoteAddress)
      }
      l.stateUpdateHandler = { [weak self] state in
        guard let self else { return }
        if case .failed = state {
          self.listenerFailed = true
          self.queue.asyncAfter(deadline: .now() + 2) { if self.foreground { self.startListener() } }
        }
      }
      l.start(queue: queue)
      listener = l
    } catch {
      listenerFailed = true
    }
  }

  // ------------------------------------------------------------ discovery --

  /// The PC this phone wants right now (TXT `want`, BLE advertised name).
  private func currentWant() -> String? {
    switch status {
    case .connecting, .reconnecting: return target?.pcId
    case .idle:
      return foreground && autoConnect && !userDisconnected && micPermission ? lastPcId : nil
    default: return nil
    }
  }

  private func txtRecord() -> NWTXTRecord {
    var txt = NWTXTRecord()
    txt["v"] = "1"
    txt["id"] = deviceId
    txt["name"] = deviceName
    txt["want"] = currentWant() ?? ""
    return txt
  }

  /// Republishes the Bonjour TXT and the BLE advertisement after any change.
  private func updateAdvertising() {
    let txt: [String: String] = ["v": "1", "id": deviceId, "name": deviceName, "want": currentWant() ?? ""]
    if txt != advertisedTxt, let listener {
      advertisedTxt = txt
      listener.service = NWListener.Service(name: deviceName, type: Proto.bonjourType, domain: nil, txtRecord: txtRecord())
    }
    // BLE: only while asking for a PC over Bluetooth.
    var bleName: String?
    if let t = target, t.transport == "bluetooth", status == .connecting || status == .reconnecting {
      bleName = "MNX" + (t.pcId.map { String($0.prefix(8)) } ?? "*")
    } else if status == .idle, lastTransport == "bluetooth", let want = currentWant() {
      bleName = "MNX" + String(want.prefix(8))
    }
    ble.advertise(bleName)
  }

  private func prunePcs() {
    let before = pcs.count
    pcs = pcs.filter { Date().timeIntervalSince($0.value.seen) < 9 }
    if pcs.count != before { notifyPcs() }
  }

  /// PCs to show (introduced themselves recently); any of them may do BLE.
  func pcList() -> [[String: Any]] {
    let last = lastPcId
    return pcs.values
      .sorted { a, b in (a.id == last ? 0 : 1, a.name) < (b.id == last ? 0 : 1, b.name) }
      .map { pc in
        [
          "key": pc.id, "pcId": pc.id, "name": pc.name,
          "wifi": pc.wifi as Any, "bluetooth": "ble", "busy": pc.busy, "last": pc.id == last,
        ]
      }
  }

  // ------------------------------------------------------------- connect --

  /// The user tapped a PC (`key` = its id) or "Bluetooth" (`ble:any`).
  func connect(key: String, transport: String) {
    queue.async {
      guard self.micPermission else {
        self.error = "Permita o uso do microfone para conectar."
        return self.notifyState()
      }
      self.userDisconnected = false
      self.error = nil
      self.muted = false
      if self.status == .connected || self.status == .pending { self.session?.bye("user") }
      let pc = self.pcs[key]
      let pcId = key == "ble:any" ? nil : key
      if transport == "bluetooth" { self.ble.activate() }
      self.setStatus(.connecting, Target(
        pcId: pcId, name: pc?.name ?? "PC", transport: transport == "bluetooth" ? "bluetooth" : "wifi",
        address: transport == "bluetooth" ? "Bluetooth" : pc?.wifi ?? ""))
      let expected = self.target?.pcId
      let waitFor: Double = transport == "bluetooth" ? 20 : 10
      self.queue.asyncAfter(deadline: .now() + waitFor) {
        guard self.status == .connecting, self.target?.pcId == expected else { return }
        self.error = transport == "bluetooth"
          ? "Nenhum PC respondeu pelo Bluetooth. O MicNexus está aberto no PC, com o Bluetooth ligado?"
          : "O PC não respondeu. Confira se o MicNexus está aberto nele."
        self.setStatus(.idle, nil)
      }
    }
  }

  // ------------------------------------------------------------ incoming --

  private func accept(link: Link, transport: String, address: String) {
    let s = Session(link: link, transport: transport, address: address, queue: queue, events: self) { [weak self] in
      self?.helloPayload() ?? [:]
    }
    incoming[ObjectIdentifier(s)] = s
    s.start()
  }

  private func helloPayload() -> [String: Any] {
    [
      "v": Proto.version, "id": deviceId, "name": deviceName, "model": "iPhone",
      "version": Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0",
      "codecs": ["opus", "pcm16"], "muted": muted,
    ]
  }

  func decide(_ s: Session, transport: String) -> String? {
    // Every PC that knocks goes into the list (that is how iOS finds PCs).
    let known = pcs[s.pcId]
    pcs[s.pcId] = Pc(
      id: s.pcId, name: s.pcName, wifi: transport == "wifi" ? s.address : known?.wifi, busy: s.pcBusy, seen: Date())
    if known == nil || known?.name != s.pcName || known?.busy != s.pcBusy { notifyPcs() }

    guard micPermission else { return "idle" }
    let refuse: String?
    switch status {
    case .connected, .pending:
      if let cur = session, cur.pcId == s.pcId, transport == "usb", cur.transport != "usb" {
        refuse = nil  // same PC over the cable: move to it
      } else {
        refuse = "busy"
      }
    case .reconnecting:
      refuse = target?.pcId == s.pcId ? nil : "busy"
    case .connecting:
      // The PC we asked for (any transport), or any PC when "Bluetooth" was tapped.
      if let t = target, t.pcId == s.pcId || (t.pcId == nil && transport == "bluetooth" && t.transport == "bluetooth") {
        refuse = nil
      } else {
        refuse = "busy"
      }
    case .idle:
      switch transport {
      case "usb": refuse = !userDisconnected && foreground ? nil : "idle"
      case "bluetooth": refuse = currentWant() == s.pcId ? nil : "idle"
      default: refuse = currentWant() == s.pcId ? nil : "idle"
      }
    }
    if refuse != nil { return refuse }

    if let old = session, old !== s {
      old.replaced = true
      old.bye("replaced")
    }
    session = s
    incoming[ObjectIdentifier(s)] = nil
    let t = Target(pcId: s.pcId, name: s.pcName, transport: transport, address: s.address)
    if status == .reconnecting || status == .connected {
      target = t
    } else {
      setStatus(.connecting, t)
    }
    return nil
  }

  func sessionPending(_ s: Session) {
    guard s === session else { return }
    setStatus(.pending, target)
  }

  func sessionWelcome(_ s: Session, codec: String, rate: Int, frameMs: Int) {
    guard s === session else { return }
    self.codec = codec
    self.rate = rate
    self.frameMs = frameMs
    error = nil
    reconnectUntil = nil
    if connectedAt == nil { connectedAt = Date() }
    defaults.set(s.pcId, forKey: "lastPcId")
    defaults.set(s.pcName, forKey: "lastPcName")
    defaults.set(s.transport, forKey: "lastTransport")
    pcs[s.pcId]?.name = s.pcName
    setStatus(.connected, Target(pcId: s.pcId, name: s.pcName, transport: s.transport, address: s.address))
    startCapture()
  }

  private func startCapture() {
    if let capture {
      // Already running (reconnect or transport change): same format? keep it.
      if capture.matches(codec: codec ?? "pcm16", rate: rate, frameMs: frameMs) {
        capture.muted = muted
        return
      }
      capture.stop()
      self.capture = nil
    }
    let c = Capture(
      codec: codec ?? "pcm16", rate: rate, frameMs: frameMs, noiseSuppression: noiseSuppression, gain: gain,
      send: { [weak self] payload in
        self?.queue.async {
          guard let s = self?.session, s.welcomed else { return }
          s.send(Proto.audio, payload)
        }
      },
      onLevel: { [weak self] level in self?.onLevel?(level) },
      onError: { [weak self] msg in
        self?.queue.async {
          self?.error = msg
          self?.notifyState()
        }
      })
    c.muted = muted
    do {
      try c.start()
      capture = c
    } catch {
      self.error = "Não foi possível abrir o microfone: \(error.localizedDescription)"
      session?.bye("idle")
    }
  }

  private func stopCapture() {
    capture?.stop()
    capture = nil
    onLevel?(0)
  }

  func sessionMuteRequest(_ s: Session, muted: Bool) {
    guard s === session else { return }
    applyMute(muted)
  }

  func sessionStats(_ s: Session, bufferMs: Int, rttMs: Int) {
    guard s === session else { return }
    self.bufferMs = bufferMs
    self.rttMs = rttMs
    onState?()
  }

  func sessionEnded(_ s: Session, reason: String) {
    incoming[ObjectIdentifier(s)] = nil
    guard s === session, !s.replaced else { return }
    session = nil
    rttMs = -1
    bufferMs = -1
    let wasConnected = status == .connected || status == .reconnecting
    let dropped = reason == "network" || reason == "timeout"
    if dropped && wasConnected && !userDisconnected {
      // Keep the audio session running: it is what keeps us alive in the background.
      if reconnectUntil == nil { reconnectUntil = Date().addingTimeInterval(60) }
      setStatus(.reconnecting, target)
      scheduleReconnectCheck()
      return
    }
    switch reason {
    case "rejected": error = "O PC recusou a conexão."
    case "busy": error = "O PC já está usando outro celular."
    case "version": error = "Atualize o MicNexus no PC e no celular para a mesma versão."
    case "user":
      if !userDisconnected {
        error = "O PC desconectou."
        userDisconnected = true  // the PC chose to stop: no auto-reconnect
      }
    case "local", "replaced", "idle", "declined": break
    default: error = wasConnected ? "A conexão caiu." : "Não foi possível conectar."
    }
    reconnectUntil = nil
    setStatus(.idle, nil)
  }

  private func scheduleReconnectCheck() {
    queue.asyncAfter(deadline: .now() + 3) {
      guard self.status == .reconnecting else { return }
      if let until = self.reconnectUntil, Date() > until {
        self.error = "Conexão perdida com \(self.target?.name ?? "o PC")."
        self.reconnectUntil = nil
        self.setStatus(.idle, nil)
        return
      }
      self.scheduleReconnectCheck()
    }
  }

  // --------------------------------------------------------- user actions --

  func disconnect() {
    queue.async {
      self.userDisconnected = true
      self.reconnectUntil = nil
      let s = self.session
      self.session = nil
      s?.bye("user")
      self.error = nil
      self.setStatus(.idle, nil)
    }
  }

  func setMuted(_ value: Bool) {
    queue.async { self.applyMute(value) }
  }

  func toggleMute() {
    queue.async { self.applyMute(!self.muted) }
  }

  private func applyMute(_ value: Bool) {
    if muted == value {
      session?.sendMute(value)  // confirm the PC's request
      return
    }
    muted = value
    capture?.muted = value
    if value { onLevel?(0) }
    session?.sendMute(value)
    notifyState()
  }

  func clearError() {
    queue.async {
      self.error = nil
      self.notifyState()
    }
  }

  func requestBluetooth() {
    queue.async { self.ble.activate() }
  }

  // --------------------------------------------------------------- state --

  private func setStatus(_ next: Status, _ t: Target?) {
    status = next
    target = t
    if next == .idle {
      codec = nil
      connectedAt = nil
      stopCapture()
    }
    updateAdvertising()
    notifyState()
  }

  private func notifyState() {
    onState?()
    let live = status == .connected || status == .reconnecting
    let name = target?.name ?? "PC"
    let content = MicActivityAttributes.ContentState(
      muted: muted, status: status == .reconnecting ? "reconnecting" : "connected",
      transport: target?.transport ?? "wifi", connectedAt: connectedAt ?? Date())
    Task { @MainActor in
      if live {
        LiveActivityController.shared.show(pcName: name, state: content)
      } else {
        LiveActivityController.shared.end()
      }
    }
  }

  private func notifyPcs() {
    onPcs?()
  }

  /// Snapshot for the UI (call on `queue`).
  func stateDictionary() -> [String: Any] {
    var d: [String: Any] = [
      "status": status.rawValue,
      "muted": muted,
      "codec": codec as Any,
      "rttMs": rttMs >= 0 ? rttMs : NSNull(),
      "bufferMs": bufferMs >= 0 ? bufferMs : NSNull(),
      "error": error as Any,
      "connectedAt": (connectedAt?.timeIntervalSince1970 ?? 0) * 1000,
      "bluetoothOn": ble.poweredOn,
      "bluetoothPermission": ble.authorized,
      "micPermission": micPermission,
      "deviceName": deviceName,
    ]
    if let t = target {
      d["target"] = ["pcId": t.pcId as Any, "name": t.name, "transport": t.transport, "address": t.address]
    } else {
      d["target"] = NSNull()
    }
    return d
  }
}
