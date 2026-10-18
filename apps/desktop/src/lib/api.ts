import type { AudioEndpoint, DesktopSettings, DesktopState, Level } from "@micnexus/protocol";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { mockBackend } from "./mock";

/** Commands of src-tauri/src/lib.rs. */
export interface Backend {
  getState(): Promise<DesktopState>;
  getSettings(): Promise<DesktopSettings>;
  setSettings(patch: Partial<DesktopSettings>): Promise<DesktopSettings>;
  listOutputs(): Promise<AudioEndpoint[]>;
  setMuted(muted: boolean): Promise<void>;
  disconnect(): Promise<void>;
  decide(accept: boolean): Promise<void>;
  forgetDevice(id: string): Promise<void>;
  installCable(): Promise<void>;
  openSoundSettings(): Promise<void>;
  openBluetoothSettings(): Promise<void>;
  bluetoothTurnOn(): Promise<void>;
  onState(cb: (s: DesktopState) => void): () => void;
  onLevel(cb: (l: Level) => void): () => void;
}

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function subscribe<T>(event: string, cb: (payload: T) => void): () => void {
  let stop: (() => void) | null = null;
  let cancelled = false;
  void listen<T>(event, (e) => cb(e.payload)).then((un) => {
    if (cancelled) un();
    else stop = un;
  });
  return () => {
    cancelled = true;
    stop?.();
  };
}

const tauriBackend: Backend = {
  getState: () => invoke("get_state"),
  getSettings: () => invoke("get_settings"),
  setSettings: (patch) => invoke("set_settings", { patch }),
  listOutputs: () => invoke("list_outputs"),
  setMuted: (muted) => invoke("set_muted", { muted }),
  disconnect: () => invoke("disconnect"),
  decide: (accept) => invoke("decide", { accept }),
  forgetDevice: (id) => invoke("forget_device", { id }),
  installCable: () => invoke("install_cable"),
  openSoundSettings: () => invoke("open_sound_settings"),
  openBluetoothSettings: () => invoke("open_bluetooth_settings"),
  bluetoothTurnOn: () => invoke("bluetooth_turn_on"),
  onState: (cb) => subscribe("state", cb),
  onLevel: (cb) => subscribe("level", cb),
};

/** The real backend inside the app; a scripted one in a plain browser (UI work). */
export const api: Backend = inTauri ? tauriBackend : mockBackend();
export const isMock = !inTauri;
