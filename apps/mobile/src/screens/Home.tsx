import React from 'react';
import { ActivityIndicator, Platform, Pressable, StyleSheet, Text, View } from 'react-native';

const ios = Platform.OS === 'ios';
import { MicNexus, type EngineState, type Pc } from '../native/MicNexus';
import { Banner, Chip, Gradient, Icon, Pulse, SecondaryButton } from '../ui/components';
import { colors, common, radius, space } from '../ui/theme';

/** Disconnected: the PCs found (Wi‑Fi now, Bluetooth paired) and how to get one. */
export function Home({ state, pcs, onAskBluetooth }: { state: EngineState; pcs: Pc[]; onAskBluetooth: () => void }) {
  const busy = state.status === 'connecting' || state.status === 'pending';
  return (
    <View>
      <View style={[common.card, styles.hero]}>
        {busy ? (
          <Connecting state={state} />
        ) : (
          <>
            <Pulse size={112}>
              <Gradient style={styles.heroIcon}>
                <Icon name="laptop" size={30} color="#fff" />
              </Gradient>
            </Pulse>
            <Text style={[common.title, styles.center, { marginTop: 8 }]}>Conecte ao seu PC</Text>
            <Text style={[common.muted, styles.center, { marginTop: 6 }]}>
              Abra o MicNexus no Windows. Os computadores aparecem aqui sozinhos.
            </Text>
          </>
        )}
      </View>

      {state.error && !busy && <Banner text={state.error} onClose={() => MicNexus.clearError()} />}

      <Text style={common.section}>Computadores</Text>
      {pcs.length === 0 ? (
        <View style={[common.card, styles.empty]}>
          <View style={[common.row, { gap: 10, marginBottom: 14 }]}>
            <ActivityIndicator color={colors.accent} />
            <Text style={common.text}>Procurando…</Text>
          </View>
          <Tip icon="wifi" color={colors.wifi} title="Wi‑Fi" text="Celular e PC na mesma rede, com o MicNexus aberto no PC." />
          {ios ? (
            <>
              <Tip icon="usb" color="#c4a8ff" title="Cabo USB" text="Ligue o cabo com este app aberto. No PC é preciso o app Dispositivos Apple (o mesmo que o Sideloadly usa)." />
              <Tip icon="bluetooth" color="#9aa8ff" title="Bluetooth" text="Toque em “Conectar por Bluetooth” abaixo, com o Bluetooth do PC ligado." />
            </>
          ) : (
            <>
              <Tip icon="usb" color="#c4a8ff" title="Cabo USB" text="Ligue o cabo com a depuração USB ativa: o PC abre o app e conecta sozinho." />
              <Tip icon="bluetooth" color="#9aa8ff" title="Bluetooth" text="Pareie o celular com o PC uma vez nas configurações de Bluetooth." />
            </>
          )}
        </View>
      ) : (
        <View style={{ gap: 10 }}>
          {pcs.map((pc) => (
            <PcRow key={pc.key} pc={pc} disabled={busy} />
          ))}
        </View>
      )}

      {ios ? (
        <Pressable style={styles.btHint} disabled={busy} onPress={() => MicNexus.connect('ble:any', 'bluetooth')}>
          <Icon name="bluetooth" size={18} color={colors.accentHover} />
          <Text style={styles.btHintText}>Conectar por Bluetooth ao PC mais próximo</Text>
          <Icon name="chevronRight" size={18} color={colors.textFaint} />
        </Pressable>
      ) : !state.bluetoothPermission ? (
        <Pressable style={styles.btHint} onPress={onAskBluetooth}>
          <Icon name="bluetooth" size={18} color={colors.accentHover} />
          <Text style={styles.btHintText}>Permitir Bluetooth para ver os PCs pareados</Text>
          <Icon name="chevronRight" size={18} color={colors.textFaint} />
        </Pressable>
      ) : !state.bluetoothOn ? (
        <Pressable style={styles.btHint} onPress={() => MicNexus.openBluetoothSettings()}>
          <Icon name="bluetooth" size={18} color={colors.textFaint} />
          <Text style={styles.btHintText}>O Bluetooth do celular está desligado</Text>
          <Icon name="chevronRight" size={18} color={colors.textFaint} />
        </Pressable>
      ) : (
        <Pressable style={styles.btHint} onPress={() => MicNexus.openBluetoothSettings()}>
          <Icon name="bluetooth" size={18} color={colors.textFaint} />
          <Text style={styles.btHintText}>Parear um PC pelo Bluetooth</Text>
          <Icon name="chevronRight" size={18} color={colors.textFaint} />
        </Pressable>
      )}
    </View>
  );
}

