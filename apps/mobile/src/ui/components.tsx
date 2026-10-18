import { ICON_PATHS, type IconName } from '@micnexus/ui';
import React, { useEffect, useRef, type ReactNode } from 'react';
import { ActivityIndicator, Animated, Easing, Pressable, StyleSheet, Text, View, type ViewStyle } from 'react-native';
import Svg, { Defs, LinearGradient, Path, Rect, Stop } from 'react-native-svg';
import { colors, common, radius } from './theme';

export function Icon({ name, size = 20, color = colors.text, strokeWidth = 2 }: { name: IconName; size?: number; color?: string; strokeWidth?: number }) {
  return (
    <Svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round">
      <Path d={ICON_PATHS[name]} />
    </Svg>
  );
}

/** Indigo → violet fill behind its children (primary buttons, avatars). */
export function Gradient({ style, children, from = colors.accent, to = colors.accent2 }: { style?: ViewStyle; children?: ReactNode; from?: string; to?: string }) {
  return (
    <View style={[{ overflow: 'hidden' }, style]}>
      <Svg style={StyleSheet.absoluteFill} preserveAspectRatio="none" viewBox="0 0 100 100">
        <Defs>
          <LinearGradient id="g" x1="0" y1="0" x2="1" y2="1">
            <Stop offset="0" stopColor={from} />
            <Stop offset="1" stopColor={to} />
          </LinearGradient>
        </Defs>
        <Rect x="0" y="0" width="100" height="100" fill="url(#g)" />
      </Svg>
      {children}
    </View>
  );
}

export function PrimaryButton({ title, icon, onPress, disabled, busy }: { title: string; icon?: IconName; onPress: () => void; disabled?: boolean; busy?: boolean }) {
  return (
    <Pressable onPress={onPress} disabled={disabled || busy} style={({ pressed }) => [{ opacity: disabled ? 0.5 : pressed ? 0.85 : 1, transform: [{ scale: pressed ? 0.98 : 1 }] }]}>
      <Gradient style={common.button}>
        {busy ? <ActivityIndicator color="#fff" /> : icon ? <Icon name={icon} size={20} color="#fff" /> : null}
        <Text style={common.buttonText}>{title}</Text>
      </Gradient>
    </Pressable>
  );
}

export function SecondaryButton({ title, icon, onPress, danger }: { title: string; icon?: IconName; onPress: () => void; danger?: boolean }) {
  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [
        common.buttonSecondary,
        danger && { borderColor: 'rgba(244,80,107,0.35)', backgroundColor: 'rgba(244,80,107,0.08)' },
        pressed && { opacity: 0.8 },
      ]}
    >
      {icon && <Icon name={icon} size={18} color={danger ? colors.danger : colors.text} />}
      <Text style={[common.buttonSecondaryText, danger && { color: colors.danger }]}>{title}</Text>
    </Pressable>
  );
}

export function IconButton({ name, onPress, label, color = colors.textMuted }: { name: IconName; onPress: () => void; label: string; color?: string }) {
  return (
    <Pressable
      onPress={onPress}
      accessibilityLabel={label}
      hitSlop={8}
      style={({ pressed }) => [styles.iconBtn, pressed && { backgroundColor: colors.surfaceHover }]}
    >
      <Icon name={name} size={22} color={color} />
    </Pressable>
  );
}

export function Chip({ icon, label, color }: { icon: IconName; label: string; color: string }) {
  return (
    <View style={[styles.chip, { backgroundColor: color + '1f' }]}>
      <Icon name={icon} size={12} color={color} strokeWidth={2.4} />
      <Text style={[styles.chipText, { color }]}>{label}</Text>
    </View>
  );
}

