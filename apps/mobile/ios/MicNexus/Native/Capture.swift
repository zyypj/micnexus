import AVFoundation
import os

/// Microphone → (gain) → PCM 16-bit or Opus → AUDIO payloads (`u32 seq` + data).
///
/// An AVAudioSinkNode gets the input at the I/O buffer size (~5 ms) instead
/// of a tap's ~100 ms blocks. The realtime callback only copies samples; a
/// 5 ms timer on `work` resamples, frames and encodes.
///
/// While connected the engine keeps running even when muted: an active audio
/// session is what keeps the app alive in the background (muted frames are
/// simply not sent).
final class Capture {
  var muted = false

  private let codec: String
  private let rate: Int
  private let frameSamples: Int
  private let noiseSuppression: Bool
  private let gain: Float
  private let send: (Data) -> Void
  private let onLevel: (Float) -> Void
  private let onError: (String) -> Void

  private let engine = AVAudioEngine()
  private var sink: AVAudioSinkNode?
  private let work = DispatchQueue(label: "micnexus.capture", qos: .userInteractive)
  private var timer: DispatchSourceTimer?
  private let ring = OSAllocatedUnfairLock(initialState: [Float]())
  private var inputRate: Double = 48000

  // work-queue state
  private var phase: Double = 0
  private var lastSample: Float = 0
  private var pending: [Int16] = []
  private var seq: UInt32 = 0
  private var peak: Float = 0
  private var levelCount = 0
  private var opus: OpusEncoder?

  init(
    codec: String, rate: Int, frameMs: Int, noiseSuppression: Bool, gain: Float,
    send: @escaping (Data) -> Void, onLevel: @escaping (Float) -> Void, onError: @escaping (String) -> Void
  ) {
    self.codec = codec
    self.rate = codec == "opus" ? 48000 : rate
    self.frameSamples = self.rate * frameMs / 1000
    self.noiseSuppression = noiseSuppression
    self.gain = gain
    self.send = send
    self.onLevel = onLevel
    self.onError = onError
  }

  func matches(codec: String, rate: Int, frameMs: Int) -> Bool {
    let r = codec == "opus" ? 48000 : rate
    return self.codec == codec && self.rate == r && frameSamples == r * frameMs / 1000
  }

