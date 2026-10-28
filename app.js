// Jembatan ke backend Rust (Tauri). Nama window.di dipertahankan agar sisa kode sama dengan versi Electron.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
window.di = {
  setMode: (mode) => invoke('set_mode', { mode }),
  onBlur: (cb) => listen('window-blurred', () => cb()),
  onMedia: (cb) => listen('media', (e) => cb(e.payload)),
  onCursor: (cb) => listen('cursor-inside', (e) => cb(e.payload)),
  autostartGet: () => invoke('autostart_status'),
  autostartSet: (enabled) => invoke('autostart_set', { enabled }),
  fetchIcon: (url, referer) => invoke('fetch_icon', { url, referer }),
  onSecond: (cb) => listen('second-instance', () => cb()),
  probe: (url) => invoke('probe', { url }),
  detect: (url) => invoke('detect', { url }),
  diagText: () => invoke('diag_text'),
  metricsStart: () => invoke('metrics_start'),
  metricsStop: () => invoke('metrics_stop'),
  onMetrics: (cb) => listen('metrics', (e) => cb(e.payload)),
  vaultLoad: () => invoke('vault_load'),
  vaultSave: (data) => invoke('vault_save', { data }),
  vaultReset: () => invoke('vault_reset'),
  aiKeyStatus: () => invoke('ai_key_status'),
  aiKeySet: (key) => invoke('ai_key_set', { key }),
  aiKeyClear: () => invoke('ai_key_clear'),
  aiAsk: (provider, endpoint, model, prompt) => invoke('ai_ask', { provider, endpoint, model, prompt }),
  onHotkey: (cb) => listen('hotkey', (e) => cb(e.payload)),
  quit: () => invoke('quit_app')
};

