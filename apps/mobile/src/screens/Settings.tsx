import React, { useEffect, useRef, useState } from 'react';
import { PanResponder, Platform, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

const ios = Platform.OS === 'ios';
import { MicNexus, type EngineSettings } from '../native/MicNexus';
import { useUpdate, installUpdate, checkForUpdate } from '../lib/updater';
import { Icon, IconButton, Toggle } from '../ui/components';
import { colors, common, radius, space } from '../ui/theme';

export function Settings({ onClose, connected }: { onClose: () => void; connected: boolean }) {
  const [s, setS] = useState<EngineSettings | null>(null);
  const update = useUpdate();
  useEffect(() => {
    MicNexus.getSettings().then(setS).catch(() => {});
  }, []);
  const set = (patch: Partial<EngineSettings>) => {
    setS((cur) => (cur ? { ...cur, ...patch } : cur));
    MicNexus.setSettings(patch).then(setS).catch(() => {});
  };
  if (!s) return null;
  return (
    <View style={common.screen}>
      <View style={styles.head}>
        <IconButton name="arrowLeft" label="Voltar" onPress={onClose} color={colors.text} />
        <Text style={styles.headTitle}>Configurações</Text>
      </View>
      <ScrollView contentContainerStyle={{ padding: space.lg, paddingTop: 0 }}>
        <Text style={common.section}>Conexão</Text>
        <View style={[common.card, styles.group]}>
          <Toggle
            value={s.autoConnect}
            onChange={(v) => set({ autoConnect: v })}
            title="Conectar automaticamente"
            hint="Ao abrir o app, conecta no último PC usado. Com o cabo USB, conecta ao plugar."
          />
        </View>

        <Text style={common.section}>Microfone</Text>
        <View style={[common.card, styles.group]}>
          <Toggle
            value={s.noiseSuppression}
            onChange={(v) => set({ noiseSuppression: v })}
            title="Redução de ruído"
            hint="Usa o cancelamento de eco e a supressão de ruído do celular (como numa ligação)."
          />
          <View style={styles.divider} />
          <Text style={styles.label}>Volume do microfone</Text>
          <GainSlider value={s.gain} onChange={(gain) => set({ gain })} />
          {connected && <Text style={[common.faint, { marginBottom: 8 }]}>Vale a partir da próxima vez que o microfone abrir (mutar e desmutar aplica na hora).</Text>}
        </View>

        <Text style={common.section}>Atalhos</Text>
        <View style={[common.card, styles.group, { paddingVertical: 14 }]}>
          <View style={[common.row, { gap: 12 }]}>
            <Icon name="zap" size={20} color={colors.highlight} />
            {ios ? (
              <Text style={[common.muted, { flex: 1 }]}>
                Enquanto conectado, <Text style={{ color: colors.text, fontWeight: '700' }}>Mutar</Text> e{' '}
                <Text style={{ color: colors.text, fontWeight: '700' }}>Desconectar</Text> ficam na tela de bloqueio e na
                Dynamic Island. Se não aparecerem, ative “Atividades ao Vivo” do MicNexus nos Ajustes.
              </Text>
            ) : (
              <Text style={[common.muted, { flex: 1 }]}>
                Adicione o bloco <Text style={{ color: colors.text, fontWeight: '700' }}>MicNexus</Text> nas Configurações rápidas
                (puxe a barra de notificações e toque no lápis) para mutar sem abrir o app.
              </Text>
            )}
          </View>
        </View>

        {ios ? (
          <>
            <Text style={common.section}>Instalação</Text>
            <View style={[common.card, styles.link]}>
              <Icon name="info" size={20} color={colors.textMuted} />
              <Text style={[common.muted, { flex: 1 }]}>
                Instalado pelo Sideloadly com um Apple ID grátis, o app vale por 7 dias: reinstale (ou use o “auto-refresh”
                do Sideloadly) para renovar.
              </Text>
            </View>
          </>
        ) : (
          <>
            <Text style={common.section}>Bluetooth</Text>
            <Pressable style={[common.card, styles.link]} onPress={() => MicNexus.openBluetoothSettings()}>
              <Icon name="bluetooth" size={20} color={colors.textMuted} />
              <Text style={[common.text, { flex: 1 }]}>Parear com um PC</Text>
              <Icon name="external" size={18} color={colors.textFaint} />
            </Pressable>
          </>
        )}

        <Text style={common.section}>Sobre</Text>
        <View style={[common.card, styles.link]}>
          <Icon name="info" size={20} color={colors.textMuted} />
          <View style={{ flex: 1 }}>
            <Text style={common.text}>MicNexus {update.current ?? ''}</Text>
            <Text style={common.faint}>
              {update.available
                ? `Versão ${update.available.version} disponível`
                : update.status === 'downloading'
                  ? 'Baixando atualização…'
                  : 'Atualizado'}
            </Text>
          </View>
          {update.available ? (
            <Pressable style={styles.smallBtn} onPress={() => void installUpdate()}>
              <Text style={styles.smallBtnText}>Atualizar</Text>
            </Pressable>
          ) : (
            <Pressable style={styles.smallBtn} onPress={() => void checkForUpdate()}>
              <Text style={styles.smallBtnText}>Procurar</Text>
            </Pressable>
          )}
        </View>
      </ScrollView>
    </View>
  );
}

const MIN = 0.5;
const MAX = 4;

/** 50%..400% gain; the track is logarithmic so 100% sits a third of the way. */
function GainSlider({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  const [width, setWidth] = useState(1);
  const [local, setLocal] = useState(value);
  const widthRef = useRef(1);
  const toPos = (v: number) => Math.log(v / MIN) / Math.log(MAX / MIN);
  const fromPos = (p: number) => {
    const v = MIN * Math.pow(MAX / MIN, Math.max(0, Math.min(1, p)));
    return Math.abs(v - 1) < 0.06 ? 1 : Math.round(v * 20) / 20; // snap to 100%
  };
  const startRef = useRef(0);
  const localRef = useRef(value);
  localRef.current = local;
  const pan = useRef(
    PanResponder.create({
      onStartShouldSetPanResponder: () => true,
      onPanResponderGrant: (e) => {
        startRef.current = e.nativeEvent.locationX;
        const v = fromPos(e.nativeEvent.locationX / widthRef.current);
        setLocal(v);
      },
      onPanResponderMove: (_e, g) => setLocal(fromPos((startRef.current + g.dx) / widthRef.current)),
      onPanResponderRelease: () => onChange(localRef.current),
    }),
  ).current;
  const pos = toPos(local);
  return (
    <View style={{ paddingVertical: 8 }}>
      <View style={[common.row, { justifyContent: 'space-between', marginBottom: 6 }]}>
        <Text style={common.faint}>Mais baixo</Text>
        <Text style={[common.text, { fontWeight: '800' }]}>{Math.round(local * 100)}%</Text>
        <Text style={common.faint}>Mais alto</Text>
      </View>
      <View
        style={styles.track}
        onLayout={(e) => {
          widthRef.current = e.nativeEvent.layout.width;
          setWidth(e.nativeEvent.layout.width);
        }}
        {...pan.panHandlers}
      >
        <View pointerEvents="none" style={styles.rail} />
        <View pointerEvents="none" style={[styles.fill, { width: pos * width }]} />
        <View pointerEvents="none" style={[styles.knob, { left: pos * width - 13 }]} />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  head: { flexDirection: 'row', alignItems: 'center', gap: 6, paddingHorizontal: space.sm, height: 60 },
  headTitle: { color: colors.text, fontSize: 19, fontWeight: '800' },
  group: { paddingVertical: 4 },
  divider: { height: StyleSheet.hairlineWidth, backgroundColor: colors.border, marginVertical: 6 },
  label: { color: colors.text, fontSize: 15.5, fontWeight: '600', marginTop: 8 },
  link: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingVertical: 14 },
  smallBtn: { paddingHorizontal: 14, height: 34, borderRadius: radius.md, backgroundColor: colors.surfaceRaised, borderWidth: 1, borderColor: colors.border, justifyContent: 'center' },
  smallBtnText: { color: colors.text, fontWeight: '700' },
  track: { height: 36, justifyContent: 'center' },
  rail: { height: 6, borderRadius: 3, backgroundColor: colors.surfaceHover },
  fill: { position: 'absolute', height: 6, borderRadius: 3, backgroundColor: colors.accent },
  knob: { position: 'absolute', width: 26, height: 26, borderRadius: 13, backgroundColor: '#fff', borderWidth: 4, borderColor: colors.accent },
});
