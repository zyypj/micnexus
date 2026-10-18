import React, { useEffect, useState } from 'react';
import { BackHandler, Image, PermissionsAndroid, Platform, ScrollView, StatusBar, StyleSheet, Text, View, type Permission } from 'react-native';
import { SafeAreaProvider, useSafeAreaInsets } from 'react-native-safe-area-context';
import { useEngine } from './lib/useEngine';
import { checkForUpdate, installUpdate, useUpdate } from './lib/updater';
import { MicNexus, type EngineState } from './native/MicNexus';
import { Connected } from './screens/Connected';
import { Home } from './screens/Home';
import { Settings } from './screens/Settings';
import { Welcome } from './screens/Welcome';
import { Banner, IconButton } from './ui/components';
import { colors, common, radius, space } from './ui/theme';

const sdk = typeof Platform.Version === 'number' ? Platform.Version : 0;

async function requestPermissions(): Promise<'granted' | 'denied' | 'blocked'> {
  const wanted: Permission[] = [PermissionsAndroid.PERMISSIONS.RECORD_AUDIO];
  if (sdk >= 33) wanted.push(PermissionsAndroid.PERMISSIONS.POST_NOTIFICATIONS);
  if (sdk >= 31) wanted.push(PermissionsAndroid.PERMISSIONS.BLUETOOTH_CONNECT);
  const result = await PermissionsAndroid.requestMultiple(wanted);
  MicNexus.refresh();
  const mic = result[PermissionsAndroid.PERMISSIONS.RECORD_AUDIO];
  return mic === 'granted' ? 'granted' : mic === 'never_ask_again' ? 'blocked' : 'denied';
}

async function requestBluetooth() {
  if (sdk >= 31) await PermissionsAndroid.request(PermissionsAndroid.PERMISSIONS.BLUETOOTH_CONNECT);
  MicNexus.refresh();
}

export default function App() {
  return (
    <SafeAreaProvider>
      <StatusBar barStyle="light-content" />
      <Root />
    </SafeAreaProvider>
  );
}

function Root() {
  const insets = useSafeAreaInsets();
  const { state, pcs } = useEngine();
  const [view, setView] = useState<'home' | 'settings'>('home');
  const [blocked, setBlocked] = useState(false);
  const update = useUpdate();

  useEffect(() => {
    void checkForUpdate();
    const t = setInterval(() => void checkForUpdate(), 6 * 60 * 60 * 1000);
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    const sub = BackHandler.addEventListener('hardwareBackPress', () => {
      if (view === 'settings') {
        setView('home');
        return true;
      }
      return false;
    });
    return () => sub.remove();
  }, [view]);

  const pad = { paddingTop: insets.top, paddingBottom: insets.bottom };
  if (!state) return <View style={[common.screen, pad]} />;

  if (!state.micPermission) {
    return (
      <View style={[common.screen, pad]}>
        <Welcome
          denied={blocked}
          onSettings={() => MicNexus.openAppSettings()}
          onAllow={async () => setBlocked((await requestPermissions()) === 'blocked')}
        />
      </View>
    );
  }

  const live = state.status === 'connected' || state.status === 'reconnecting';
  if (view === 'settings') {
    return (
      <View style={[common.screen, pad]}>
        <Settings onClose={() => setView('home')} connected={live} />
      </View>
    );
  }

  return (
    <View style={[common.screen, pad]}>
      <Header state={state} onSettings={() => setView('settings')} />
      {live ? (
        <View style={styles.body}>
          <Connected state={state} />
        </View>
      ) : (
        <ScrollView contentContainerStyle={styles.scroll}>
          {update.available && (
            <Banner tone="info" text={`Nova versão ${update.available.version} disponível. Toque para atualizar.`} onClose={() => void installUpdate()} />
          )}
          <Home state={state} pcs={pcs} onAskBluetooth={() => void requestBluetooth()} />
        </ScrollView>
      )}
    </View>
  );
}

function Header({ state, onSettings }: { state: EngineState; onSettings: () => void }) {
  const live = state.status === 'connected';
  const tone = live ? (state.muted ? colors.danger : colors.live) : state.status === 'idle' ? colors.textFaint : colors.warning;
  const label = live
    ? state.muted
      ? 'Mutado'
      : 'Ao vivo'
    : state.status === 'reconnecting'
      ? 'Reconectando'
      : state.status === 'idle'
        ? 'Desconectado'
        : 'Conectando';
  return (
    <View style={styles.header}>
      <Image source={require('./assets/logo.png')} style={styles.logo} />
      <Text style={styles.brand}>MicNexus</Text>
      <View style={[styles.pill, { borderColor: tone + '55', backgroundColor: tone + '14' }]}>
        <View style={[styles.dot, { backgroundColor: tone }]} />
        <Text style={[styles.pillText, { color: tone }]}>{label}</Text>
      </View>
      <View style={{ flex: 1 }} />
      <IconButton name="settings" label="Configurações" onPress={onSettings} />
    </View>
  );
}

const styles = StyleSheet.create({
  header: { flexDirection: 'row', alignItems: 'center', gap: 10, paddingHorizontal: space.lg, height: 62 },
  logo: { width: 32, height: 32, borderRadius: 9 },
  brand: { color: colors.text, fontSize: 19, fontWeight: '800', letterSpacing: 0.2 },
  pill: { flexDirection: 'row', alignItems: 'center', gap: 6, borderWidth: 1, borderRadius: radius.round, paddingHorizontal: 9, paddingVertical: 3 },
  dot: { width: 7, height: 7, borderRadius: 4 },
  pillText: { fontSize: 11, fontWeight: '800', textTransform: 'uppercase', letterSpacing: 0.5 },
  scroll: { padding: space.lg, paddingTop: space.sm },
  body: { flex: 1, padding: space.lg, paddingTop: space.sm },
});
