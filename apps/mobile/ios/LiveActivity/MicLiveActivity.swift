import ActivityKit
import AppIntents
import SwiftUI
import WidgetKit

@main
struct MicNexusWidgets: WidgetBundle {
  var body: some Widget {
    MicLiveActivity()
  }
}

private let accent = Color(red: 0x54 / 255, green: 0x68 / 255, blue: 0xF5 / 255)
private let live = Color(red: 0x2F / 255, green: 0xD2 / 255, blue: 0x7A / 255)
private let danger = Color(red: 0xF4 / 255, green: 0x50 / 255, blue: 0x6B / 255)

/// Lock screen banner + Dynamic Island while a PC uses the microphone:
/// who, how, for how long, and the Mutar / Desconectar buttons.
struct MicLiveActivity: Widget {
  var body: some WidgetConfiguration {
    ActivityConfiguration(for: MicActivityAttributes.self) { context in
      LockScreenView(pcName: context.attributes.pcName, state: context.state)
        .activityBackgroundTint(Color(red: 0x11 / 255, green: 0x13 / 255, blue: 0x1F / 255))
        .activitySystemActionForegroundColor(.white)
    } dynamicIsland: { context in
      DynamicIsland {
        DynamicIslandExpandedRegion(.leading) {
          MicBadge(state: context.state, size: 44)
        }
        DynamicIslandExpandedRegion(.center) {
          VStack(alignment: .leading, spacing: 2) {
            Text(context.attributes.pcName).font(.headline).lineLimit(1)
            Subtitle(state: context.state)
          }
        }
        DynamicIslandExpandedRegion(.bottom) {
          Buttons(state: context.state)
        }
      } compactLeading: {
        Image(systemName: context.state.muted ? "mic.slash.fill" : "mic.fill")
          .foregroundStyle(context.state.muted ? danger : live)
      } compactTrailing: {
        Text(context.state.connectedAt, style: .timer)
          .monospacedDigit()
          .frame(maxWidth: 48)
          .font(.caption2)
      } minimal: {
        Image(systemName: context.state.muted ? "mic.slash.fill" : "mic.fill")
          .foregroundStyle(context.state.muted ? danger : live)
      }
    }
  }
}

private struct MicBadge: View {
  let state: MicActivityAttributes.ContentState
  var size: CGFloat = 40

  var body: some View {
    ZStack {
      Circle().fill(state.muted ? danger : live)
      Image(systemName: state.muted ? "mic.slash.fill" : "mic.fill")
        .font(.system(size: size * 0.45, weight: .semibold))
        .foregroundStyle(.white)
    }
    .frame(width: size, height: size)
  }
}

private struct Subtitle: View {
  let state: MicActivityAttributes.ContentState

  var body: some View {
    HStack(spacing: 6) {
      if state.status == "reconnecting" {
        Text("Reconectando…")
      } else {
        Text(state.muted ? "Mutado" : "Microfone ativo")
        Text("·")
        Text(state.transportLabel)
        Text("·")
        Text(state.connectedAt, style: .timer).monospacedDigit()
      }
    }
    .font(.caption)
    .foregroundStyle(.secondary)
  }
}

private struct Buttons: View {
  let state: MicActivityAttributes.ContentState

  var body: some View {
    HStack(spacing: 10) {
      Button(intent: ToggleMuteIntent()) {
        Label(state.muted ? "Desmutar" : "Mutar", systemImage: state.muted ? "mic.fill" : "mic.slash.fill")
          .frame(maxWidth: .infinity)
      }
      .tint(state.muted ? live : danger)
      Button(intent: DisconnectIntent()) {
        Label("Desconectar", systemImage: "power")
          .frame(maxWidth: .infinity)
      }
      .tint(accent)
    }
    .font(.subheadline.weight(.semibold))
    .buttonStyle(.borderedProminent)
  }
}

private struct LockScreenView: View {
  let pcName: String
  let state: MicActivityAttributes.ContentState

  var body: some View {
    VStack(spacing: 12) {
      HStack(spacing: 12) {
        MicBadge(state: state)
        VStack(alignment: .leading, spacing: 2) {
          Text(pcName).font(.headline).foregroundStyle(.white).lineLimit(1)
          Subtitle(state: state)
        }
        Spacer()
      }
      Buttons(state: state)
    }
    .padding(16)
  }
}
