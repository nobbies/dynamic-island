// Membuat src/stations.default.js (daftar radio bawaan) dari data/daftar_radio_bali.js.
// DIJALANKAN OTOMATIS setiap `npm run tauri ...` (dev maupun build), jadi cukup edit data/daftar_radio_bali.js.
//
//   npm run stations            -> buat ulang daftar saja
//   npm run stations -- --icons -> sekaligus unduh semua ikon ke src/stations-icons/ agar ikut masuk installer
//
// Format data: [{ name, frequency, location, streamUrl, icon }, ...]
//   streamUrl = alamat stream audio langsung. Jika berupa alamat halaman web (radiostreaming.id/*.html atau
//   alamat situs tanpa jalur seperti https://situs.com/), disimpan sebagai `page` dan streamnya dicari aplikasi
//   saat stasiun diputar.
import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';
import { mkdir, readdir, rm, writeFile } from 'node:fs/promises';

const require = createRequire(import.meta.url);
const list = require('../data/daftar_radio_bali.js');
const withIcons = process.argv.includes('--icons');
const ICON_DIR = 'src/stations-icons';

const norm = (s) => String(s).toLowerCase().replace(/[^a-z0-9]/g, '');
// ID stabil dari nama (FNV-1a): tidak berubah walau urutan atau alamat di data berubah.
const hash = (s) => { let h = 0x811c9dc5; for (const c of s) { h ^= c.charCodeAt(0); h = Math.imul(h, 0x01000193); } return (h >>> 0) & 0x7fffffff; };
const pageLike = (u) => (u.host === 'radiostreaming.id' && /\.html?$/.test(u.pathname)) || (u.pathname === '/' && !u.port);

if (withIcons) await mkdir(ICON_DIR, { recursive: true });
const haveIcons = existsSync(ICON_DIR) ? await readdir(ICON_DIR) : [];

const out = [], ids = new Set(), names = new Set(), urls = new Set();
for (const r of list) {
  let u;
  try { u = new URL(String(r.streamUrl || '').trim()); } catch { console.warn('dilewati (alamat tidak valid):', r.name); continue; }
  if (!/^https?:$/.test(u.protocol)) { console.warn('dilewati (bukan http/https):', r.name); continue; }
  if (names.has(norm(r.name)) || urls.has(u.href)) { console.warn('dilewati (nama atau alamat ganda):', r.name); continue; }
  names.add(norm(r.name)); urls.add(u.href);

  let id = hash(norm(r.name));
  while (ids.has(id)) id++; // bentrok hash (sangat jarang)
  ids.add(id);

  const item = { id, name: r.name, frequency: r.frequency, location: r.location };
  if (pageLike(u)) item.page = u.href; else item.stream = u.href;
  item.icon = r.icon || '';

  if (withIcons && r.icon) {
    try {
      const res = await fetch(r.icon, { signal: AbortSignal.timeout(15000) });
      const type = res.headers.get('content-type') || '';
      if (!res.ok || !type.startsWith('image/')) throw new Error(`HTTP ${res.status} ${type}`);
      const ext = (type.split('/')[1] || 'webp').split(/[;+]/)[0];
      await writeFile(`${ICON_DIR}/${id}.${ext}`, Buffer.from(await res.arrayBuffer()));
      console.log('ikon OK    ', r.name);
      haveIcons.push(`${id}.${ext}`);
    } catch (e) { console.warn('ikon gagal ', r.name, '-', e.message); }
  }
  const local = haveIcons.find((f) => f.startsWith(`${id}.`));
  if (local) item.iconLocal = `stations-icons/${local}`; // ikon yang sudah ada di disk ikut dipakai (tidak hilang saat build biasa)
  out.push(item);
}

// buang ikon lokal milik stasiun yang sudah tidak ada di data
for (const f of haveIcons) if (!ids.has(parseInt(f, 10))) await rm(`${ICON_DIR}/${f}`, { force: true });

await writeFile('src/stations.default.js',
  '// Dibuat otomatis oleh scripts/build-stations.mjs dari data/daftar_radio_bali.js. Jangan diedit manual.\n' +
  'window.DEFAULT_STATIONS = ' + JSON.stringify(out, null, 2) + ';\n');
const pages = out.filter((s) => s.page).length;
console.log(`stasiun bawaan: ${out.length} (${out.length - pages} alamat stream langsung, ${pages} halaman web yang dicari otomatis)`);
