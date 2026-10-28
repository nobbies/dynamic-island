// Membuat semua ikon aplikasi dari SVG.   Jalankan:  npm run icons
//
//   assets/icon.svg        ikon lengkap (dengan tulisan) -> ukuran besar (128 px ke atas), icns, logo Store, dll.
//   assets/icon-small.svg  varian sederhana (kapsul + equalizer) -> ukuran kecil 16/24/32 px agar tetap terbaca
//   ukuran 48/64 px        ikon lengkap tanpa tulisan
//
// Hasil: src-tauri/icons/* (termasuk icon.ico bertingkat) dan app-icon.png.
// Tulisan di SVG dirender dengan font yang ada di komputer ini (Inter jika terpasang, selain itu Arial).
import { Resvg } from '@resvg/resvg-js';
import pngToIco from 'png-to-ico';
import { execSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const full = readFileSync('assets/icon.svg', 'utf8');
const small = readFileSync('assets/icon-small.svg', 'utf8');
// varian tanpa tulisan: buang <text> dan garis hias di kiri-kanan wordmark
const mark = full
  .replace(/\s*<text[^>]*>[\s\S]*?<\/text>/g, '')
  .replace(/\s*<path d="M100 435 H166 M346 435 H412"[^>]*\/>/, '');

const png = (svg, size) =>
  new Resvg(svg, {
    fitTo: { mode: 'width', value: size },
    font: { loadSystemFonts: true, defaultFontFamily: 'Arial' },
    background: 'rgba(0,0,0,0)',
  }).render().asPng();

// 1) ikon dasar dari Tauri CLI (png, icns, Square*Logo, StoreLogo, ...)
const tmp = mkdtempSync(join(tmpdir(), 'icons-'));
const source = join(tmp, 'source.png');
writeFileSync(source, png(full, 1024));
execSync(`npx tauri icon "${source}" --output src-tauri/icons`, { stdio: 'inherit' });
rmSync('src-tauri/icons/android', { recursive: true, force: true }); // tidak dipakai (hanya desktop)
rmSync('src-tauri/icons/ios', { recursive: true, force: true });
rmSync(tmp, { recursive: true, force: true });

// 2) icon.ico bertingkat: tiap ukuran memakai gambar yang paling sesuai (Explorer, taskbar, dan installer memakai file ini)
const frames = [[16, small], [24, small], [32, small], [48, mark], [64, mark], [128, full], [256, full]].map(([s, svg]) => png(svg, s));
writeFileSync('src-tauri/icons/icon.ico', await pngToIco(frames));
writeFileSync('src-tauri/icons/32x32.png', png(small, 32));
writeFileSync('app-icon.png', png(full, 1024));
console.log('\nIkon selesai dibuat dari assets/icon.svg');
