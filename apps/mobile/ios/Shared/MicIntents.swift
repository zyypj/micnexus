import AppIntents
import Foundation

// Buttons of the Live Activity. LiveActivityIntent runs `perform()` in the
// app's process (alive in the background thanks to the audio session), so it
// can drive the Engine; the widget extension only references the types.

struct ToggleMuteIntent: LiveActivityIntent {
  static var title: LocalizedStringResource = "Mutar ou desmutar o microfone"
  static var openAppWhenRun: Bool = false

  func perform() async throws -> some IntentResult {
    #if MICNEXUS_APP
    await MainActor.run { Engine.shared.toggleMute() }
    #endif
    return .result()
  }
}

struct DisconnectIntent: LiveActivityIntent {
  static var title: LocalizedStringResource = "Desconectar do PC"
  static var openAppWhenRun: Bool = false

  func perform() async throws -> some IntentResult {
    #if MICNEXUS_APP
    await MainActor.run { Engine.shared.disconnect() }
    #endif
    return .result()
  }
}
