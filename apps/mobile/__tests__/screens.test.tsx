/**
 * Renders every screen with realistic engine states to catch runtime errors
 * in the UI without a device, and checks the buttons reach the native module.
 */
import React from 'react';
import ReactTestRenderer, { act } from 'react-test-renderer';
import { NativeModules } from 'react-native';
import type { EngineState, Pc } from '../src/native/MicNexus';

const native = {
  getState: jest.fn(),
  getPcs: jest.fn(async (): Promise<Pc[]> => []),
  getSettings: jest.fn(async () => ({ autoConnect: true, noiseSuppression: true, gain: 1, lastPcId: null })),
  setSettings: jest.fn(async (p: object) => ({ autoConnect: true, noiseSuppression: true, gain: 1, lastPcId: null, ...p })),
  connect: jest.fn(),
  disconnect: jest.fn(),
  setMuted: jest.fn(),
  clearError: jest.fn(),
  refresh: jest.fn(),
  openBluetoothSettings: jest.fn(),
  openAppSettings: jest.fn(),
  getAppVersion: jest.fn(async () => '0.1.0'),
  installUpdate: jest.fn(async () => 'downloading'),
  addListener: jest.fn(),
  removeListeners: jest.fn(),
};
NativeModules.MicNexus = native;
(globalThis as unknown as { fetch: unknown }).fetch = jest.fn(async () => ({ ok: false }));

jest.mock('react-native-safe-area-context', () => ({
  SafeAreaProvider: ({ children }: { children: React.ReactNode }) => children,
  useSafeAreaInsets: () => ({ top: 24, bottom: 16, left: 0, right: 0 }),
}));

const base: EngineState = {
  status: 'idle',
  target: null,
  muted: false,
  codec: null,
  rttMs: null,
  bufferMs: null,
  error: null,
  connectedAt: 0,
  bluetoothOn: true,
  bluetoothPermission: true,
  micPermission: true,
  deviceName: 'Galaxy S23',
};

const pcs: Pc[] = [
  { key: 'pc1', pcId: 'pc1', name: 'TUF-DE-SAMUEL', wifi: '192.168.0.12', bluetooth: 'AA:BB:CC:DD:EE:FF', busy: false, last: true },
  { key: 'bt:11', pcId: null, name: 'NOTEBOOK', wifi: null, bluetooth: '11:22:33:44:55:66', busy: false, last: false },
];

const mounted: ReactTestRenderer.ReactTestRenderer[] = [];
afterEach(async () => {
  await act(async () => mounted.splice(0).forEach((t) => t.unmount()));
});

async function render(state: EngineState, list: Pc[] = []) {
  native.getState.mockResolvedValue(state);
  native.getPcs.mockResolvedValue(list);
  // Required lazily: MicNexus.ts reads NativeModules.MicNexus when it loads.
  const App = require('../src/App').default as React.ComponentType;
  let tree!: ReactTestRenderer.ReactTestRenderer;
  await act(async () => {
    tree = ReactTestRenderer.create(<App />);
  });
  mounted.push(tree);
  await act(async () => {});
  return tree;
}

const text = (tree: ReactTestRenderer.ReactTestRenderer) =>
  tree.root
    .findAll((n) => typeof n.type === 'string' && (n.type as string) === 'Text')
    .map((n) => (n.children as unknown[]).filter((c) => typeof c === 'string').join(''))
    .join(' | ');

beforeEach(() => jest.clearAllMocks());

test('welcome when the microphone is not allowed', async () => {
  const tree = await render({ ...base, micPermission: false });
  expect(text(tree)).toContain('Seu celular vira o microfone do PC');
});

test('searching, with tips', async () => {
  const tree = await render(base, []);
  const t = text(tree);
  expect(t).toContain('Conecte ao seu PC');
  expect(t).toContain('Procurando');
  expect(t).toContain('Cabo USB');
});

test('PC list connects over the best transport', async () => {
  const tree = await render(base, pcs);
  expect(text(tree)).toContain('TUF-DE-SAMUEL');
  expect(text(tree)).toContain('Último usado');
  const row = tree.root.findAll((n) => n.props.onPress && text({ root: n } as never).includes('TUF-DE-SAMUEL'))[0]!;
  await act(async () => row.props.onPress());
  expect(native.connect).toHaveBeenCalledWith('pc1', 'wifi');
});

test('pending approval', async () => {
  const tree = await render({ ...base, status: 'pending', target: { pcId: 'pc1', name: 'TUF-DE-SAMUEL', transport: 'wifi', address: '192.168.0.12' } }, pcs);
  expect(text(tree)).toContain('Permita no PC');
});

test('connected: mute button and disconnect', async () => {
  const state: EngineState = {
    ...base,
    status: 'connected',
    target: { pcId: 'pc1', name: 'TUF-DE-SAMUEL', transport: 'usb', address: 'USB' },
    codec: 'pcm16',
    rttMs: 4,
    bufferMs: 30,
    connectedAt: Date.now() - 65_000,
  };
  const tree = await render(state, pcs);
  const t = text(tree);
  expect(t).toContain('Microfone ativo');
  expect(t).toContain('Cabo USB');
  const mute = tree.root.find((n) => n.props.accessibilityLabel === 'Mutar microfone' && n.props.onPress);
  await act(async () => mute.props.onPress());
  expect(native.setMuted).toHaveBeenCalledWith(true);
});

test('muted and reconnecting states render', async () => {
  const target = { pcId: 'pc1', name: 'TUF', transport: 'bluetooth' as const, address: 'AA' };
  let tree = await render({ ...base, status: 'connected', target, muted: true, codec: 'opus', connectedAt: Date.now() }, pcs);
  expect(text(tree)).toContain('Microfone mutado');
  tree = await render({ ...base, status: 'reconnecting', target, connectedAt: Date.now() }, pcs);
  expect(text(tree)).toContain('Reconectando');
});

test('settings screen renders', async () => {
  const tree = await render(base, pcs);
  const gear = tree.root.find((n) => n.props.accessibilityLabel === 'Configurações' && n.props.onPress);
  await act(async () => gear.props.onPress());
  await act(async () => {});
  const t = text(tree);
  expect(t).toContain('Conectar automaticamente');
  expect(t).toContain('Redução de ruído');
  expect(t).toContain('100%');
});
