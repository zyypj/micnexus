import { colors, radius, space } from '@micnexus/ui';
import { StyleSheet } from 'react-native';

export { colors, radius, space };

/** Same look as the Windows app: navy background, floating rounded cards. */
export const common = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.bg },
  row: { flexDirection: 'row', alignItems: 'center' },
  card: {
    backgroundColor: colors.surface,
    borderRadius: radius.lg,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: colors.border,
    padding: space.lg,
  },
  title: { color: colors.text, fontSize: 22, fontWeight: '800', letterSpacing: 0.2 },
  text: { color: colors.text, fontSize: 15 },
  muted: { color: colors.textMuted, fontSize: 14, lineHeight: 20 },
  faint: { color: colors.textFaint, fontSize: 12 },
  section: {
    color: colors.textFaint,
    fontSize: 12,
    fontWeight: '800',
    textTransform: 'uppercase',
    letterSpacing: 0.7,
    marginBottom: space.sm,
    marginTop: space.lg,
    marginLeft: 4,
  },
  button: {
    height: 50,
    borderRadius: radius.md,
    alignItems: 'center',
    justifyContent: 'center',
    flexDirection: 'row',
    gap: 8,
    overflow: 'hidden',
  },
  buttonText: { color: colors.accentText, fontWeight: '700', fontSize: 16 },
  buttonSecondary: {
    height: 48,
    borderRadius: radius.md,
    alignItems: 'center',
    justifyContent: 'center',
    flexDirection: 'row',
    gap: 8,
    backgroundColor: colors.surfaceRaised,
    borderWidth: 1,
    borderColor: colors.border,
  },
  buttonSecondaryText: { color: colors.text, fontWeight: '700', fontSize: 15 },
});
