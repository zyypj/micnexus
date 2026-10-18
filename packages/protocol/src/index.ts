/**
 * Shared MicNexus constants and the state shapes the native layers (Rust on
 * Windows, Kotlin on Android) hand to the UIs. Wire format: docs/PROTOCOL.md.
 */
export const PROTOCOL_VERSION = 1;
/** TCP port the phone listens on. */
export const PHONE_PORT = 47820;
/** UDP port the phone answers discovery on. */
export const DISCOVERY_PORT = 47821;
export const BLUETOOTH_UUID = "a3c8e0f2-5b17-4d9a-8e61-0c4f2b9d7a15";
export const ANDROID_PACKAGE = "app.micnexus.android";
export const IOS_BUNDLE_ID = "app.micnexus.ios";
export const BONJOUR_TYPE = "_micnexus._tcp";
export const BLE_SERVICE = "a3c8e0f2-5b17-4d9a-8e61-0c4f2b9d7a16";
export const BLE_RX = "a3c8e0f2-5b17-4d9a-8e61-0c4f2b9d7a17";
export const BLE_TX = "a3c8e0f2-5b17-4d9a-8e61-0c4f2b9d7a18";

export type Transport = "wifi" | "usb" | "bluetooth";
export type Codec = "pcm16" | "opus";

export const TRANSPORT_LABEL: Record<Transport, string> = {
  wifi: "Wi‑Fi",
  usb: "Cabo USB",
  bluetooth: "Bluetooth",
};

// ---------------------------------------------------------------- Windows --

export interface UsbDevice {
  serial: string;
  model: string;
  /** adb state, plus "no-app" when MicNexus is not installed on the phone,
   * and "closed" for an iPhone (usbmux) where the app is not open. */
  state: "device" | "unauthorized" | "offline" | "no-app" | "closed";
  /** `adb reverse` is in place: the phone can reach the PC through the cable. */
  forwarded: boolean;
}

export interface DesktopSession {
  deviceId: string;
  deviceName: string;
  model: string;
  transport: Transport;
  codec: Codec;
  rate: number;
  muted: boolean;
  connectedAt: number;
  rttMs: number | null;
  bufferMs: number;
  underruns: number;
}

export interface PendingDevice {
  deviceId: string;
  deviceName: string;
  model: string;
  address: string;
}

export type CableKind = "vbcable" | "voicemeeter" | "steam" | "vac" | "other";

export interface AudioEndpoint {
  id: string;
  name: string;
  /** Virtual cable this render endpoint belongs to, if any. */
  cable: CableKind | null;
  /** Capture side of the cable ("CABLE Output ..."), what other apps pick as microphone. */
  captureName: string | null;
  isDefault: boolean;
}

export interface LanPhone {
  id: string;
  name: string;
  model: string;
  address: string;
}

export interface DesktopState {
  pc: { id: string; name: string; version: string };
  /** The PC finds phones by broadcast and connects to them (no firewall rule needed). */
  wifi: { ok: boolean; addresses: string[]; phones: LanPhone[]; error: string | null };
  bluetooth: { state: "ready" | "off" | "unsupported" | "error"; error: string | null };
  /** Android over adb, iPhones over usbmux (`apple`: Apple's USB service is installed). */
  usb: { state: "ready" | "no-adb" | "error"; devices: UsbDevice[]; error: string | null; apple: boolean };
  session: DesktopSession | null;
  pending: PendingDevice | null;
  /** Where the phone's audio goes (normally a virtual cable's input). */
  output: AudioEndpoint | null;
  /** Whether the cable's capture side is currently the Windows default microphone. */
  isDefaultMic: boolean;
  cableInstall: "idle" | "downloading" | "installing" | "failed";
}

export interface DesktopSettings {
  pcName: string;
  outputId: string | null;
  setDefaultMic: boolean;
  monitor: boolean;
  monitorId: string | null;
  usbEnabled: boolean;
  usbAutoLaunch: boolean;
  wifiEnabled: boolean;
  bluetoothEnabled: boolean;
  startWithWindows: boolean;
  muteHotkey: string | null;
  notifyOnConnect: boolean;
  trusted: { id: string; name: string; addedAt: number }[];
}

export interface Level {
  /** 0..1 */
  peak: number;
  rms: number;
}

// ---------------------------------------------------------------- Android --

export type PhoneStatus = "idle" | "connecting" | "pending" | "connected" | "reconnecting";

export interface PcTarget {
  /** Stable key for lists: the PC id when known, otherwise transport:address. */
  key: string;
  pcId: string | null;
  name: string;
  transport: Transport;
  /** IP (Wi‑Fi), "127.0.0.1" (USB) or MAC (Bluetooth). */
  address: string;
  port: number;
  busy: boolean;
}

export interface PhoneState {
  status: PhoneStatus;
  target: PcTarget | null;
  muted: boolean;
  codec: Codec | null;
  rttMs: number | null;
  bufferMs: number | null;
  /** Last error or disconnect reason, cleared on the next connect. */
  error: string | null;
}

export interface PhoneSettings {
  autoConnect: boolean;
  noiseSuppression: boolean;
  /** Software gain, 0.5..4 (1 = unchanged). */
  gain: number;
  lastPcId: string | null;
}