(() => {
const $ = (id) => document.getElementById(id);
const box = $('island');
// Dua elemen audio: `plain` (selalu aman) dan `cors` (terhubung ke AnalyserNode untuk waveform asli). Elemen `cors` hanya
// dipakai bila pengguna mengaktifkannya DAN server stream mengirim header CORS; selain itu audio asli tidak bisa dibaca.
const plain = new Audio(), cors = new Audio(); cors.crossOrigin = 'anonymous';
let audio = plain; // elemen yang sedang aktif

let saved = {};
try { saved = JSON.parse(localStorage.getItem('di-state') || '{}'); } catch {}
const S = { id: saved.id || 0, vol: saved.vol ?? .85, muted: false, auto: !!saved.auto, list: Array.isArray(saved.list) ? saved.list : [],
  dead: saved.dead || {}, seen: saved.seen || [], zoom: saved.zoom ?? 1.2, realWave: !!saved.realWave, noCors: saved.noCors || [],
  ai: saved.ai || { provider: 'anthropic', endpoint: '', model: '' }, mod: 'radio', isErr: false, theme: saved.theme || 'holo', light: false, startup: false, playing: false, expanded: false, lib: false, set: false, sleepIdx: 0, msg: '', retried: false };
const SLEEP = [0, 15, 30, 60];
let hideTimer, sleepTimer, logoFor = null, lastFail = 0, connTimer;
const save = () => { try { localStorage.setItem('di-state', JSON.stringify({ id: S.id, vol: S.vol, auto: S.auto, list: S.list, dead: S.dead, seen: S.seen, zoom: S.zoom, iconVer: 2, realWave: S.realWave, noCors: S.noCors, ai: S.ai, theme: S.theme })); } catch {} };
const applyVol = () => [plain, cors].forEach((e) => { e.volume = S.vol; e.muted = S.muted; });
applyVol();

// Daftar radio bawaan installer (src/stations.default.js). Dijalankan setiap aplikasi dibuka:
//  - stasiun bawaan yang SUDAH tersimpan diperbarui mengikuti data terbaru (alamat stream, nama, ikon), kecuali yang sudah
//    diubah pengguna (alamat manual, nama, atau ikon sendiri);
//  - stasiun bawaan yang sudah tidak ada di data dibuang (jika belum diubah pengguna);
//  - stasiun bawaan baru ditambahkan sekali; yang dihapus pengguna tidak muncul lagi;
//  - stasiun buatan pengguna tidak pernah disentuh (nama yang sama dengan bawaan: bawaan dilewati).
function syncDefaults() {
  const nm = (s) => String(s).toLowerCase().replace(/[^a-z0-9]/g, '');
  const defs = window.DEFAULT_STATIONS || [];
  const subOf = (d) => [d.frequency, d.location].filter(Boolean).join(' · ');
  const legacy = (e) => e.builtin || /radiostreaming\.id/.test(e.page || ''); // versi lama belum menandai `builtin`
  const matched = new Set(), next = [];
  for (const e of S.list) {
    if (!legacy(e)) { next.push(e); continue; }
    const d = defs.find((x) => x.id === e.id) || defs.find((x) => nm(x.name) === nm(e.title));
    if (!d || matched.has(d.id)) {
      if (e.manual || e.renamed || e.iconCustom) { e.builtin = false; next.push(e); } // sudah diubah pengguna: jadikan stasiun biasa
      continue;
    }
    matched.add(d.id);
    if (e.id !== d.id) { // ID baru dibuat dari nama: pindahkan semua rujukan ke ID lama
      if (S.id === e.id) S.id = d.id;
      if (S.dead[e.id]) { S.dead[d.id] = S.dead[e.id]; delete S.dead[e.id]; }
      e.id = d.id;
    }
    e.builtin = true; e.sub = subOf(d);
    if (!e.renamed) e.title = d.name;
    if (!e.manual) { // alamat yang diisi pengguna tidak ditimpa
      if (d.stream) { if (e.url !== d.stream || e.page) { e.url = d.stream; e.page = ''; delete S.dead[e.id]; } }
      else if (d.page && e.page !== d.page) { e.page = d.page; e.url = ''; delete S.dead[e.id]; }
    }
    if (!e.iconCustom && !e.iconOff) {
      if (d.iconLocal) { e.icon = d.iconLocal; e.iconSrc = d.icon || ''; }
      else if (d.icon && e.iconSrc !== d.icon) { e.iconSrc = d.icon; e.icon = null; e.iconTry = 0; }
    }
    next.push(e);
  }
  S.list = next;
  for (const d of defs) {
    const k = nm(d.name);
    if (matched.has(d.id)) { if (!S.seen.includes(k)) S.seen.push(k); continue; }
    if (S.seen.includes(k)) continue; // pernah ditambahkan lalu dihapus pengguna
    S.seen.push(k);
    if (S.list.some((r) => nm(r.title) === k)) continue; // pengguna sudah punya stasiun dengan nama yang sama
    S.list.push({ id: d.id, title: d.name, sub: subOf(d), url: d.stream || '', page: d.page || '', builtin: true,
      icon: d.iconLocal || null, iconSrc: d.icon || '' });
  }
  save();
}
syncDefaults();
if (saved.iconVer !== 2) { // pemrosesan ikon baru (margin putih dipangkas): ambil ulang ikon otomatis yang punya sumber
  for (const r of S.list) if (r.iconSrc && !r.iconCustom) { r.icon = null; r.iconTry = 0; }
  save();
}
if (!S.list.some((r) => r.id === S.id)) S.id = S.list[0] ? S.list[0].id : 0; // stasiun terpilih harus ada di daftar

const cur = () => S.list.find((r) => r.id === S.id) || S.list[0] || { id: 0, title: 'Belum ada stasiun', url: '' };
const accent = (id) => `hsl(${Math.round((id * 137.5) % 360)} 85% ${S.light ? 40 : 66}%)`; // tema terang memakai aksen lebih gelap
const isDead = (id) => Date.now() - (S.dead[id] || 0) < 864e5; // ditandai tidak aktif selama 24 jam
function setLogo(el, r) {
  const initial = () => { el.replaceChildren(); el.textContent = (r.title || '?').trim()[0] || '?'; el.style.background = accent(r.id); };
  if (!r.icon) return initial();
  const im = new Image(); im.alt = ''; im.onerror = initial; im.src = r.icon;
  el.style.background = '#fff'; el.replaceChildren(im);
}
// Perkecil ke 96x96. Margin transparan/putih di tepi dipangkas lebih dulu agar logo memenuhi kotak (tanpa bingkai putih).
function shrink(src) {
  return new Promise((res) => {
    const t = setTimeout(() => res(null), 3000), im = new Image();
    im.onload = () => {
      try {
        const iw = im.naturalWidth || 96, ih = im.naturalHeight || 96;
        const k0 = Math.min(1, 256 / Math.max(iw, ih)), W = Math.max(1, Math.round(iw * k0)), H = Math.max(1, Math.round(ih * k0));
        const wk = document.createElement('canvas'); wk.width = W; wk.height = H;
        const wx = wk.getContext('2d'); wx.drawImage(im, 0, 0, W, H);
        const px = wx.getImageData(0, 0, W, H).data;
        let x0 = W, y0 = H, x1 = -1, y1 = -1;
        for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
          const i = (y * W + x) * 4;
          if (px[i + 3] < 24 || (px[i] > 240 && px[i + 1] > 240 && px[i + 2] > 240)) continue; // transparan atau hampir putih
          if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y;
        }
        let sx = 0, sy = 0, sw = W, sh = H;
        if (x1 >= 0) { // kotak isi sebenarnya + ruang 4% di sekelilingnya
          const pad = Math.round(Math.max(x1 - x0 + 1, y1 - y0 + 1) * .04);
          sx = Math.max(0, x0 - pad); sy = Math.max(0, y0 - pad);
          sw = Math.min(W, x1 + 1 + pad) - sx; sh = Math.min(H, y1 + 1 + pad) - sy;
        }
        const n = 96, cv = document.createElement('canvas'); cv.width = cv.height = n;
        const k = Math.min(n / sw, n / sh), w = sw * k, h = sh * k;
        cv.getContext('2d').drawImage(wk, sx, sy, sw, sh, (n - w) / 2, (n - h) / 2, w, h);
        clearTimeout(t); res(cv.toDataURL('image/png'));
      } catch { clearTimeout(t); res(null); }
    };
    im.onerror = () => { clearTimeout(t); res(null); };
    im.src = src;
  });
}
const normIcon = async (src) => (await shrink(src)) || (src.length < 500000 ? src : null);
const readFile = (f) => new Promise((res, rej) => { const r = new FileReader(); r.onload = () => res(r.result); r.onerror = rej; r.readAsDataURL(f); });
let pendingIcon = null, picking = false;

const ICON_PLAY = '<svg viewBox="0 0 16 16"><path d="M4 2.5v11l9-5.5z"/></svg>';
const ICON_PAUSE = '<svg viewBox="0 0 16 16"><path d="M4 3h3v10H4zM9 3h3v10H9z"/></svg>';

function render() {
  const r = cur();
  box.style.setProperty('--accent', accent(r.id));
  box.style.setProperty('--zoom', S.zoom);
  $('zoom').value = S.zoom; $('zoomVal').textContent = Math.round(S.zoom * 100) + '%';
  box.classList.toggle('playing', S.playing);
  box.dataset.state = S.isErr ? 'error' : S.playing && S.msg ? 'wait' : S.playing ? 'live' : 'idle'; // kilau tepi = status (NFR-06)
  wcol = accent(r.id);
  $('miniName').textContent = $('title').textContent = r.title;
  if (logoFor !== r.id) { logoFor = r.id; setLogo($('logo'), r); }
  $('status').textContent = S.msg || (S.playing ? 'Siaran langsung' + (S.sleepIdx ? ` · tidur ${SLEEP[S.sleepIdx]} mnt` : '') : S.list.length ? 'Dijeda' : 'Tekan 🔍 Stasiun untuk menambah');
  $('miniPlay').innerHTML = $('play').innerHTML = S.playing ? ICON_PAUSE : ICON_PLAY;
  renderStrip();
  $('mute').textContent = S.muted || S.vol === 0 ? '🔇' : '🔊';
  $('vol').value = S.vol;
  for (const [id, on] of [['startup', S.startup], ['auto', S.auto], ['realWave', S.realWave]]) { $(id).classList.toggle('on', on); $(id).setAttribute('aria-checked', String(on)); }
  $('sleepSeg').replaceChildren(...SLEEP.map((m, i) => {
    const b = document.createElement('button');
    b.className = 'pill' + (i === S.sleepIdx ? ' on' : ''); b.textContent = m ? `${m} mnt` : 'Mati'; b.onclick = () => setSleep(i);
    return b;
  }));
  syncModules();
}

let startToken = 0, reresId = null, gotPlaying = false;
async function start() {
  const r = cur(), token = ++startToken;
  clearTimeout(connTimer); audio.pause(); S.msg = ''; S.isErr = false; S.retried = false;
  if (!S.playing || (!r.url && !r.page)) { audio.removeAttribute('src'); render(); return; }
  if (!r.url) { // stasiun bawaan: alamat stream dicari dari halaman sumbernya, lalu disimpan
    audio.removeAttribute('src'); S.msg = 'Mencari stream…'; render();
    let d = null; try { d = await window.di.detect(r.page); } catch {}
    if (token !== startToken) return; // pengguna sudah pindah stasiun atau menjeda
    if (!d) return failNow('Stream tidak ditemukan di halaman sumber');
    r.url = d.url; save();
    if (d.icon && !r.icon) normIcon(d.icon).then((n) => { if (n) { r.icon = n; save(); refreshLogos(r); } });
  }
  // Pilih elemen: `cors` hanya bila waveform asli diminta, belum pernah gagal untuk alamat ini, dan konteks audio aktif
  let el = plain;
  if (S.realWave && window.AudioContext && !S.noCors.includes(r.url)) { try { await ensureGraph(); el = cors; } catch { el = plain; } }
  if (token !== startToken) return;
  for (const x of [plain, cors]) if (x !== el) { x.pause(); x.removeAttribute('src'); }
  audio = el; gotPlaying = false; applyVol();
  audio.src = r.url;
  S.msg = 'Menyambung…'; render(); armTimeout();
  audio.play().catch((e) => e.name !== 'AbortError' && onFail());
}
function failNow(msg) {
  S.dead[S.id] = Date.now(); save();
  S.playing = false; S.msg = msg; S.isErr = true; render();
  if (S.lib) renderList();
}
// Jika server tidak mengirim audio dalam 10 detik, anggap gagal.
function armTimeout() {
  clearTimeout(connTimer);
  connTimer = setTimeout(() => { if (S.playing && audio.readyState < 3) onFail('Server tidak merespons (waktu habis)'); }, 10000);
}
const REASON = { 2: 'Jaringan terputus / server tidak merespons', 3: 'Format audio tidak bisa dibaca', 4: 'Alamat bukan stream audio, atau server mati' };
function onFail(why) {
  if (Date.now() - lastFail < 500) return; lastFail = Date.now(); // 'error' dan play() bisa memanggil bersamaan
  const url = cur().url;
  // Server Shoutcast lama di alamat akar ("http://host:port/") menjawab dengan halaman web; tambahkan "/;" agar mengalirkan audio.
  if (!why && !S.retried && /^https?:\/\/[^/?]+\/?(\?.*)?$/.test(url)) {
    S.retried = true; audio.src = url.replace(/\/?(\?.*)?$/, '/;');
    armTimeout(); audio.play().catch((e) => e.name !== 'AbortError' && onFail()); return;
  }
  const c = audio.error && audio.error.code, r = cur();
  console.log('[stream gagal]', url, 'kode:', c, audio.error ? audio.error.message : '');
  // Sekali per pemutaran (kecuali alamat diisi manual oleh pengguna): cari ulang alamat stream dari halaman sumber
  if (!r.manual && reresId !== r.id) {
    reresId = r.id;
    if (r.page) { r.url = ''; save(); start(); return; }
    // Alamat tersimpan mungkin ternyata halaman web (bukan stream): periksa, lalu cari stream di dalamnya
    const msg = why || REASON[c] || 'Stream tidak bisa diputar';
    window.di.probe(r.url).then((p) => {
      if (S.id !== r.id) return; // pengguna sudah pindah stasiun
      if (p && p.kind === 'html') { r.page = r.url; r.url = ''; save(); start(); } else failNow(msg);
    }).catch(() => { if (S.id === r.id) failNow(msg); });
    return;
  }
  failNow(why || REASON[c] || 'Stream tidak bisa diputar');
}
function wire(a) {
  a.addEventListener('waiting', () => { if (a === audio && S.playing) { S.msg = 'Buffering…'; render(); } });
  a.addEventListener('playing', () => {
    if (a !== audio) return;
    gotPlaying = true; clearTimeout(connTimer);
    if (S.dead[S.id]) { delete S.dead[S.id]; save(); }
    S.msg = ''; S.isErr = false; render(); if (S.lib) renderList();
  });
  a.addEventListener('error', () => {
    if (a !== audio || !S.playing || !a.getAttribute('src')) return;
    if (a === cors && !gotPlaying) { // server tanpa CORS: ingat alamatnya, lalu putar lewat elemen biasa (waveform = efek animasi)
      const u = cur().url;
      if (u && !S.noCors.includes(u)) { S.noCors.push(u); if (S.noCors.length > 100) S.noCors.shift(); save(); }
      start(); return;
    }
    onFail();
  });
}
wire(plain); wire(cors);

function play(id) { S.id = id; reresId = null; S.playing = true; save(); start(); if (S.lib) renderList(); }
function togglePlay() {
  if (!S.list.length) { S.msg = 'Tambahkan stasiun dulu'; render(); return; }
  S.playing = !S.playing; start();
}
function step(d) {
  const q = S.list.filter((r) => r.id === S.id || !isDead(r.id)); // lewati stasiun yang tidak aktif
  if (!q.length) return;
  let n = q.findIndex((r) => r.id === S.id);
  n = n < 0 ? (d > 0 ? 0 : q.length - 1) : (n + d + q.length) % q.length;
  play(q[n].id);
}

// ---- Stasiun saya: daftar + tambah manual
function renderList() {
  const q = $('q').value.trim().toLowerCase();
  const items = q ? S.list.filter((r) => `${r.title} ${r.sub || ''}`.toLowerCase().includes(q)) : S.list;
  $('count').textContent = q ? `${items.length} dari ${S.list.length} stasiun` : S.list.length ? `${S.list.length} stasiun` : '';
  if (!items.length) {
    const e = document.createElement('div'); e.className = 'empty';
    e.textContent = q ? `Tidak ada stasiun untuk “${$('q').value.trim()}”.` : 'Belum ada stasiun. Tekan ＋ Tambah, lalu tempel alamat stream.';
    $('list').replaceChildren(e); return;
  }
  $('list').replaceChildren(...items.map((r) => {
    const row = document.createElement('div'); row.className = 'item' + (r.id === S.id ? ' on' : ''); row.dataset.id = r.id;
    const lg = document.createElement('div'); lg.className = 'logo sm'; setLogo(lg, r);
    const t = document.createElement('span'); t.className = 't'; t.title = r.url || r.page || '';
    const nm = document.createElement('span'); nm.className = 'nm'; nm.textContent = r.title + (isDead(r.id) ? '  ⚠ tidak aktif' : '');
    t.append(nm);
    if (r.sub) { const sm = document.createElement('small'); sm.textContent = r.sub; t.append(sm); }
    const del = document.createElement('button'); del.className = 'del'; del.textContent = '✕'; del.setAttribute('aria-label', 'Hapus stasiun');
    del.onclick = (e) => { e.stopPropagation(); removeStation(r.id); };
    const ed = document.createElement('button'); ed.className = 'del'; ed.textContent = '✎'; ed.title = 'Edit stasiun'; ed.setAttribute('aria-label', 'Edit stasiun');
    ed.onclick = (e) => { e.stopPropagation(); startEdit(r); };
    row.onclick = () => play(r.id);
    row.append(lg, t, ed, del); return row;
  }));
}
let adding = false, editId = null;
const PH_UR = 'Alamat stream, mis. http://server:8000/live', PH_IC = 'Ikon (opsional): URL gambar, atau pilih file →';
function clearForm() { $('nm').value = $('ur').value = $('ic').value = ''; pendingIcon = null; $('err').textContent = ''; $('ur').placeholder = PH_UR; $('ic').placeholder = PH_IC; }
// on=true,id=null: form tambah. on=true,id=<id>: form edit stasiun itu. on=false: tutup.
function setAdding(on, id = null) {
  if (editId != null && (!on || id == null)) clearForm(); // sisa isian dari mode edit dibuang
  adding = on; editId = on ? id : null;
  const edit = on && id != null;
  $('libv').classList.toggle('adding', on); $('libv').classList.toggle('editing', edit);
  $('toggleAdd').textContent = on ? '✕ Tutup' : '＋ Tambah';
  $('libTitle').textContent = edit ? 'Edit stasiun' : 'Stasiun';
  $('add').textContent = edit ? 'Simpan perubahan' : 'Tambah dan putar';
  if (on) setTimeout(() => $('ur').focus(), 60);
}
function startEdit(r) {
  setAdding(true, r.id); clearForm();
  $('nm').value = r.title; $('ur').value = r.url || '';
  $('ur').placeholder = r.page ? 'Kosongkan = cari otomatis dari halaman sumber' : 'Alamat stream';
  $('ic').placeholder = 'Ikon: kosong = tidak diubah. URL gambar, atau pilih file →';
  $('reStream').hidden = !r.page;
}
// ---- Strip stasiun (deretan ikon)
let stripKey = '', stripFor = null;
function renderStrip() {
  const strip = $('strip'); strip.hidden = !S.list.length;
  const key = S.list.map((r) => r.id).join(',');
  if (key !== stripKey) { // bangun ulang hanya jika daftar berubah (supaya posisi gulir tidak melompat)
    stripKey = key; const sl = strip.scrollLeft;
    strip.replaceChildren(...S.list.map((r) => {
      const b = document.createElement('button'); b.className = 'tile'; b.dataset.id = r.id; b.title = r.title; b.setAttribute('aria-label', r.title);
      const lg = document.createElement('div'); lg.className = 'logo'; setLogo(lg, r); b.append(lg);
      b.onclick = () => play(r.id); return b;
    }));
    strip.scrollLeft = sl; stripFor = null;
  }
  const byId = new Map(S.list.map((r) => [r.id, r]));
  strip.querySelectorAll('.tile').forEach((b) => {
    const id = +b.dataset.id, r = byId.get(id);
    b.classList.toggle('on', id === S.id); b.classList.toggle('dead', isDead(id));
    if (r) { b.title = r.title; b.setAttribute('aria-label', r.title); }
  });
  if (stripFor !== S.id) { // tengahkan stasiun aktif (dihitung manual agar tidak menggeser elemen lain)
    stripFor = S.id; const on = strip.querySelector('.tile.on');
    if (on) strip.scrollTo({ left: on.offsetLeft - (strip.clientWidth - on.offsetWidth) / 2, behavior: 'smooth' });
  }
}
// Perbarui logo stasiun di semua tempat (strip, daftar, header) tanpa membangun ulang apa pun.
function refreshLogos(r) {
  document.querySelectorAll(`[data-id="${r.id}"] .logo`).forEach((el) => setLogo(el, r));
  if (r.id === S.id) { logoFor = null; render(); }
}

// ---- Ikon stasiun
const norm = (s) => String(s).toLowerCase().replace(/[^a-z0-9]/g, '');
// Stasiun milik pengguna yang namanya cocok dengan daftar bawaan (mis. "Menara FM Bali" ~ "Menara FM") memakai ikonnya.
function builtinFor(r) {
  const n = norm(r.title);
  return (window.DEFAULT_STATIONS || []).find((d) => {
    const m = norm(d.name);
    return m === n || (Math.min(m.length, n.length) >= 6 && (m.startsWith(n) || n.startsWith(m)));
  }) || null;
}
// Cari ikon untuk stasiun yang belum punya (3 sekaligus). `force` mengabaikan jeda percobaan ulang (6 jam).
async function fillIcons(force = false) {
  const todo = S.list.filter((r) => !r.icon && !r.iconOff && (force || Date.now() - (r.iconTry || 0) > 216e5));
  let i = 0, ok = 0; const fail = [];
  const worker = async () => {
    while (i < todo.length) {
      const r = todo[i++]; r.iconTry = Date.now();
      try {
        let src = r.iconSrc, local = null, ref = r.page;
        const b = builtinFor(r);
        if (b) { if (!src) { src = b.icon; ref = b.page; } local = b.iconLocal || null; } // ikon yang ikut installer diutamakan
        if (!ref && src) { try { ref = new URL(src).origin + '/'; } catch {} }
        let data = null;
        if (local) data = local;                                       // ikon yang ikut di dalam installer
        else if (src) data = await window.di.fetchIcon(src, ref);      // unduh dari CDN (dengan Referer halaman asal)
        else if (r.url) { const p = await window.di.probe(r.url); data = p && p.icon; if (!data) throw 'server tidak menyebut logo'; }
        else throw 'tidak ada sumber ikon';
        const n = local ? data : await normIcon(data);
        if (!n) throw 'gambar tidak bisa dibaca';
        r.icon = n; if (src && !r.iconSrc) r.iconSrc = src; ok++;
      } catch (e) { fail.push(`${r.title}: ${e}`); }
      save(); refreshLogos(r);
    }
  };
  await Promise.all(Array.from({ length: 3 }, worker));
  return { ok, fail, total: todo.length };
}
// Periksa alamat yang diketik: validasi, lalu (jika ternyata halaman web) cari stream di dalamnya.
// Mengembalikan {url, autoIcon, note, host}, atau null bila gagal (pesan sudah ditampilkan lewat err).
async function checkUrl(raw, err) {
  let u;
  try { u = new URL(raw.trim()); } catch { err('Alamat tidak valid. Awali dengan http:// atau https://'); return null; }
  if (!/^https?:$/.test(u.protocol)) { err('Hanya alamat http:// atau https:// yang didukung.'); return null; }
  if (/\.(pls|m3u8?)$/i.test(u.pathname) || /format=pls/i.test(u.search)) { err('Itu playlist (.pls/.m3u). Gunakan alamat stream audio langsung.'); return null; }
  let url = u.href, note = '', autoIcon = null;
  err('Memeriksa alamat…');
  try {
    const p = await window.di.probe(url);
    if (p.kind === 'html') { // yang ditempel adalah halaman web, bukan stream
      err('Itu halaman web. Mencari alamat stream di dalamnya…');
      const d = await window.di.detect(url);
      if (!d) { err('Itu halaman web dan alamat audionya tidak ditemukan. Cari manual: buka halamannya di browser, F12 → Network → filter Media → tekan play.'); return null; }
      url = d.url; note = 'Stream ditemukan otomatis.'; autoIcon = d.icon || null;
    } else autoIcon = p.icon || null;
  } catch { /* pemeriksaan gagal: tetap coba putar langsung */ }
  err('');
  return { url, autoIcon, note, host: u.hostname };
}
// Ikon dari isian form: file pilihan > URL yang diketik > ikon otomatis.
async function iconFromForm(autoIcon) {
  let icon = pendingIcon, note = ''; const typed = $('ic').value.trim();
  if (!icon && /^https?:\/\//i.test(typed)) {
    try { icon = await window.di.fetchIcon(typed); } catch (e) { note = `Ikon dari URL tidak bisa dimuat (${e}).`; }
  }
  const custom = !!icon; icon = icon || autoIcon;
  if (icon) icon = await normIcon(icon);
  return { icon: icon || null, note, custom };
}
async function addStation() {
  if ($('add').disabled) return;
  const err = (t) => { $('err').textContent = t; };
  $('add').disabled = true;
  try {
    const c = await checkUrl($('ur').value, err); if (!c) return;
    if (S.list.some((r) => r.url === c.url)) return err('Alamat ini sudah ada di daftar.');
    const ic = await iconFromForm(c.autoIcon);
    const id = Date.now();
    S.list.push({ id, title: $('nm').value.trim() || c.host, url: c.url, icon: ic.icon, iconCustom: ic.custom || undefined });
    $('nm').value = $('ur').value = $('ic').value = ''; pendingIcon = null;
    err([c.note, ic.note].filter(Boolean).join(' '));
    play(id); $('q').value = ''; renderList(); setTimeout(() => fillIcons(), 300);
    if (!ic.note) setAdding(false);
    $('list').scrollTop = 1e6;
  } finally { $('add').disabled = false; }
}
// Simpan perubahan stasiun: nama, alamat stream, dan ikon.
async function saveEdit() {
  if ($('add').disabled) return;
  const r = S.list.find((x) => x.id === editId); if (!r) return setAdding(false);
  const err = (t) => { $('err').textContent = t; };
  $('add').disabled = true;
  try {
    const raw = $('ur').value.trim(); let urlChanged = false, autoIcon = null, note = '';
    if (raw && raw !== (r.url || '')) {
      const c = await checkUrl(raw, err); if (!c) return;
      if (S.list.some((x) => x.id !== r.id && x.url === c.url)) return err('Alamat ini sudah dipakai stasiun lain.');
      r.url = c.url; r.manual = true; urlChanged = true; autoIcon = c.autoIcon; note = c.note; // alamat manual tidak ditimpa pencarian otomatis
    } else if (!raw) {
      if (!r.page) return err('Alamat stream tidak boleh kosong.');
      if (r.url) { r.url = ''; r.manual = false; urlChanged = true; } // dikosongkan: kembali mencari otomatis dari halaman sumber
    }
    const name = $('nm').value.trim(); if (name && name !== r.title) { r.title = name; r.renamed = true; }
    const ic = await iconFromForm(null);
    if (ic.icon) { r.icon = ic.icon; r.iconOff = false; r.iconCustom = ic.custom || r.iconCustom; }
    else if (autoIcon && !r.icon && !r.iconOff) r.icon = await normIcon(autoIcon);
    delete S.dead[r.id]; save();
    refreshLogos(r); render(); renderList();
    const msg = [note, ic.note].filter(Boolean).join(' ');
    if (urlChanged) play(r.id); // uji alamat baru langsung
    if (ic.note) { err(msg); return; } // biarkan form terbuka agar kegagalan ikon terbaca
    setAdding(false); // menutup form juga membersihkan isiannya
  } finally { $('add').disabled = false; }
}
function removeStation(id) {
  S.list = S.list.filter((r) => r.id !== id); delete S.dead[id];
  if (S.id === id) { S.playing = false; S.id = S.list[0] ? S.list[0].id : 0; start(); }
  save(); render(); renderList();
}
$('pick').onclick = () => { picking = true; $('file').click(); };
$('file').addEventListener('cancel', () => { picking = false; });
$('file').onchange = async () => {
  const f = $('file').files[0]; $('file').value = ''; picking = false;
  const err = (t) => { $('err').textContent = t; };
  if (!f) return;
  if (f.size > 3e6) return err('Gambar terlalu besar (maksimal 3 MB).');
  let d = null; try { d = await normIcon(await readFile(f)); } catch {}
  if (!d) return err('Gambar tidak bisa dibaca.');
  pendingIcon = d; $('ic').value = '✓ ' + f.name; err('');
};
function openSet() {
  refreshAiState();
  S.set = S.expanded = true; S.lib = false; window.di.setMode('settings'); setMode('settings'); render();
}
function closeSet() {
  S.set = false; setMode('expanded');
  setTimeout(() => { if (S.expanded && !S.lib && !S.set) window.di.setMode('expanded'); }, 320);
}
function setSleep(i) {
  clearTimeout(sleepTimer); S.sleepIdx = i;
  if (SLEEP[i]) sleepTimer = setTimeout(() => { S.playing = false; S.sleepIdx = 0; start(); }, SLEEP[i] * 60000);
  render();
}
function openLib() {
  S.lib = S.expanded = true; S.set = false; window.di.setMode('library'); setMode('library'); renderList();
  setAdding(!S.list.length);
}
function closeLib() {
  S.lib = false; setMode('expanded');
  setTimeout(() => { if (S.expanded && !S.lib && !S.set) window.di.setMode('expanded'); }, 320);
}

// ---- Tema: hanya mengatur <html data-theme>; semua warna ada di style.css
const THEMES = { auto: 'Otomatis (mengikuti Windows)', holo: 'Holografik', midnight: 'Midnight (hitam polos)', light: 'Terang', ocean: 'Samudra', sunset: 'Senja' };
const sysLight = window.matchMedia ? window.matchMedia('(prefers-color-scheme: light)') : null;
function applyTheme() {
  const t = THEMES[S.theme] ? S.theme : 'holo';
  const eff = t === 'auto' ? (sysLight && sysLight.matches ? 'light' : 'holo') : t;
  document.documentElement.dataset.theme = eff; S.light = eff === 'light';
  document.querySelectorAll('.sw').forEach((b) => { const on = b.dataset.theme === t; b.classList.toggle('on', on); b.setAttribute('aria-checked', String(on)); });
  $('themeName').textContent = THEMES[t];
}
document.querySelectorAll('.sw').forEach((b) => { b.onclick = () => { S.theme = b.dataset.theme; save(); applyTheme(); render(); }; });
if (sysLight) sysLight.addEventListener('change', () => { if (S.theme === 'auto') { applyTheme(); render(); } });

function setMode(mode) {
  $('shell').dataset.mode = mode;
  // Window dibesarkan sebelum animasi tumbuh, dan dikecilkan setelah animasi selesai.
  box.classList.remove('compact', 'expanded', 'library', 'settings', 'notch');
  box.classList.add(mode);
  syncModules();
}
function setExpanded(on) {
  S.expanded = on; S.lib = S.set = false;
  clearTimeout(hideTimer);
  if (!on) flushNotes();
  if (on) { window.di.setMode('expanded'); setMode('expanded'); return; }
  setMode('compact');
  setTimeout(() => { if (!S.expanded && !box.classList.contains('notch')) window.di.setMode('compact'); }, 320);
  armHide();
}
function armHide() {
  clearTimeout(hideTimer);
  if (!S.auto || S.expanded) return;
  hideTimer = setTimeout(() => {
    setMode('notch');
    setTimeout(() => { if (box.classList.contains('notch')) window.di.setMode('notch'); }, 320);
  }, 1000);
}
// ======================= Waveform (FR-02.1) =======================
// Data asli dari AnalyserNode bila tersedia (stream ber-CORS + opsi diaktifkan); selain itu efek animasi.
let graph = null, wcol = '#ff9f43', wraf = 0, wlast = 0;
const wcv = $('wave'), wx = wcv.getContext ? wcv.getContext('2d') : null;
async function ensureGraph() {
  if (!graph) {
    const ctx = new AudioContext(), an = ctx.createAnalyser();
    an.fftSize = 1024; an.smoothingTimeConstant = .75;
    const src = ctx.createMediaElementSource(cors);
    src.connect(an); an.connect(ctx.destination);
    graph = { ctx, an, buf: new Uint8Array(an.fftSize) };
    // jika konteks audio berhenti saat memutar lewat elemen `cors`, audio jadi senyap: pindah ke elemen biasa
    ctx.onstatechange = () => { if (ctx.state !== 'running' && audio === cors && S.playing) start(); };
  }
  if (graph.ctx.state !== 'running') await Promise.race([graph.ctx.resume(), new Promise((r) => setTimeout(r, 600))]);
  if (graph.ctx.state !== 'running') throw new Error('AudioContext tidak aktif');
}
function drawWave(t) {
  wraf = requestAnimationFrame(drawWave);
  if (t - wlast < 33) return; wlast = t; // ~30 fps
  const dpr = window.devicePixelRatio || 1, W = Math.round(wcv.clientWidth * dpr), H = Math.round(wcv.clientHeight * dpr);
  if (!W || !H) return;
  if (wcv.width !== W || wcv.height !== H) { wcv.width = W; wcv.height = H; }
  wx.clearRect(0, 0, W, H);
  const n = 72, real = !!graph && audio === cors && gotPlaying && S.playing && graph.ctx.state === 'running'; // data asli hanya setelah audio benar-benar berbunyi
  if (real) graph.an.getByteTimeDomainData(graph.buf);
  const g = wx.createLinearGradient(0, 0, W, 0);
  g.addColorStop(0, 'transparent'); g.addColorStop(.18, wcol); g.addColorStop(.82, wcol); g.addColorStop(1, 'transparent');
  wx.strokeStyle = g; wx.lineWidth = 2 * dpr; wx.lineJoin = 'round'; wx.shadowColor = wcol; wx.shadowBlur = 8 * dpr;
  wx.beginPath();
  for (let i = 0; i <= n; i++) {
    const k = i / n; let y;
    if (real) y = (graph.buf[Math.floor(k * (graph.buf.length - 1))] - 128) / 128;
    else { const beat = .55 + .45 * Math.sin(t / 260); y = beat * (.5 * Math.sin(k * 18 + t / 140) + .3 * Math.sin(k * 37 - t / 90) + .2 * Math.sin(k * 7 + t / 330)); }
    const py = H / 2 + y * Math.sin(Math.PI * k) * H * .42; // mengecil ke tepi
    i ? wx.lineTo(k * W, py) : wx.moveTo(k * W, py);
  }
  wx.stroke();
}
function waveOn(on) {
  if (!wx) return;
  if (on && !wraf) wraf = requestAnimationFrame(drawWave);
  else if (!on && wraf) { cancelAnimationFrame(wraf); wraf = 0; wx.clearRect(0, 0, wcv.width, wcv.height); }
}

// ======================= Tumpukan modul (FR-02.4) =======================
const MODS = ['radio', 'sys', 'notes', 'ai'];
let metricsOn = false, lastSwipe = 0;
function setModule(name, focus = true) {
  if (!MODS.includes(name)) return;
  const to = MODS.indexOf(name); S.mod = name;
  document.querySelectorAll('.mod').forEach((m) => { const i = MODS.indexOf(m.dataset.mod); m.classList.toggle('on', i === to); m.classList.toggle('left', i < to); });
  document.querySelectorAll('[data-mt]').forEach((t) => t.classList.toggle('on', t.dataset.mt === name));
  syncModules();
  if (focus) setTimeout(() => { if (name === 'ai') $('aiIn').focus(); else if (name === 'notes' && !$('noteText').disabled && !$('noteText').hidden) $('noteText').focus(); }, 120);
}
const stepModule = (d) => setModule(MODS[(MODS.indexOf(S.mod) + d + MODS.length) % MODS.length]);
const swipeModule = (d) => { const n = Date.now(); if (n - lastSwipe < 350) return; lastSwipe = n; stepModule(d > 0 ? 1 : -1); };
// Pekerjaan berat hanya berjalan saat modulnya benar-benar terlihat (menjaga RAM/CPU idle, NFR-01)
function syncModules() {
  const visible = S.expanded && !S.lib && !S.set && box.classList.contains('expanded');
  const wantMetrics = visible && S.mod === 'sys';
  if (wantMetrics !== metricsOn) { metricsOn = wantMetrics; Promise.resolve(wantMetrics ? window.di.metricsStart() : window.di.metricsStop()).catch(() => {}); }
  waveOn(visible && S.mod === 'radio' && S.playing);
  if (visible && S.mod === 'notes') loadVault();
}

// ======================= Monitor sistem (FR-02.2) =======================
const hist = { cpu: [], ram: [] };
const spark = (a) => a.map((v, i) => `${(i * 120 / 59).toFixed(1)},${(38 - Math.min(100, Math.max(0, v)) / 100 * 36).toFixed(1)}`).join(' ');
window.di.onMetrics((m) => {
  const ramPct = m.ram_total ? m.ram_used / m.ram_total * 100 : 0, gb = (b) => (b / 1073741824).toFixed(1);
  for (const [k, v] of [['cpu', m.cpu], ['ram', ramPct]]) { hist[k].push(v); if (hist[k].length > 60) hist[k].shift(); }
  $('cpuV').textContent = Math.round(m.cpu) + '%'; $('ramV').textContent = Math.round(ramPct) + '%';
  $('ramSub').textContent = `${gb(m.ram_used)} / ${gb(m.ram_total)} GB`;
  $('cpuS').setAttribute('points', spark(hist.cpu)); $('ramS').setAttribute('points', spark(hist.ram));
  if (m.app_mb != null) { // memori aplikasi + proses WebView (diukur tiap 5 detik); target SRS: <= 80 MB saat idle
    const el = $('appMem'), b = document.createElement('b'); b.textContent = `≈ ${m.app_mb} MB`;
    el.className = 'appmem ' + (m.app_mb <= 80 ? 'ok' : 'warn');
    el.replaceChildren('Memori aplikasi ini (termasuk WebView): ', b, ' · target SRS ≤ 80 MB saat idle');
  }
});

// ======================= Catatan & tugas terenkripsi (FR-02.3) =======================
const mk = (tag, cls) => { const e = document.createElement(tag); if (cls) e.className = cls; return e; };
let vault = { notes: '', tasks: [] }, vaultState = 'idle', saveTimer = null; // idle | loading | ok | error
async function loadVault() {
  if (vaultState === 'loading' || vaultState === 'ok') return;
  vaultState = 'loading'; $('noteErr').textContent = ''; $('noteReset').hidden = true;
  try {
    const raw = await window.di.vaultLoad();
    if (raw) {
      let j; try { j = JSON.parse(raw); } catch { throw 'Isi catatan tidak terbaca.'; }
      vault = { notes: String(j.notes || ''),
        tasks: (Array.isArray(j.tasks) ? j.tasks : []).filter((t) => t && typeof t.text === 'string').map((t) => ({ id: t.id, text: t.text, done: !!t.done })) };
    }
    vaultState = 'ok'; $('noteText').value = vault.notes; $('noteText').disabled = false; renderTasks();
  } catch (e) { vaultState = 'error'; $('noteErr').textContent = String(e); $('noteReset').hidden = false; }
}
function queueSave() { if (vaultState !== 'ok') return; $('saveInfo').textContent = 'Menyimpan…'; clearTimeout(saveTimer); saveTimer = setTimeout(doSave, 600); }
async function doSave() {
  clearTimeout(saveTimer); saveTimer = null; if (vaultState !== 'ok') return;
  try { await window.di.vaultSave(JSON.stringify(vault)); $('saveInfo').textContent = 'Tersimpan 🔒'; $('noteErr').textContent = ''; }
  catch (e) { $('saveInfo').textContent = ''; $('noteErr').textContent = 'Gagal menyimpan: ' + e; }
}
const flushNotes = () => { if (saveTimer) doSave(); };
function renderTasks() {
  const open = vault.tasks.filter((t) => !t.done).length;
  $('taskCount').textContent = vault.tasks.length ? `(${open})` : '';
  if (!vault.tasks.length) { const e = mk('div', 'empty'); e.textContent = 'Belum ada tugas.'; $('taskList').replaceChildren(e); return; }
  $('taskList').replaceChildren(...vault.tasks.map((t) => {
    const row = mk('div', 'titem' + (t.done ? ' done' : ''));
    const ck = mk('button', 'ck'); ck.textContent = t.done ? '✓' : ''; ck.setAttribute('aria-label', t.done ? 'Tandai belum selesai' : 'Tandai selesai');
    ck.onclick = () => { t.done = !t.done; renderTasks(); queueSave(); };
    const tx = mk('span', 'tx'); tx.textContent = t.text;
    const del = mk('button', 'del'); del.textContent = '✕'; del.setAttribute('aria-label', 'Hapus tugas');
    del.onclick = () => { vault.tasks = vault.tasks.filter((x) => x !== t); renderTasks(); queueSave(); };
    row.append(ck, tx, del); return row;
  }));
}
function addTask() {
  const v = $('taskIn').value.trim();
  if (!v || vaultState !== 'ok' || vault.tasks.length >= 200) return;
  vault.tasks.push({ id: Date.now() + Math.random(), text: v.slice(0, 200), done: false });
  $('taskIn').value = ''; renderTasks(); queueSave();
}
function setNoteTab(t) {
  $('noteText').hidden = t !== 'notes'; $('tasksBox').hidden = t !== 'tasks';
  document.querySelectorAll('[data-nt]').forEach((b) => b.classList.toggle('on', b.dataset.nt === t));
  setTimeout(() => (t === 'tasks' ? $('taskIn') : $('noteText')).focus(), 60);
}

// ======================= Asisten AI (FR-03) =======================
const AI_DEF = { anthropic: { endpoint: 'https://api.anthropic.com/v1/messages', model: 'claude-haiku-5-5' },
                 openai: { endpoint: 'https://api.openai.com/v1/chat/completions', model: '' } };
let aiKey = false, aiBusy = false, aiPrefix = '';
const aiCfg = () => { const a = S.ai, d = AI_DEF[a.provider] || AI_DEF.anthropic; return { provider: AI_DEF[a.provider] ? a.provider : 'anthropic', endpoint: a.endpoint || d.endpoint, model: a.model || d.model }; };
function renderAi() {
  const c = aiCfg();
  $('aiProv').value = c.provider; $('aiEp').value = S.ai.endpoint || ''; $('aiEp').placeholder = AI_DEF[c.provider].endpoint;
  $('aiModel').value = S.ai.model || ''; $('aiModel').placeholder = AI_DEF[c.provider].model || 'mis. gpt-4o-mini atau llama3.2';
  $('aiState').textContent = aiKey ? 'API key tersimpan di Credential Manager ✓' : 'Belum ada API key' + (c.provider === 'openai' ? ' (boleh kosong untuk server lokal)' : '');
  $('aiInfo').textContent = aiKey || c.provider === 'openai' ? c.model : 'Belum ada API key → ⚙';
}
async function refreshAiState() { try { aiKey = !!(await window.di.aiKeyStatus()); } catch { aiKey = false; } renderAi(); }
async function sendAi() {
  if (aiBusy) return;
  const text = $('aiIn').value.trim(); if (!text) return;
  const c = aiCfg(), out = $('aiOut');
  aiBusy = true; $('aiSend').disabled = true; out.className = 'aiout busy'; out.textContent = 'Memproses…'; $('aiCopy').hidden = true;
  try {
    const r = await window.di.aiAsk(c.provider, c.endpoint, c.model, aiPrefix ? `${aiPrefix}\n\n${text}` : text);
    out.className = 'aiout'; out.textContent = r; $('aiCopy').hidden = false; $('aiIn').value = '';
  } catch (e) { out.className = 'aiout err'; out.textContent = String(e); }
  finally { aiBusy = false; $('aiSend').disabled = false; }
}

function setVol(v) {
  S.vol = Math.min(1, Math.max(0, v)); S.muted = false;
  applyVol(); save(); render();
}

document.querySelectorAll('[data-mt]').forEach((t) => { t.onclick = () => setModule(t.dataset.mt); });
document.querySelectorAll('[data-nt]').forEach((b) => { b.onclick = () => setNoteTab(b.dataset.nt); });
$('noteText').oninput = () => { vault.notes = $('noteText').value; queueSave(); };
$('taskAdd').onclick = addTask;
$('noteReset').onclick = async () => { try { await window.di.vaultReset(); } catch {} vaultState = 'idle'; vault = { notes: '', tasks: [] }; $('noteText').value = ''; loadVault(); };
$('aiSend').onclick = sendAi;
document.querySelectorAll('[data-aq]').forEach((b) => { b.onclick = () => {
  aiPrefix = aiPrefix === b.dataset.aq ? '' : b.dataset.aq;
  document.querySelectorAll('[data-aq]').forEach((x) => x.classList.toggle('on', x.dataset.aq === aiPrefix)); $('aiIn').focus();
}; });
$('aiCopy').onclick = async () => {
  try { await navigator.clipboard.writeText($('aiOut').textContent); $('aiCopy').textContent = 'Disalin ✓'; } catch { $('aiCopy').textContent = 'Gagal menyalin'; }
  setTimeout(() => { $('aiCopy').textContent = 'Salin'; }, 1500);
};
$('aiProv').onchange = () => { S.ai = { provider: $('aiProv').value, endpoint: '', model: '' }; save(); renderAi(); };
$('aiEp').onchange = () => { S.ai.endpoint = $('aiEp').value.trim(); save(); renderAi(); };
$('aiModel').onchange = () => { S.ai.model = $('aiModel').value.trim(); save(); renderAi(); };
$('aiKeySave').onclick = async () => {
  try { await window.di.aiKeySet($('aiKey').value); $('aiKey').value = ''; $('setErr').textContent = ''; aiKey = true; }
  catch (e) { $('setErr').textContent = String(e); }
  renderAi();
};
$('aiKeyDel').onclick = async () => {
  try { await window.di.aiKeyClear(); aiKey = false; $('setErr').textContent = ''; } catch (e) { $('setErr').textContent = String(e); }
  renderAi();
};
$('diagShow').onclick = async () => {
  const out = $('diagOut'); $('diagShow').disabled = true; out.hidden = false; out.textContent = 'Memuat…';
  try { out.textContent = await window.di.diagText(); $('diagCopy').hidden = false; }
  catch (e) { out.textContent = 'Gagal membaca diagnostik: ' + e; }
  $('diagShow').disabled = false; $('diagShow').textContent = 'Muat ulang';
};
$('diagCopy').onclick = async () => {
  try { await navigator.clipboard.writeText($('diagOut').textContent); $('diagCopy').textContent = 'Disalin ✓'; } catch { $('diagCopy').textContent = 'Gagal menyalin'; }
  setTimeout(() => { $('diagCopy').textContent = 'Salin'; }, 1500);
};
$('realWave').onclick = () => { S.realWave = !S.realWave; save(); if (!S.realWave && audio === cors && S.playing) start(); render(); };
$('tabs').addEventListener('wheel', (e) => { e.preventDefault(); e.stopPropagation(); swipeModule(e.deltaY || e.deltaX); }, { passive: false });
$('mods').addEventListener('wheel', (e) => {
  if (Math.abs(e.deltaX) > 20 && Math.abs(e.deltaX) > Math.abs(e.deltaY) * 1.5) { e.preventDefault(); e.stopPropagation(); swipeModule(e.deltaX); } // geser horizontal = ganti modul
}, { passive: false });
window.di.onHotkey((a) => { // FR-04.2: Ctrl+Shift+Space buka/tutup; Ctrl+Shift+A langsung ke asisten AI
  if (a === 'panel') setExpanded(!S.expanded);
  else if (a === 'ai') { setExpanded(true); setModule('ai'); }
});

$('mini').onclick = () => setExpanded(true);
$('miniPlay').onclick = (e) => { e.stopPropagation(); togglePlay(); };
$('miniOpen').onclick = (e) => { e.stopPropagation(); setExpanded(true); };
document.querySelectorAll('.collapse, #close').forEach((b) => { b.onclick = () => setExpanded(false); });
$('play').onclick = togglePlay;
$('prev').onclick = () => step(-1);
$('next').onclick = () => step(1);
$('vol').oninput = (e) => setVol(+e.target.value);
$('mute').onclick = () => { S.muted = !S.muted; applyVol(); render(); };
$('auto').onclick = () => { S.auto = !S.auto; save(); clearTimeout(hideTimer); if (S.auto) armHide(); render(); };
$('startup').onclick = async () => {
  try { S.startup = await window.di.autostartSet(!S.startup); $('setErr').textContent = ''; }
  catch (e) { $('setErr').textContent = typeof e === 'string' ? e : 'Gagal mengubah pengaturan startup.'; console.log('[startup]', e); }
  render();
};
$('browse').onclick = openLib;
$('toggleAdd').onclick = () => setAdding(!adding);
$('cancelEdit').onclick = () => setAdding(false);
$('reStream').onclick = () => {
  const r = S.list.find((x) => x.id === editId); if (!r) return;
  r.manual = false; r.url = ''; delete S.dead[r.id]; save(); setAdding(false); play(r.id); renderList();
};
$('delIcon').onclick = () => {
  const r = S.list.find((x) => x.id === editId); if (!r) return;
  r.icon = null; r.iconOff = true; r.iconCustom = false; save(); refreshLogos(r); $('err').textContent = 'Ikon dihapus (tampil sebagai huruf awal).';
};
$('zoom').oninput = (e) => { S.zoom = +e.target.value; box.style.setProperty('--zoom', S.zoom); $('zoomVal').textContent = Math.round(S.zoom * 100) + '%'; };
$('zoom').onchange = save;
$('q').oninput = renderList;
$('openSet').onclick = openSet;
$('reloadIcons').onclick = async () => {
  const b = $('reloadIcons'); if (b.disabled) return;
  b.disabled = true; $('iconInfo').textContent = 'Memuat ikon…';
  const r = await fillIcons(true); b.disabled = false;
  $('iconInfo').textContent = !r.total ? 'Semua stasiun sudah punya ikon.'
    : `${r.ok} dari ${r.total} ikon dimuat.` + (r.fail.length ? ` Gagal: ${r.fail.slice(0, 2).join('; ')}${r.fail.length > 2 ? '…' : ''}` : '');
};
$('backSet').onclick = closeSet;
$('back').onclick = closeLib;
$('add').onclick = () => (editId != null ? saveEdit() : addStation());
$('quit').onclick = () => window.di.quit();

// Mouse: tembus-klik di luar island, auto-hide, scroll untuk volume
box.addEventListener('mouseenter', () => {
  clearTimeout(hideTimer);
  if (box.classList.contains('notch')) { window.di.setMode('compact'); setMode('compact'); }
});
box.addEventListener('mouseleave', armHide);
$('strip').addEventListener('wheel', (e) => { e.preventDefault(); e.stopPropagation(); $('strip').scrollLeft += e.deltaY || e.deltaX; }, { passive: false });
box.addEventListener('wheel', (e) => { if (S.mod === 'radio' && !S.lib && !S.set) setVol(S.vol - Math.sign(e.deltaY) * .05); }, { passive: true });

// Keyboard (saat widget fokus) + tombol media dan pintasan global dari backend
window.addEventListener('keydown', (e) => {
  const open = S.expanded && !S.lib && !S.set;
  if (e.altKey && /^[1-4]$/.test(e.key) && open) { e.preventDefault(); setModule(MODS[+e.key - 1]); return; }
  const t = e.target, typing = (t.tagName === 'INPUT' && !['range', 'checkbox'].includes(t.type)) || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT';
  if (typing) { // sedang mengetik: tombol pintasan biasa tidak boleh aktif
    if (e.key === 'Escape') { if (S.lib) closeLib(); else if (S.set) closeSet(); else setExpanded(false); }
    else if (e.key === 'Enter') {
      if (t.id === 'taskIn') { e.preventDefault(); addTask(); }
      else if (t.id === 'aiIn' && !e.shiftKey) { e.preventDefault(); sendAi(); }
      else if (['nm', 'ur', 'ic'].includes(t.id)) (editId != null ? saveEdit() : addStation());
    }
    return;
  }
  if (e.ctrlKey && (e.key === 'ArrowRight' || e.key === 'ArrowLeft') && open) { e.preventDefault(); stepModule(e.key === 'ArrowRight' ? 1 : -1); return; }
  const radio = S.mod === 'radio';
  if (e.code === 'Space' && radio) { e.preventDefault(); togglePlay(); }
  else if (e.key === 'ArrowRight' && radio) step(1);
  else if (e.key === 'ArrowLeft' && radio) step(-1);
  else if (e.key === 'Escape') { if (S.lib) closeLib(); else if (S.set) closeSet(); else setExpanded(false); }
  else if (e.key.toLowerCase() === 'm' && radio) $('mute').click();
});
window.di.onMedia((a) => ({ toggle: togglePlay, next: () => step(1), prev: () => step(-1) })[a]?.());
window.di.onCursor((inside) => { if (inside && box.classList.contains('notch')) { window.di.setMode('compact'); setMode('compact'); } });
window.di.onBlur(() => { if (S.expanded && !picking) setExpanded(false); });
window.addEventListener('focus', () => setTimeout(() => { picking = false; }, 800));
window.di.onSecond(() => setExpanded(true)); // aplikasi dibuka lagi: tampilkan panel yang sudah berjalan

applyTheme(); setModule('radio', false); renderAi(); refreshAiState();
render(); armHide(); setTimeout(() => fillIcons(), 1500);
window.di.autostartGet().then((v) => { S.startup = !!v; render(); }).catch(() => {});
})();