function Connecting({ state }: { state: EngineState }) {
  const pending = state.status === 'pending';
  const name = state.target?.name ?? 'PC';
  return (
    <>
      <Pulse size={112} color={pending ? colors.warning : colors.accent}>
        <Gradient style={styles.heroIcon} from={pending ? '#f5b041' : colors.accent} to={pending ? '#f08a3c' : colors.accent2}>
          <Icon name={pending ? 'shield' : 'link'} size={30} color="#fff" />
        </Gradient>
      </Pulse>
      <Text style={[common.title, styles.center, { marginTop: 8 }]}>{pending ? 'Permita no PC' : `Conectando…`}</Text>
      <Text style={[common.muted, styles.center, { marginTop: 6, marginBottom: 16 }]}>
        {pending
          ? `Clique em "Permitir" no MicNexus do ${name}. É só na primeira vez.`
          : `${name} pelo ${state.target?.transport === 'bluetooth' ? 'Bluetooth' : state.target?.transport === 'usb' ? 'cabo USB' : 'Wi‑Fi'}`}
      </Text>
      <SecondaryButton title="Cancelar" onPress={() => MicNexus.disconnect()} />
    </>
  );
}

function PcRow({ pc, disabled }: { pc: Pc; disabled: boolean }) {
  const best: 'wifi' | 'bluetooth' = pc.wifi ? 'wifi' : 'bluetooth';
  const subtitle = pc.busy
    ? 'Em uso por outro celular'
    : pc.wifi
      ? pc.last
        ? 'Último usado · toque para conectar'
        : 'Toque para conectar'
      : ios
        ? 'Toque para conectar pelo Bluetooth'
        : 'Pareado · toque para conectar pelo Bluetooth';
  return (
    <Pressable
      disabled={disabled}
      onPress={() => MicNexus.connect(pc.key, best)}
      style={({ pressed }) => [common.card, styles.pc, pressed && { backgroundColor: colors.surfaceHover }, disabled && { opacity: 0.5 }]}
    >
      <Gradient style={styles.pcIcon}>
        <Icon name="laptop" size={22} color="#fff" />
      </Gradient>
      <View style={{ flex: 1 }}>
        <Text style={styles.pcName} numberOfLines={1}>
          {pc.name}
        </Text>
        <Text style={common.faint} numberOfLines={1}>
          {subtitle}
        </Text>
        <View style={[common.row, { gap: 6, marginTop: 6 }]}>
          {pc.wifi && <Chip icon="wifi" label="Wi‑Fi" color={colors.wifi} />}
          {pc.bluetooth && !ios && <Chip icon="bluetooth" label="Bluetooth" color="#9aa8ff" />}
        </View>
      </View>
      {pc.wifi && pc.bluetooth ? (
        <Pressable hitSlop={6} onPress={() => MicNexus.connect(pc.key, 'bluetooth')} style={styles.altBtn} accessibilityLabel="Conectar pelo Bluetooth">
          <Icon name="bluetooth" size={18} color={colors.textMuted} />
        </Pressable>
      ) : (
        <Icon name="chevronRight" size={20} color={colors.textFaint} />
      )}
    </Pressable>
  );
}

function Tip({ icon, color, title, text }: { icon: 'wifi' | 'usb' | 'bluetooth'; color: string; title: string; text: string }) {
  return (
    <View style={styles.tip}>
      <View style={[styles.tipIcon, { backgroundColor: color + '1f' }]}>
        <Icon name={icon} size={17} color={color} />
      </View>
      <View style={{ flex: 1 }}>
        <Text style={styles.tipTitle}>{title}</Text>
        <Text style={common.muted}>{text}</Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  hero: { alignItems: 'center', paddingVertical: space.xl, marginBottom: 12 },
  heroIcon: { width: 68, height: 68, borderRadius: 22, alignItems: 'center', justifyContent: 'center' },
  center: { textAlign: 'center' },
  empty: { paddingVertical: space.lg },
  tip: { flexDirection: 'row', gap: 12, paddingVertical: 8 },
  tipIcon: { width: 34, height: 34, borderRadius: 10, alignItems: 'center', justifyContent: 'center' },
  tipTitle: { color: colors.text, fontWeight: '700', fontSize: 15, marginBottom: 2 },
  pc: { flexDirection: 'row', alignItems: 'center', gap: 14, paddingVertical: 14 },
  pcIcon: { width: 48, height: 48, borderRadius: 15, alignItems: 'center', justifyContent: 'center' },
  pcName: { color: colors.text, fontSize: 17, fontWeight: '700', marginBottom: 2 },
  altBtn: { width: 40, height: 40, borderRadius: radius.md, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.surfaceRaised },
  btHint: { flexDirection: 'row', alignItems: 'center', gap: 10, padding: 14, marginTop: 12, borderRadius: radius.md, borderWidth: 1, borderColor: colors.border, borderStyle: 'dashed' },
  btHintText: { flex: 1, color: colors.textMuted, fontSize: 14 },
});