/** Expanding rings behind an icon (searching / connecting). */
export function Pulse({ children, color = colors.accent, size = 120 }: { children: ReactNode; color?: string; size?: number }) {
  const rings = [useRef(new Animated.Value(0)).current, useRef(new Animated.Value(0)).current, useRef(new Animated.Value(0)).current];
  useEffect(() => {
    const anims = rings.map((v, i) =>
      Animated.loop(
        Animated.sequence([
          Animated.delay(i * 900),
          Animated.timing(v, { toValue: 1, duration: 2700, easing: Easing.out(Easing.quad), useNativeDriver: true }),
          Animated.timing(v, { toValue: 0, duration: 0, useNativeDriver: true }),
        ]),
      ),
    );
    anims.forEach((a) => a.start());
    return () => anims.forEach((a) => a.stop());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return (
    <View style={{ width: size, height: size, alignItems: 'center', justifyContent: 'center' }}>
      {rings.map((v, i) => (
        <Animated.View
          key={i}
          style={{
            position: 'absolute',
            width: size,
            height: size,
            borderRadius: size / 2,
            borderWidth: 2,
            borderColor: color,
            opacity: v.interpolate({ inputRange: [0, 1], outputRange: [0.8, 0] }),
            transform: [{ scale: v.interpolate({ inputRange: [0, 1], outputRange: [0.5, 1.2] }) }],
          }}
        />
      ))}
      {children}
    </View>
  );
}

export function Toggle({ value, onChange, title, hint }: { value: boolean; onChange: (v: boolean) => void; title: string; hint?: string }) {
  const anim = useRef(new Animated.Value(value ? 1 : 0)).current;
  useEffect(() => {
    Animated.spring(anim, { toValue: value ? 1 : 0, useNativeDriver: false, speed: 20, bounciness: 6 }).start();
  }, [value, anim]);
  return (
    <Pressable style={styles.toggleRow} onPress={() => onChange(!value)} accessibilityRole="switch" accessibilityState={{ checked: value }}>
      <View style={{ flex: 1 }}>
        <Text style={styles.toggleTitle}>{title}</Text>
        {hint && <Text style={styles.toggleHint}>{hint}</Text>}
      </View>
      <Animated.View
        style={[
          styles.track,
          { backgroundColor: anim.interpolate({ inputRange: [0, 1], outputRange: [colors.surfaceHover, colors.accent] }) },
        ]}
      >
        <Animated.View style={[styles.thumb, { transform: [{ translateX: anim.interpolate({ inputRange: [0, 1], outputRange: [0, 20] }) }] }]} />
      </Animated.View>
    </Pressable>
  );
}

export function Banner({ text, tone = 'error', onClose }: { text: string; tone?: 'error' | 'info'; onClose?: () => void }) {
  const color = tone === 'error' ? colors.danger : colors.highlight;
  return (
    <View style={[styles.banner, { borderColor: color + '66', backgroundColor: color + '14' }]}>
      <Icon name={tone === 'error' ? 'alert' : 'info'} size={18} color={color} />
      <Text style={[styles.bannerText]}>{text}</Text>
      {onClose && <IconButton name="x" label="Fechar" onPress={onClose} />}
    </View>
  );
}

const styles = StyleSheet.create({
  iconBtn: { width: 44, height: 44, borderRadius: 12, alignItems: 'center', justifyContent: 'center' },
  chip: { flexDirection: 'row', alignItems: 'center', gap: 4, paddingHorizontal: 8, paddingVertical: 3, borderRadius: radius.round },
  chipText: { fontSize: 11.5, fontWeight: '700' },
  toggleRow: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingVertical: 12 },
  toggleTitle: { color: colors.text, fontSize: 15.5, fontWeight: '600' },
  toggleHint: { color: colors.textMuted, fontSize: 13, marginTop: 2, lineHeight: 18 },
  track: { width: 48, height: 28, borderRadius: 14, padding: 3 },
  thumb: { width: 22, height: 22, borderRadius: 11, backgroundColor: '#fff' },
  banner: { flexDirection: 'row', alignItems: 'center', gap: 10, borderWidth: 1, borderRadius: radius.md, paddingLeft: 12, paddingVertical: 4, marginBottom: 12 },
  bannerText: { flex: 1, color: colors.text, fontSize: 14, lineHeight: 19, paddingVertical: 8 },
});
