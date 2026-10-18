import { NativeEventEmitter, NativeModules } from 'react-native';
import type { Codec, Transport } from '@micnexus/protocol';

/** Kotlin module: android/app/src/main/java/app/micnexus/android/MicNexusModule.kt */
export type Status = 'idle' | 'connecting' | 'pending' | 'connected' | 'reconnecting';

export interface EngineState {
  status: Status;
  target: { pcId: string | null; name: string; transport: Transport; address: string } | null;
  muted: boolean;
  codec: Codec | null;
  rttMs: number | null;
  bufferMs: number | null;
  error: string | null;
  connectedAt: number;
  bluetoothOn: boolean;
  bluetoothPermission: boolean;
  micPermission: boolean;
  deviceName: string;
}

export interface Pc {
  key: string;
  pcId: string | null;
  name: string;
  /** IP when found on Wi‑Fi right now. */
  wifi: string | null;
  /** MAC when paired over Bluetooth (Android); "ble" on iPhone (any PC may answer over BLE). */
  bluetooth: string | null;
  busy: boolean;
  /** The PC used last time. */
  last: boolean;
}

export interface EngineSettings {
  autoConnect: boolean;
  noiseSuppression: boolean;
  /** 0.5 .. 4 */
  gain: number;
  lastPcId: string | null;
}

interface Spec {
  getState(): Promise<EngineState>;
  getPcs(): Promise<Pc[]>;
  getSettings(): Promise<EngineSettings>;
  setSettings(patch: Partial<EngineSettings>): Promise<EngineSettings>;
  connect(key: string, transport: 'wifi' | 'bluetooth'): void;
  disconnect(): void;
  setMuted(muted: boolean): void;
  clearError(): void;
  refresh(): void;
  openBluetoothSettings(): void;
  openAppSettings(): void;
  getAppVersion(): Promise<string>;
  installUpdate(url: string, version: string): Promise<'permission' | 'downloading' | 'opened'>;
  /** iOS only (Android uses PermissionsAndroid). */
  requestPermissions?(): Promise<'granted' | 'denied' | 'blocked'>;
}

const mod: Spec | undefined = NativeModules.MicNexus;

function native(): Spec {
  if (!mod) throw new Error('MicNexus native module is not linked');
  return mod;
}

export const MicNexus = {
  getState: async () => native().getState(),
  getPcs: async () => native().getPcs(),
  getSettings: async () => native().getSettings(),
  setSettings: async (patch: Partial<EngineSettings>) => native().setSettings(patch),
  connect: (key: string, transport: 'wifi' | 'bluetooth') => mod?.connect(key, transport),
  disconnect: () => mod?.disconnect(),
  setMuted: (muted: boolean) => mod?.setMuted(muted),
  clearError: () => mod?.clearError(),
  refresh: () => mod?.refresh(),
  openBluetoothSettings: () => mod?.openBluetoothSettings(),
  openAppSettings: () => mod?.openAppSettings(),
  getAppVersion: async () => native().getAppVersion(),
  installUpdate: async (url: string, version: string) => native().installUpdate(url, version),
  requestPermissions: async () => (await native().requestPermissions?.()) ?? 'denied',
};

export const events = mod ? new NativeEventEmitter(NativeModules.MicNexus) : null;
export const STATE_EVENT = 'MicNexusState';
export const PCS_EVENT = 'MicNexusPcs';
export const LEVEL_EVENT = 'MicNexusLevel';
export const UPDATE_ERROR_EVENT = 'MicNexusUpdateError';
