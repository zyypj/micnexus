import ActivityKit
import Foundation

/// The Live Activity shown while a PC is connected (lock screen and Dynamic
/// Island) — the iOS counterpart of Android's ongoing notification.
struct MicActivityAttributes: ActivityAttributes {
  struct ContentState: Codable, Hashable {
    var muted: Bool
    /// "connected" | "reconnecting"
    var status: String
    /// "wifi" | "usb" | "bluetooth"
    var transport: String
    var connectedAt: Date
  }

  var pcName: String
}

extension MicActivityAttributes.ContentState {
  var transportLabel: String {
    switch transport {
    case "usb": return "Cabo USB"
    case "bluetooth": return "Bluetooth"
    default: return "Wi‑Fi"
    }
  }
}
