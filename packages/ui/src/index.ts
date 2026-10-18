/**
 * MicNexus visual identity: the Nexus navy surfaces and indigo → violet
 * gradient, plus a "live" green for the microphone and red for mute.
 */
export const colors = {
  bg: "#0a0c14",
  surface: "#11131f",
  surfaceRaised: "#181b2b",
  surfaceHover: "#1f2336",
  border: "#252a40",
  text: "#e9ebf8",
  textMuted: "#9aa0c3",
  textFaint: "#62688a",
  accent: "#5468f5",
  accentHover: "#6b7dff",
  accentText: "#ffffff",
  /** Second gradient stop (violet). */
  accent2: "#9b5cf6",
  highlight: "#22d3ee",
  /** Microphone open / connected. */
  live: "#2fd27a",
  /** Microphone muted / errors. */
  danger: "#f4506b",
  warning: "#f5b041",
  /** Transport colors (chips and badges). */
  wifi: "#22d3ee",
  usb: "#9b5cf6",
  bluetooth: "#5468f5",
} as const;

export const space = { xs: 4, sm: 8, md: 12, lg: 16, xl: 24, xxl: 32 } as const;
export const radius = { sm: 8, md: 12, lg: 18, xl: 26, round: 999 } as const;
export const font = {
  family: '"Segoe UI Variable", "Segoe UI", system-ui, sans-serif',
  mono: '"Cascadia Code", Consolas, monospace',
  size: { xs: 11, sm: 13, md: 14, lg: 16, xl: 20, xxl: 26 },
} as const;

export type ColorName = keyof typeof colors;

const kebab = (s: string) => s.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

/** CSS custom properties (`--c-text`, `--space-md`...) for the desktop app. */
export function cssVariables(): Record<string, string> {
  const vars: Record<string, string> = {};
  for (const [k, v] of Object.entries(colors)) vars[`--c-${kebab(k)}`] = v;
  for (const [k, v] of Object.entries(space)) vars[`--space-${k}`] = `${v}px`;
  for (const [k, v] of Object.entries(radius)) vars[`--radius-${k}`] = `${v}px`;
  for (const [k, v] of Object.entries(font.size)) vars[`--fs-${k}`] = `${v}px`;
  vars["--font"] = font.family;
  vars["--font-mono"] = font.mono;
  return vars;
}

export * from "./icons";