  func start() throws {
    let session = AVAudioSession.sharedInstance()
    try session.setCategory(
      .playAndRecord, mode: noiseSuppression ? .voiceChat : .measurement,
      options: [.mixWithOthers, .defaultToSpeaker])
    try? session.setPreferredSampleRate(48000)
    try? session.setPreferredIOBufferDuration(0.005)
    try session.setActive(true)

    let input = engine.inputNode
    if noiseSuppression {
      try? input.setVoiceProcessingEnabled(true)
    }
    let format = input.outputFormat(forBus: 0)
    inputRate = format.sampleRate > 0 ? format.sampleRate : 48000
    ring.withLock { $0.reserveCapacity(Int(inputRate)) }
    let ring = self.ring
    let node = AVAudioSinkNode { _, frameCount, list -> OSStatus in
      let buffers = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: list))
      guard let first = buffers.first, let data = first.mData else { return noErr }
      let samples = data.assumingMemoryBound(to: Float.self)
      let n = Int(frameCount)
      ring.withLock { r in
        if r.count > 48000 { r.removeAll(keepingCapacity: true) }  // consumer stalled: drop, never lag
        r.append(contentsOf: UnsafeBufferPointer(start: samples, count: n))
      }
      return noErr
    }
    sink = node
    engine.attach(node)
    engine.connect(input, to: node, format: format)
    if codec == "opus" {
      opus = OpusEncoder(frameSamples: frameSamples)
      if opus == nil { onError("Este iPhone não conseguiu iniciar o codificador Opus.") }
    }
    engine.prepare()
    try engine.start()

    let t = DispatchSource.makeTimerSource(queue: work)
    t.schedule(deadline: .now() + 0.005, repeating: 0.005, leeway: .milliseconds(1))
    t.setEventHandler { [weak self] in self?.process() }
    t.resume()
    timer = t

    NotificationCenter.default.addObserver(
      self, selector: #selector(interrupted(_:)), name: AVAudioSession.interruptionNotification, object: nil)
    NotificationCenter.default.addObserver(
      self, selector: #selector(reset), name: AVAudioSession.mediaServicesWereResetNotification, object: nil)
  }

  func stop() {
    NotificationCenter.default.removeObserver(self)
    timer?.cancel()
    timer = nil
    engine.stop()
    if let sink { engine.detach(sink) }
    sink = nil
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    onLevel(0)
  }

  @objc private func interrupted(_ note: Notification) {
    guard let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
      AVAudioSession.InterruptionType(rawValue: raw) == .ended
    else { return }
    // A phone call took the microphone; take it back when it ends.
    try? AVAudioSession.sharedInstance().setActive(true)
    try? engine.start()
  }

  @objc private func reset() {
    try? AVAudioSession.sharedInstance().setActive(true)
    try? engine.start()
  }

  private func process() {
    let input = ring.withLock { r -> [Float] in
      let out = r
      r.removeAll(keepingCapacity: true)
      return out
    }
    if input.isEmpty { return }
    // Linear resampling input rate → output rate (usually 48k → 48k: a copy).
    let step = inputRate / Double(rate)
    var pos = phase
    while pos < Double(input.count) {
      let i = Int(pos)
      let frac = Float(pos - Double(i))
      let a = i == 0 ? lastSample : input[i - 1]
      let s = (a + (input[i] - a) * frac) * gain
      let clipped = max(-1, min(1, s))
      pending.append(Int16(clipped * 32767))
      peak = max(peak, abs(clipped))
      pos += step
    }
    phase = pos - Double(input.count)
    lastSample = input[input.count - 1]

    levelCount += input.count
    if levelCount >= Int(inputRate / 20) {
      onLevel(muted ? 0 : peak)
      peak = 0
      levelCount = 0
    }
    while pending.count >= frameSamples {
      let frame = Array(pending.prefix(frameSamples))
      pending.removeFirst(frameSamples)
      if muted { continue }
      if let opus {
        for packet in opus.encode(frame) {
          send(Proto.le32(seq) + packet)
          seq &+= 1
        }
      } else {
        var payload = Proto.le32(seq)
        frame.withUnsafeBufferPointer { payload.append(Data(buffer: $0)) }
        send(payload)
        seq &+= 1
      }
    }
  }
}

/// AVAudioConverter Opus encoder: 20 ms PCM in, one Opus packet out.
private final class OpusEncoder {
  private let pcmFormat = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 48000, channels: 1, interleaved: true)!
  private let opusFormat: AVAudioFormat
  private let converter: AVAudioConverter
  private let frameSamples: Int

  init?(frameSamples: Int) {
    var asbd = AudioStreamBasicDescription(
      mSampleRate: 48000, mFormatID: kAudioFormatOpus, mFormatFlags: 0, mBytesPerPacket: 0,
      mFramesPerPacket: UInt32(frameSamples), mBytesPerFrame: 0, mChannelsPerFrame: 1, mBitsPerChannel: 0, mReserved: 0)
    guard let f = AVAudioFormat(streamDescription: &asbd), let c = AVAudioConverter(from: pcmFormat, to: f) else {
      return nil
    }
    c.bitRate = 64000
    opusFormat = f
    converter = c
    self.frameSamples = frameSamples
  }

  func encode(_ samples: [Int16]) -> [Data] {
    guard let input = AVAudioPCMBuffer(pcmFormat: pcmFormat, frameCapacity: AVAudioFrameCount(samples.count)) else {
      return []
    }
    input.frameLength = AVAudioFrameCount(samples.count)
    samples.withUnsafeBufferPointer { src in
      input.int16ChannelData![0].update(from: src.baseAddress!, count: samples.count)
    }
    let out = AVAudioCompressedBuffer(
      format: opusFormat, packetCapacity: 1, maximumPacketSize: max(converter.maximumOutputPacketSize, 1500))
    var fed = false
    var error: NSError?
    let status = converter.convert(to: out, error: &error) { _, inputStatus in
      if fed {
        inputStatus.pointee = .noDataNow
        return nil
      }
      fed = true
      inputStatus.pointee = .haveData
      return input
    }
    guard status != .error, out.byteLength > 0 else { return [] }
    return [Data(bytes: out.data, count: Int(out.byteLength))]
  }
}
