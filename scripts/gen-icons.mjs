#!/usr/bin/env node
// Renders every icon from apps/desktop/assets/logo.svg (and the tray glyphs
// below): Windows app/tray icons and the Android launcher icons.
//
//   npm run icons
import { Resvg } from "@resvg/resvg-js";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const logo = readFileSync(join(root, "apps/desktop/assets/logo.svg"), "utf8");

const png = (svg, size) => new Resvg(svg, { fitTo: { mode: "width", value: size } }).render().asPng();
const write = (rel, data) => {
  const path = join(root, rel);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, data);
};

/** ICO with PNG entries (Windows Vista+). */
function ico(svg, sizes) {
  const images = sizes.map((s) => png(svg, s));
  const header = Buffer.alloc(6 + 16 * images.length);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);
  let offset = header.length;
  images.forEach((img, i) => {
    const e = 6 + i * 16;
    const s = sizes[i];
    header.writeUInt8(s >= 256 ? 0 : s, e);
    header.writeUInt8(s >= 256 ? 0 : s, e + 1);
    header.writeUInt16LE(1, e + 4);
    header.writeUInt16LE(32, e + 6);
    header.writeUInt32LE(img.length, e + 8);
    header.writeUInt32LE(offset, e + 12);
    offset += img.length;
  });
  return Buffer.concat([header, ...images]);
}

// ------------------------------------------------------------- Windows --
const icons = "apps/desktop/src-tauri/icons";
write(`${icons}/32x32.png`, png(logo, 32));
write(`${icons}/128x128.png`, png(logo, 128));
write(`${icons}/128x128@2x.png`, png(logo, 256));
write(`${icons}/icon.png`, png(logo, 512));
write(`${icons}/icon.ico`, ico(logo, [16, 24, 32, 48, 64, 128, 256]));
write("apps/desktop/public/logo.svg", logo);

// Tray: a colored tile reads on light and dark taskbars alike.
const MIC =
  '<path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3zM5 11a7 7 0 0 0 14 0M12 18v3" fill="none" stroke="#fff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>';
const MIC_OFF =
  '<path d="M3 3l18 18M9 9v3a3 3 0 0 0 5.1 2.1M15 9.3V6a3 3 0 0 0-5.7-1.3M5 11a7 7 0 0 0 11.6 5.3M19 11a7 7 0 0 1-.8 3.2M12 18v3" fill="none" stroke="#fff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>';
const tray = (fill, glyph, dot = "") =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#5468f5"/><stop offset="1" stop-color="#9b5cf6"/></linearGradient></defs><rect x="0.5" y="0.5" width="23" height="23" rx="6" fill="${fill}"/><g transform="translate(2.4 2.2) scale(0.8)">${glyph}</g>${dot}</svg>`;
write(`${icons}/tray-idle.png`, png(tray("#4b5070", MIC), 64));
write(`${icons}/tray-live.png`, png(tray("url(#g)", MIC, '<circle cx="19.5" cy="4.5" r="4" fill="#2fd27a" stroke="#0a0c14" stroke-width="1.2"/>'), 64));
write(`${icons}/tray-muted.png`, png(tray("#f4506b", MIC_OFF), 64));

// ------------------------------------------------------------- Android --
const res = "apps/mobile/android/app/src/main/res";
const densities = { mdpi: 48, hdpi: 72, xhdpi: 96, xxhdpi: 144, xxxhdpi: 192 };
const round = logo.replace('rx="224"', 'rx="480"');
for (const [d, size] of Object.entries(densities)) {
  write(`${res}/mipmap-${d}/ic_launcher.png`, png(logo, size));
  write(`${res}/mipmap-${d}/ic_launcher_round.png`, png(round, size));
}
write("apps/mobile/src/assets/logo.png", png(logo, 256));

console.log("icons written");
