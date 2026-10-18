import type { AudioEndpoint, DesktopSettings, DesktopState, Level } from "@micnexus/protocol";
import type { Backend } from "./api";

/**
 * Fake backend for working on the UI in a browser (`npm run dev`, no Tauri).
 * `?mock=idle|connected|muted|pending|nocable|usb-auth` picks the scenario.
 */
export function mockBackend(): Backend {
  const scenario = new URLSearchParams(location.search).get("mock") ?? "idle";
  const outputs: AudioEndpoint[] = [
    {
      id: "cable",
      name: "CABLE Input (VB-Audio Virtual Cable)",
      cable: "vbcable",
      captureName: "CABLE Output (VB-Audio Virtual Cable)",
      isDefault: false,
    },
    { id: "spk", name: "Alto-falantes (Realtek(R) Audio)", cable: null, captureName: null, isDefault: true },
  ];
  let state: DesktopState = {
    pc: { id: "pc", name: "TUF-DE-SAMUEL", version: "0.1.0" },
    wifi: { ok: true, addresses: ["192.168.0.12"], phones: [], error: null },
    bluetooth: { state: "off", error: null },
    usb: { state: "ready", devices: [], error: null, apple: false },
    session: null,
    pending: null,
    output: scenario === "nocable" ? null : outputs[0]!,
    isDefaultMic: false,
    cableInstall: "idle",
  };
  if (scenario === "connected" || scenario === "muted") {
    state.session = {
      deviceId: "p1",
      deviceName: "Galaxy S23",
      model: "SM-S911B",
      transport: "wifi",
      codec: "pcm16",
      rate: 48000,
      muted: scenario === "muted",
      connectedAt: Date.now() - 5 * 60_000,
      rttMs: 6,
      bufferMs: 32,
      underruns: 0,
    };
    state.isDefaultMic = true;
  }
  if (scenario === "pending") {
    state.pending = { deviceId: "p2", deviceName: "Pixel 8", model: "Pixel 8", address: "192.168.0.31" };
  }
  if (scenario === "usb-auth") {
    state.usb.devices = [{ serial: "R5CT", model: "SM S911B", state: "unauthorized", forwarded: false }];
  }
  if (scenario === "idle") {
    state.wifi.phones = [{ id: "p1", name: "Galaxy S23", model: "SM-S911B", address: "192.168.0.20" }];
  }
  let settings: DesktopSettings = {
    pcName: "",
    outputId: null,
    setDefaultMic: true,
    monitor: false,
    monitorId: null,
    usbEnabled: true,
    usbAutoLaunch: true,
    wifiEnabled: true,
    bluetoothEnabled: true,
    startWithWindows: true,
    muteHotkey: "Ctrl+Shift+M",
    notifyOnConnect: true,
    trusted: [{ id: "p1", name: "Galaxy S23", addedAt: Date.now() - 86_400_000 }],
  };
  const stateListeners = new Set<(s: DesktopState) => void>();
  const levelListeners = new Set<(l: Level) => void>();
  const push = (next: Partial<DesktopState>) => {
    state = { ...state, ...next };
    stateListeners.forEach((cb) => cb(state));
  };
  let t = 0;
  setInterval(() => {
    t += 0.05;
    const live = state.session && !state.session.muted;
    const peak = live ? Math.min(1, Math.abs(Math.sin(t * 3)) * (0.3 + 0.5 * Math.abs(Math.sin(t * 0.7)))) : 0;
    levelListeners.forEach((cb) => cb({ peak, rms: peak * 0.6 }));
  }, 50);

  return {
    getState: async () => state,
    getSettings: async () => settings,
    setSettings: async (patch) => {
      settings = { ...settings, ...patch };
      return settings;
    },
    listOutputs: async () => outputs,
    setMuted: async (muted) => {
      if (state.session) push({ session: { ...state.session, muted } });
    },
    disconnect: async () => push({ session: null }),
    decide: async () => push({ pending: null }),
    forgetDevice: async (id) => {
      settings = { ...settings, trusted: settings.trusted.filter((d) => d.id !== id) };
    },
    installCable: async () => {
      push({ cableInstall: "downloading" });
      setTimeout(() => push({ cableInstall: "installing" }), 1500);
      setTimeout(() => push({ cableInstall: "idle", output: outputs[0]! }), 4000);
    },
    openSoundSettings: async () => {},
    openBluetoothSettings: async () => {},
    bluetoothTurnOn: async () => push({ bluetooth: { state: "ready", error: null } }),
    onState: (cb) => {
      stateListeners.add(cb);
      return () => stateListeners.delete(cb);
    },
    onLevel: (cb) => {
      levelListeners.add(cb);
      return () => levelListeners.delete(cb);
    },
  };
}
