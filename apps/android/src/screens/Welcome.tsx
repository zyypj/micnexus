import React from 'react';
import { Image, StyleSheet, Text, View } from 'react-native';
import { PrimaryButton, SecondaryButton, Icon } from '../ui/components';
import { colors, common, space } from '../ui/theme';

/** First run / microphone permission missing. */
export function Welcome({ onAllow, denied, onSettings }: { onAllow: () => void; denied: boolean; onSettings: () => void }) {
  return (
    <View style={styles.wrap}>
      <Image source={require('../assets/logo.png')} style={styles.logo} />
      <Text style={[common.title, styles.center]}>Seu celular vira o microfone do PC</Text>
      <Text style={[common.muted, styles.center, { marginTop: 8, marginBottom: 28 }]}>
        Funciona pelo Wi‑Fi, pelo cabo USB ou pelo Bluetooth, com o app em segundo plano.
      </Text>
      <Step icon="mic" title="Microfone" text="Para enviar a sua voz ao PC. Só é usado enquanto estiver conectado." />
      <Step icon="info" title="Notificações" text="Mostram que o microfone está ligado, com os botões de mutar e desconectar." />
      <Step icon="bluetooth" title="Dispositivos próximos" text="Para conectar a um PC pareado pelo Bluetooth (opcional)." />
      <View style={{ height: 28 }} />
      {denied ? (
        <>
          <Text style={[common.muted, styles.center, { marginBottom: 12 }]}>
            O microfone foi negado. Ative-o em Permissões nas configurações do app.
          </Text>
          <SecondaryButton title="Abrir configurações" icon="settings" onPress={onSettings} />
        </>
      ) : (
        <PrimaryButton title="Permitir e começar" icon="check" onPress={onAllow} />
      )}
    </View>
  );
}

function Step({ icon, title, text }: { icon: 'mic' | 'info' | 'bluetooth'; title: string; text: string }) {
  return (
    <View style={styles.step}>
      <View style={styles.stepIcon}>
        <Icon name={icon} size={20} color={colors.accentHover} />
      </View>
      <View style={{ flex: 1 }}>
        <Text style={styles.stepTitle}>{title}</Text>
        <Text style={common.muted}>{text}</Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: { flex: 1, justifyContent: 'center', padding: space.xl },
  logo: { width: 88, height: 88, alignSelf: 'center', marginBottom: 20 },
  center: { textAlign: 'center' },
  step: { flexDirection: 'row', gap: 14, paddingVertical: 10 },
  stepIcon: { width: 40, height: 40, borderRadius: 12, backgroundColor: 'rgba(84,104,245,0.14)', alignItems: 'center', justifyContent: 'center' },
  stepTitle: { color: colors.text, fontWeight: '700', fontSize: 16, marginBottom: 2 },
});
