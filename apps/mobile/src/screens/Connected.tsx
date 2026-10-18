import React, { useEffect, useRef, useState } from 'react';
import { Animated, Platform, Pressable, StyleSheet, Text, Vibration, View } from 'react-native';
import { LEVEL_EVENT, MicNexus, events, type EngineState } from '../native/MicNexus';
import { Banner, Chip, Gradient, Icon, SecondaryButton } from '../ui/components';
import { colors, common, space } from '../ui/theme';

const TRANSPORT = {
  wifi: { icon: 'wifi', label: 'Wi‑Fi', color: colors.wifi },
  usb: { icon: 'usb', label: 'Cabo USB', color: '#c4a8ff' },
  bluetooth: { icon: 'bluetooth', label: 'Bluetooth', color: '#9aa8ff' },
} as const;

function duration(ms: number) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const pad = (n: number) => String(n).padStart(2, '0');
  return h ? `${h}:${pad(m)}:${pad(s % 60)}` : `${pad(m)}:${pad(s % 60)}`;
}

/** Connected (or reconnecting): the PC, the big mute button and the voice level. */
export function Connected({ state }: { state: EngineState }) {
  const t = state.target!;
  const reconnecting = state.status === 'reconnecting';
  const tr = TRANSPORT[t.transport] ?? TRANSPORT.wifi;
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  const latency = state.bufferMs != null ? Math.round(10 + (state.rttMs ?? 0) / 2 + state.bufferMs + 15) : null;

  return (
    <View style={{ flex: 1 }}>
      {reconnecting && <Banner tone="info" text={`Conexão instável. Reconectando a ${t.name}…`} />}
      {state.error && !reconnecting && <Banner text={state.error} onClose={() => MicNexus.clearError()} />}
      <View style={[common.card, styles.pc]}>
        <Gradient style={styles.pcIcon}>
          <Icon name="laptop" size={22} color="#fff" />
        </Gradient>
        <View style={{ flex: 1 }}>
          <Text style={styles.pcName} numberOfLines={1}>
            {t.name}
          </Text>
          <View style={[common.row, { gap: 8, marginTop: 5 }]}>
            <Chip icon={tr.icon} label={tr.label} color={tr.color} />
            {state.connectedAt > 0 && <Text style={styles.meta}>{duration(now - state.connectedAt)}</Text>}
            {latency != null && !reconnecting && <Text style={styles.meta}>~{latency} ms</Text>}
          </View>
        </View>
      </View>

      <View style={styles.center}>
        <MuteButton muted={state.muted} disabled={reconnecting} />
        <Text style={styles.label}>{reconnecting ? 'Reconectando…' : state.muted ? 'Microfone mutado' : 'Microfone ativo'}</Text>
        <Text style={common.muted}>{state.muted ? 'Toque para voltar a falar' : 'Toque para mutar'}</Text>
        <LevelBar muted={state.muted || reconnecting} />
      </View>

      <Text style={[common.faint, styles.hint]}>
        {Platform.OS === 'ios'
          ? 'Pode sair do app: Mutar e Desconectar ficam na tela de bloqueio e na Dynamic Island.'
          : 'Pode sair do app: a notificação mostra o microfone e tem os botões de mutar e desconectar.'}
      </Text>
      <SecondaryButton title="Desconectar" icon="power" danger onPress={() => MicNexus.disconnect()} />
    </View>
  );
}

/** Level 0..1 from Kotlin (~20/s), smoothed into an Animated.Value. */
function useLevel(): Animated.Value {
  const value = useRef(new Animated.Value(0)).current;
  useEffect(() => {
    let level = 0;
    const sub = events?.addListener(LEVEL_EVENT, (peak) => {
      const target = Math.min(1, Math.sqrt(peak as number) * 1.15);
      level = target > level ? target : level * 0.8 + target * 0.2;
      value.setValue(level);
    });
    return () => sub?.remove();
  }, [value]);
  return value;
}

function MuteButton({ muted, disabled }: { muted: boolean; disabled: boolean }) {
  const level = useLevel();
  const press = useRef(new Animated.Value(1)).current;
  const glowScale = level.interpolate({ inputRange: [0, 1], outputRange: [0.82, 1.3] });
  const glowOpacity = level.interpolate({ inputRange: [0, 1], outputRange: [0.25, 0.95] });
  return (
    <Pressable
      disabled={disabled}
      onPressIn={() => Animated.spring(press, { toValue: 0.94, useNativeDriver: true, speed: 40 }).start()}
      onPressOut={() => Animated.spring(press, { toValue: 1, useNativeDriver: true, speed: 20, bounciness: 8 }).start()}
      onPress={() => {
        Vibration.vibrate(12);
        MicNexus.setMuted(!muted);
      }}
      accessibilityRole="button"
      accessibilityLabel={muted ? 'Desmutar microfone' : 'Mutar microfone'}
      style={styles.muteWrap}
    >
      {!muted && !disabled && (
        <Animated.View style={[styles.glow, { opacity: glowOpacity, transform: [{ scale: glowScale }] }]} />
      )}
      <Animated.View style={{ transform: [{ scale: press }], opacity: disabled ? 0.5 : 1 }}>
        <Gradient
          style={styles.mute}
          from={muted ? '#f8667f' : '#34dc85'}
          to={muted ? '#d93a56' : '#1fa865'}
        >
          <Icon name={muted ? 'micOff' : 'mic'} size={64} color="#fff" strokeWidth={2.2} />
        </Gradient>
      </Animated.View>
    </Pressable>
  );
}

function LevelBar({ muted }: { muted: boolean }) {
  const level = useLevel();
  return (
    <View style={styles.bar}>
      {!muted && (
        <Animated.View
          style={[
            styles.barFill,
            { transform: [{ translateX: level.interpolate({ inputRange: [0, 1], outputRange: [-240, 0] }) }] },
          ]}
        />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  pc: { flexDirection: 'row', alignItems: 'center', gap: 14 },
  pcIcon: { width: 50, height: 50, borderRadius: 16, alignItems: 'center', justifyContent: 'center' },
  pcName: { color: colors.text, fontSize: 18, fontWeight: '800' },
  meta: { color: colors.textMuted, fontSize: 13, fontVariant: ['tabular-nums'] },
  center: { flex: 1, alignItems: 'center', justifyContent: 'center', paddingVertical: space.xl },
  muteWrap: { width: 250, height: 250, alignItems: 'center', justifyContent: 'center' },
  glow: { position: 'absolute', width: 250, height: 250, borderRadius: 125, backgroundColor: 'rgba(47,210,122,0.28)' },
  mute: { width: 176, height: 176, borderRadius: 88, alignItems: 'center', justifyContent: 'center' },
  label: { color: colors.text, fontSize: 22, fontWeight: '800', marginTop: 4, marginBottom: 4 },
  bar: { width: 240, height: 6, borderRadius: 3, backgroundColor: colors.surfaceRaised, overflow: 'hidden', marginTop: 22 },
  barFill: { width: 240, height: 6, borderRadius: 3, backgroundColor: colors.live },
  hint: { textAlign: 'center', marginBottom: 12, marginHorizontal: 12, lineHeight: 17 },
});
