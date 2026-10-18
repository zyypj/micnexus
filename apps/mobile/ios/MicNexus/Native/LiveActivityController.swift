import ActivityKit
import Foundation

/// Lock screen / Dynamic Island while a PC is connected (iOS has no ongoing
/// notifications; this is the equivalent, with Mutar and Desconectar).
@MainActor
final class LiveActivityController {
  static let shared = LiveActivityController()
  private var activity: Activity<MicActivityAttributes>?

  func show(pcName: String, state: MicActivityAttributes.ContentState) {
    guard ActivityAuthorizationInfo().areActivitiesEnabled else { return }
    if let activity, activity.attributes.pcName == pcName {
      Task { await activity.update(ActivityContent(state: state, staleDate: nil)) }
      return
    }
    end()
    do {
      activity = try Activity.request(
        attributes: MicActivityAttributes(pcName: pcName),
        content: ActivityContent(state: state, staleDate: nil),
        pushType: nil)
    } catch {
      activity = nil
    }
  }

  func end() {
    guard let activity else { return }
    self.activity = nil
    Task { await activity.end(nil, dismissalPolicy: .immediate) }
  }

  /// Leftovers from a previous run (the app was killed while connected).
  func endAll() {
    for a in Activity<MicActivityAttributes>.activities {
      Task { await a.end(nil, dismissalPolicy: .immediate) }
    }
  }
}
