import AVFoundation
import Foundation
import React
import UIKit

/// JS bridge to the Engine (src/native/MicNexus.ts) — same API and events as
/// the Android module.
@objc(MicNexus)
final class MicNexusModule: RCTEventEmitter {
  private var listening = false

  override init() {
    super.init()
    let engine = Engine.shared
    engine.onState = { [weak self] in self?.emitState() }
    engine.onPcs = { [weak self] in self?.emitPcs() }
    engine.onLevel = { [weak self] level in
      guard let self, self.listening, engine.foreground else { return }
      self.sendEvent(withName: "MicNexusLevel", body: Double(level))
    }
  }

  override func supportedEvents() -> [String]! {
    ["MicNexusState", "MicNexusPcs", "MicNexusLevel", "MicNexusUpdateError"]
  }

  override func startObserving() { listening = true }
  override func stopObserving() { listening = false }

  private func emitState() {
    guard listening else { return }
    sendEvent(withName: "MicNexusState", body: Engine.shared.stateDictionary())
  }

  private func emitPcs() {
    guard listening else { return }
    sendEvent(withName: "MicNexusPcs", body: Engine.shared.pcList())
  }

  @objc func getState(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
    Engine.shared.queue.async { resolve(Engine.shared.stateDictionary()) }
  }

  @objc func getPcs(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
    Engine.shared.queue.async { resolve(Engine.shared.pcList()) }
  }

  private func settings() -> [String: Any] {
    let e = Engine.shared
    return ["autoConnect": e.autoConnect, "noiseSuppression": e.noiseSuppression, "gain": Double(e.gain), "lastPcId": e.lastPcId as Any]
  }

  @objc func getSettings(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
    resolve(settings())
  }

  @objc func setSettings(_ patch: NSDictionary, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
    let e = Engine.shared
    if let v = patch["autoConnect"] as? Bool { e.autoConnect = v }
    if let v = patch["noiseSuppression"] as? Bool { e.noiseSuppression = v }
    if let v = patch["gain"] as? Double { e.gain = Float(v) }
    resolve(settings())
  }

  /// Microphone permission ("granted" | "denied" | "blocked"); Bluetooth is
  /// asked the first time Bluetooth is used, Local Network on first use.
  @objc func requestPermissions(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
    switch AVAudioApplication.shared.recordPermission {
    case .granted: resolve("granted")
    case .denied: resolve("blocked")
    default:
      AVAudioApplication.requestRecordPermission { granted in
        resolve(granted ? "granted" : "denied")
        Engine.shared.clearError()  // re-emits the state with the new permission
      }
    }
  }

  @objc func connect(_ key: String, transport: String) {
    Engine.shared.connect(key: key, transport: transport)
  }

  @objc func disconnect() { Engine.shared.disconnect() }
  @objc func setMuted(_ muted: Bool) { Engine.shared.setMuted(muted) }
  @objc func clearError() { Engine.shared.clearError() }

  @objc func refresh() {
    Engine.shared.queue.async { [weak self] in
      self?.emitState()
      self?.emitPcs()
    }
  }

  /// iOS cannot open the Bluetooth page; asking for it here prompts the
  /// permission (first time) — the system settings are the app's page.
  @objc func openBluetoothSettings() {
    Engine.shared.requestBluetooth()
  }

  @objc func openAppSettings() {
    DispatchQueue.main.async {
      if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
    }
  }

  @objc func getAppVersion(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
    resolve(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0")
  }

  /// No self-install on iOS: opens the release page (new .ipa for Sideloadly).
  @objc func installUpdate(_ url: String, version: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
    DispatchQueue.main.async {
      if let u = URL(string: url) { UIApplication.shared.open(u) }
    }
    resolve("opened")
  }
}
