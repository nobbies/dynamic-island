# Dynamic Island Radio — versi Tauri 2

Widget radio bergaya Dynamic Island yang menempel di tepi atas layar. Versi ini sama dengan versi Electron
(tampilan, daftar stasiun manual, pemeriksa stream otomatis), tetapi dibangun dengan Tauri sehingga hasil
build jauh lebih kecil dan ringan.

## Struktur proyek

```
dynamic-island-tauri/
├─ package.json              CLI Tauri (@tauri-apps/cli)
├─ data/daftar_radio_bali.js  sumber daftar radio bawaan (edit di sini)
├─ scripts/build-stations.mjs pembuat src/stations.default.js (+ unduh ikon)
├─ assets/icon.svg           SUMBER IKON (SVG lengkap) + icon-small.svg (varian ukuran kecil)
├─ scripts/make-icons.mjs    pembuat semua ikon dari SVG (npm run icons)
├─ SRS-STATUS.md             status pemenuhan SRS (tabel persyaratan, bukti, dan catatan)
├─ app-icon.png              ikon sumber (ganti dengan ikon Anda, lalu jalankan `npm run tauri icon app-icon.png`)
├─ src/                      FRONTEND (HTML/CSS/JS biasa, tanpa bundler)
│  ├─ index.html
│  ├─ style.css
│  ├─ stations.default.js    DAFTAR RADIO BAWAAN (dibuat oleh skrip di atas)
│  └─ app.js                 logika widget + jembatan ke Rust (window.__TAURI__)
└─ src-tauri/                BACKEND (Rust)
   ├─ tauri.conf.json        pengaturan jendela (transparan, selalu di atas, tanpa bingkai) + CSP
   ├─ capabilities/default.json
   ├─ Cargo.toml
   ├─ icons/                 dihasilkan oleh `tauri icon`
   └─ src/
      ├─ main.rs             jendela, pembaca kursor (notch), pintasan global, daftar perintah
      ├─ detect.rs           pemeriksa stream + pencari ikon
      ├─ applog.rs           log teks (diagnosis) di folder log aplikasi
      ├─ metrics.rs          pemantau CPU/RAM (hanya hidup saat modul Monitor dibuka)
      ├─ vault.rs            brankas catatan AES-256-GCM
      ├─ secrets.rs          Windows Credential Manager (API key, kunci enkripsi)
      └─ ai.rs               klien asisten AI (Claude / kompatibel OpenAI)
```

---

## A. Menjalankan proyek ini (jalan pintas)

### 1. Pasang prasyarat (sekali saja, Windows)

1. **Microsoft C++ Build Tools** — unduh "Build Tools for Visual Studio", centang beban kerja
   **"Desktop development with C++"**.
2. **WebView2** — sudah ada di Windows 11. Di Windows 10, pasang "Evergreen Bootstrapper" dari Microsoft.
3. **Rust** — unduh `rustup-init.exe` dari rustup.rs, pilih instalasi default.
   Tauri 2 versi terbaru mensyaratkan **Rust 1.90 atau lebih baru**. Cek dengan `rustc --version`;
   jika lebih lama, jalankan `rustup update`.
4. **Node.js** (LTS) dari nodejs.org.
5. **Tutup lalu buka ulang terminal** agar `cargo` dan `node` terbaca.

Cek: `rustc --version`, `cargo --version`, `node --version`.

### 2. Jalankan

```powershell
cd dynamic-island-tauri
npm install
npm run tauri dev
```

Build pertama memakan waktu lama (sekitar 5–20 menit) karena Rust mengompilasi semua dependensi.
Build berikutnya hanya beberapa detik. Widget muncul di tengah-atas layar.

### 3. Buat installer

```powershell
npm run tauri build
```

Hasilnya ada di `src-tauri/target/release/bundle/` (installer `.msi` dan `.exe`). File `.exe` aplikasi
saja ada di `src-tauri/target/release/`.

---

## B. Membuat project dari nol (cara membangunnya sendiri)

Gunakan ini kalau ingin memahami tiap bagian atau memulai proyek baru.

1. **Siapkan prasyarat** — sama seperti A.1.
2. **Buat folder dan pasang CLI**
   ```powershell
   mkdir dynamic-island-tauri; cd dynamic-island-tauri
   npm init -y
   npm install -D @tauri-apps/cli@latest
   ```
3. **Inisialisasi Tauri**
   ```powershell
   npx tauri init
   ```
   Jawab pertanyaannya: nama aplikasi `Dynamic Island Radio`, judul jendela bebas,
   **frontend dist = `../src`**, dev URL dikosongkan, perintah dev/build dikosongkan.
   Perintah ini membuat folder `src-tauri/`.
4. **Buat frontend** — buat folder `src/` berisi `index.html`, `style.css`, `app.js`.
   Aturan penting:
   - JANGAN memakai atribut `style="..."` atau `<script>`/`<style>` inline di HTML. CSP Tauri memblokirnya;
     pakai file `.css` dan `.js` terpisah.
   - Backend dipanggil lewat `window.__TAURI__.core.invoke('nama_perintah', { argumen })`, dan event dari
     Rust didengar lewat `window.__TAURI__.event.listen('nama_event', callback)`.
     Ini aktif karena `"withGlobalTauri": true` di `tauri.conf.json`.
5. **Atur jendela** di `src-tauri/tauri.conf.json`: `decorations: false`, `transparent: true`,
   `alwaysOnTop: true`, `skipTaskbar: true`, `resizable: false`, `shadow: false`, `visible: false`
   (ditampilkan oleh Rust setelah diposisikan), dan `"macOSPrivateApi": true` bila ingin berjalan di macOS.
6. **Tambah dependensi Rust** di `src-tauri/Cargo.toml`: `tauri-plugin-global-shortcut`, `serde`,
   `reqwest` (fitur `rustls-tls`), `regex`, `futures`.
7. **Tulis backend** di `src-tauri/src/`:
   - `main.rs` — perintah `set_mode` (mengubah ukuran dan menempatkan jendela di tengah-atas monitor),
     `quit_app`, `probe`, `detect`; pembaca posisi kursor (untuk hover mode notch); tombol media global.
   - `detect.rs` — pemeriksa stream.
8. **Buat ikon** — siapkan gambar PNG persegi (minimal 512×512), lalu:
   ```powershell
   npx tauri icon app-icon.png
   ```
   Build gagal di Windows bila folder `src-tauri/icons/` (terutama `icon.ico`) tidak ada.
9. **Jalankan** `npm run tauri dev`, lalu **kemas** dengan `npm run tauri build`.

---

## Ikon aplikasi

Ikon dibuat dari `assets/icon.svg`. Untuk mengganti atau memperbarui ikon: ganti isi `assets/icon.svg`
(ikon kecil 16–32 px memakai varian sederhana `assets/icon-small.svg`), lalu jalankan:

```powershell
npm install
npm run icons
```

Skrip membuat seluruh isi `src-tauri/icons/`, termasuk `icon.ico` bertingkat (16, 24, 32, 48, 64, 128, 256 px: tiap ukuran
memakai gambar yang paling terbaca). Tulisan pada SVG dirender dengan font komputer Anda (Inter bila terpasang, selain itu Arial).

**Mengapa ikon `setup.exe` sebelumnya tidak berubah:** Tauri tidak otomatis memakai `icon.ico` untuk file installer NSIS;
ikon installer harus ditunjuk eksplisit lewat `bundle.windows.nsis.installerIcon`. Sekarang sudah diatur di `tauri.conf.json`.
Versi dinaikkan ke 0.1.1 agar nama file installer baru (`..._0.1.1_x64-setup.exe`) tidak terkena cache ikon nama lama.

**Jika ikon lama masih tampil setelah build ulang** (Windows menyimpan cache ikon):

1. Pastikan Anda membuka installer **baru** di `src-tauri\target\release\bundle\nsis\`, bukan salinan lama.
2. Jika ikon file `.exe` aplikasi masih lama, paksa penyusunan ulang sumber daya: hapus folder `src-tauri\target\release\bundle`,
   lalu jalankan `npm run tauri build` lagi. Pilihan terakhir: `cargo clean --release --manifest-path src-tauri\Cargo.toml`.
3. Segarkan cache ikon Windows: `ie4uinit.exe -show`, atau hentikan Explorer
   (`taskkill /f /im explorer.exe`), hapus `%LOCALAPPDATA%\IconCache.db` dan `%LOCALAPPDATA%\Microsoft\Windows\Explorer\iconcache*`,
   lalu jalankan `explorer.exe`.
4. Pintasan yang sudah disematkan di taskbar atau Start tetap memakai ikon lama: lepas sematan, lalu sematkan ulang dari aplikasi yang baru terpasang.

## Daftar radio bawaan installer

Sumbernya satu berkas: **`data/daftar_radio_bali.js`**. Cukup edit berkas itu; tidak ada langkah lain.

```js
{ name: "Menara FM", frequency: "102.8 FM", location: "Denpasar",
  streamUrl: "https://c1.siar.us:10280/menarafmbali", icon: "https://.../logo.png" }
```

- **Dibuat ulang otomatis** setiap `npm run tauri dev` dan `npm run tauri build` (lewat skrip `pretauri` di `package.json` dan
  `beforeBuildCommand` di `tauri.conf.json`). Skrip `scripts/build-stations.mjs` menulis `src/stations.default.js` dan mencetak
  ringkasan, mis. `stasiun bawaan: 31 (30 alamat stream langsung, 1 halaman web yang dicari otomatis)`.
  Jangan mengedit `src/stations.default.js` secara manual.
- **`streamUrl` boleh berupa alamat stream langsung (disarankan) atau alamat halaman web.** Alamat seperti
  `https://situs.com/` (tanpa jalur) atau `radiostreaming.id/...html` dianggap halaman web: stream-nya dicari aplikasi saat stasiun
  diputar. Jika alamat yang disimpan ternyata halaman web dan gagal diputar, aplikasi juga memeriksanya dan mencari stream
  di dalamnya secara otomatis. Alamat proxy (mis. `https://a.tunzilla.com/http://...`) dipakai apa adanya.
- **ID stasiun dibuat dari namanya**, jadi tidak berubah walau urutan atau alamat di data berubah. Karena itu **jangan mengganti
  nama** stasiun di data jika ingin mempertahankan stasiun yang sudah dihapus atau diubah pengguna.
- **Pengguna lama diperbarui otomatis saat aplikasi dibuka.** Aturannya:
  - stasiun bawaan yang sudah tersimpan mengikuti data terbaru (alamat stream, nama, ikon);
  - yang **sudah diubah pengguna** tidak ditimpa: alamat yang diisi manual, nama baru, atau ikon sendiri;
  - stasiun bawaan yang sudah tidak ada di data dibuang (kecuali sudah diubah pengguna, lalu menjadi stasiun biasa);
  - stasiun bawaan baru ditambahkan sekali; yang dihapus pengguna tidak muncul lagi;
  - stasiun buatan pengguna tidak pernah disentuh.
- **Ikon** diunduh bertahap saat aplikasi pertama dijalankan (perlu internet). Agar ikut masuk installer dan tampil walau offline:

  ```powershell
  npm run stations -- --icons
  npm run tauri build
  ```

  Ikon yang sudah diunduh tetap dipakai pada build berikutnya (tidak hilang saat `npm run tauri build` biasa).
- **Mulai dari nol (membuang semua stasiun tersimpan):** tutup aplikasi, lalu hapus folder data WebView-nya, biasanya
  `%LOCALAPPDATA%\com.example.dynamicislandradio`. Mode dev dan aplikasi hasil build memakai folder yang sama,
  jadi daftar yang tersimpan terbawa di antara keduanya.

## Tema dan bentuk

- **Bentuk:** sisi atas menempel ke tepi layar dengan **sudut atas yang melebar ("telinga" cekung)** seperti takik layar, dan sudut bawah yang
  lebih melengkung. Jendela 28 px lebih lebar dari pulau (14 px per telinga). Pada mode garis tipis (auto-hide) telinga disembunyikan.
  Bingkai neon dan kilau tepi dipotong/dipudarkan di zona telinga supaya tidak ada garis lurus di sampingnya, dan gradien neon dibuat
  horizontal agar warna ujung lengkung selalu sama dengan warna sisi pulau di semua ukuran panel.
- **Tema:** ⚙ Pengaturan → **Tema**. Pilihan: **Otomatis** (mengikuti mode gelap/terang Windows), **Holografik** (bawaan), **Midnight** (hitam polos),
  **Terang**, **Samudra**, **Senja**. Pilihan diingat. Warna tiap tema hanya ada di `src/style.css` (bagian `:root[data-theme=...]`),
  jadi menambah tema baru cukup menambah satu baris variabel dan satu kotak pilihan di `index.html`.

## Mengatasi: Microsoft Edge terbuka sendiri saat Windows menyala

**Hasil audit aplikasi ini:** tidak ada satu pun jalur yang bisa membuka browser. Tidak ada tautan, `window.open`, pemanggilan proses,
maupun plugin `shell`/`opener` di kode, konfigurasi, dan pohon dependensi. Sebagai pengaman tambahan, halaman aplikasi sekarang **dilarang
berpindah ke alamat luar dan dilarang membuka jendela baru**; setiap upaya dicatat di log.
Karena itu penyebab yang paling mungkin adalah fitur startup Edge sendiri ("Startup boost" atau "Continue running background...").
Cara memastikan, urut:

1. **Uji pembeda (paling menentukan):** matikan ⚙ Pengaturan → *Mulai saat komputer menyala*, lalu restart. Jika Edge **tetap** terbuka, penyebabnya bukan
   aplikasi ini. Jika Edge berhenti terbuka, nyalakan lagi dan lanjut ke langkah 2.
2. **Lihat entri startup:** ⚙ Pengaturan → **Diagnostik → Tampilkan**. Bagian *entri startup Windows (Run)* menampilkan entri aplikasi ini dan entri Edge
   (nama seperti `MicrosoftEdgeAutoLaunch_...`). Jika entri Edge ada di sana, Edge memang memulai dirinya sendiri. Jika entri aplikasi ini menunjuk ke
   `target\debug`, itu sisa dari mode dev: matikan lalu nyalakan lagi sakelar startup dari aplikasi hasil build.
3. **Matikan startup Edge:** buka `edge://settings/system` lalu matikan **Startup boost** dan **Continue running background extensions and apps when
   Microsoft Edge is closed**; atur `edge://settings/onStartup` ke *Open the new tab page*. Alternatifnya Windows Settings → Apps → Startup → Microsoft Edge → Off.
4. **Jika Edge masih terbuka dan baris log aplikasi memuat "navigasi ke luar DITOLAK" atau "jendela baru DITOLAK":** berarti halaman aplikasi yang mencoba membukanya.
   Tombol **Salin** di Diagnostik menyalin seluruh teksnya untuk dikirim.

Aplikasi sekarang juga menandai peluncuran otomatis dengan argumen `--autostart` (tercatat di log sebagai "otomatis-saat-login").

## Modul (Smart Widget Stack)

Klik widget untuk membuka panel, lalu pilih modul lewat **tab di bagian atas panel (menempel ke tepi layar)**: 📻 Radio · 📊 Sistem · 📝 Catatan · ✨ AI,
di kanan ada ⚙ Pengaturan · ⏻ Keluar · ▲ Tutup panel. Berpindah modul juga bisa dengan **`Alt+1–4`**, **`Ctrl+←/→`**, roda mouse di atas tab bar,
atau geser horizontal (touchpad) di area modul.

- **📻 Radio:** pemutar, strip ikon stasiun, volume, dan waveform di belakang tombol kontrol. Roda mouse di panel ini mengubah volume.
- **📊 Sistem:** CPU dan RAM tiap detik dengan grafik 60 detik terakhir. Di bawahnya tertulis **memori aplikasi ini (termasuk proses
  WebView)** yang dibandingkan dengan target SRS 80 MB (hijau bila ≤ 80 MB, kuning bila lebih). Pengambil sampel hanya berjalan
  selama modul ini terlihat.
- **📝 Catatan:** teks bebas dan daftar tugas (centang, hapus). **Tersimpan otomatis dan terenkripsi** (AES-256-GCM; kunci acak di Windows
  Credential Manager, berkas `notes.vault` di folder data aplikasi). Jika berkas tidak bisa dibuka (mis. kunci hilang), muncul
  tombol **Mulai catatan baru**; isi lama tidak bisa dipulihkan tanpa kuncinya.
- **✨ AI:** tanya apa saja atau tempel teks lalu pilih **Ringkas / Jelaskan / Terjemahkan**. `Enter` mengirim, `Shift+Enter` baris baru.
  Satu pertanyaan per permintaan (tanpa riwayat).

### Mengatur asisten AI (sekali saja)
Buka **⚙ Pengaturan → Asisten AI**:
1. Pilih **Penyedia**: *Claude (Anthropic)* atau *Kompatibel OpenAI* (OpenAI, OpenRouter, Groq, serta server lokal seperti Ollama/LM Studio).
2. Endpoint dan model sudah terisi untuk Claude (`claude-haiku-5-5`, dapat diganti). Untuk "Kompatibel OpenAI", isi **nama model**
   (mis. `gpt-4o-mini`, `llama3.2`) dan ubah endpoint bila perlu (server lokal boleh `http://localhost:11434/v1/chat/completions`).
3. Tempel **API key** lalu **Simpan**. Kunci disimpan di Windows Credential Manager, dikirim langsung ke Rust, dan **tidak bisa dibaca kembali**
   oleh halaman web (hanya status "ada/tidak"). Server lokal boleh tanpa kunci.
4. Permintaan dikirim dari Rust. Endpoint wajib `https://` (kecuali `localhost`).

### Pintasan keyboard
| Pintasan | Fungsi |
|---|---|
| `Ctrl+Shift+Space` | Buka / tutup panel (global, walau widget tidak fokus) |
| `Ctrl+Shift+A` | Buka langsung asisten AI dan fokus ke kolom pertanyaan |
| `Ctrl+Alt+R`, tombol media | Putar / jeda |
| `Alt+1–4`, `Ctrl+←/→` | Ganti modul |
| `←` `→`, `Space`, `M`, `Esc` | Stasiun sebelumnya/berikutnya, putar/jeda, bisukan, tutup (di modul Radio / saat tidak mengetik) |

### Waveform
Secara bawaan waveform adalah **efek animasi** yang mengikuti status pemutaran. Browser hanya mengizinkan membaca audio asli dari
stream yang mengirim header CORS, jadi data asli bersifat opsional: nyalakan **⚙ Pengaturan → Waveform dari audio asli**. Aplikasi lalu mencoba
membaca audio asli; jika server tidak mendukung, alamat itu dicatat dan diputar seperti biasa (waveform tetap tampil sebagai animasi).
Audio tidak pernah dialihkan ke jalur yang berisiko senyap: bila konteks audio tidak aktif, aplikasi otomatis memakai jalur biasa.

### Status SRS
Lihat **`SRS-STATUS.md`** untuk tabel pemenuhan tiap persyaratan, apa yang sudah diuji, dan yang belum bisa dibuktikan (terutama NFR-01).

## Cara memakai widget

- Klik widget untuk membuka panel. **🔍 Stasiun** membuka daftar (ada kolom pencarian nama/frekuensi/kota). Tekan **＋ Tambah** untuk menambah stasiun sendiri: tempel alamat stream, lalu **Tambah dan putar**.
- Jika yang ditempel adalah halaman web (bukan stream), widget mencari alamat audionya sendiri.
- Pintasan: `Space` putar/jeda, `←` `→` ganti stasiun, `M` bisukan, `Esc` tutup.
  Tombol media keyboard dan `Ctrl+Alt+R` bekerja walau widget tidak fokus.
- **⚙ Pengaturan** berisi semua pengaturan: **Mulai saat komputer menyala**, **Auto-hide**, dan **Timer tidur**
  (Mati / 15 / 30 / 60 menit). Status startup dibaca langsung dari sistem, jadi sakelar selalu sesuai keadaan sebenarnya.
  Saat timer tidur aktif, status di bawah nama stasiun menampilkannya.
- **Strip stasiun:** di panel utama, semua stasiun tampil sebagai deretan ikon yang bisa digulir (roda mouse atau
  touchpad). Stasiun yang sedang diputar diberi cincin warna dan otomatis ditengahkan, jadi memilih stasiun urutan
  berapa pun selalu terlihat. Stasiun yang gagal diputar diredupkan.
- **Edit stasiun (✎ di daftar):** memperbaiki stasiun yang bermasalah tanpa menghapusnya.
  - **Nama:** ganti dan simpan.
  - **Alamat stream:** isi alamat audio yang benar, atau alamat halaman web (widget mencari streamnya sendiri).
    Alamat yang Anda isi dipakai apa adanya dan tidak akan ditimpa pencarian otomatis. Setelah disimpan, stasiun
    langsung diputar untuk menguji. Untuk stasiun bawaan, mengosongkan alamat (atau tombol **🔄 Cari ulang stream**)
    mengembalikannya ke pencarian otomatis dari halaman sumber.
  - **Ikon:** isi URL gambar atau pilih file (📁); **🗑 Hapus ikon** mengembalikan ke huruf awal (tidak diunduh ulang otomatis).
    Kosongkan kolom ikon jika tidak ingin mengubahnya.
- **Ikon stasiun:** saat menambah stasiun, widget mencari ikonnya sendiri (logo di halaman web, atau header
  `icy-logo` dari server stream). Stasiun yang namanya cocok dengan daftar bawaan (mis. "Menara FM Bali" ~ "Menara FM")
  memakai ikon daftar bawaan. Saat diimpor, **margin putih/transparan di tepi gambar dipangkas otomatis** sehingga logo
  memenuhi kotak. Untuk ikon yang sudah tersimpan atau masih punya bingkai putih, atur **⚙ Pengaturan → Ikon stasiun →
  Ukuran** (100%–180%, bawaan 120%): ikon diperbesar dan sisi yang berlebih terpotong oleh kotak.
  Jika ikon tidak muncul, tekan **Muat ulang**: hasilnya ditulis di bawahnya beserta alasan kegagalan
  (mis. `HTTP 403`, `server tidak menyebut logo`). Gambar disimpan lokal (96×96), jadi tetap tampil walau offline.
- **Roda mouse:** di panel utama mengubah volume. Di strip ikon menggulir stasiun, dan di daftar stasiun serta menu
  Pengaturan hanya menggulir isi (volume tidak berubah).
- **Satu instance:** jika aplikasi sudah berjalan lalu dibuka lagi, salinan kedua langsung berhenti dan panel
  salinan pertama ditampilkan. Suara tidak akan dobel.
- **Keluar:** tombol ⏻ di tab bar. Tombol **▲** di ujung kanan tab bar (atau `Esc`) menutup panel. Auto-hide mengecilkan widget jadi garis tipis; arahkan kursor ke tengah-atas
  layar untuk memunculkannya lagi.

## Perbedaan dari versi Electron

| | Electron | Tauri |
|---|---|---|
| Hover pada notch | `setIgnoreMouseEvents(forward)` | Rust membaca posisi kursor tiap 80 ms |
| Pemeriksa stream | `detect.js` (Node) | `detect.rs` (Rust, hasil uji sama) |
| Monitor | mengikuti kursor | tetap di monitor tempat jendela berada |
| Data tersimpan | profil Electron | profil WebView2 (daftar stasiun perlu ditambah ulang) |

## Catatan dan keterbatasan

- **Bug startup (jendela terminal + kotak abu-abu saat login):** terjadi bila startup dinyalakan dari `npm run tauri dev`.
  Windows lalu menjalankan file `.exe` mode debug (`src-tauri\target\debug\...`) saat login. Build debug membuka
  jendela konsol dan mencoba memuat server dev yang tidak berjalan. Sekarang sakelar startup **menolak menyala di mode dev**
  (pesannya tampil di Pengaturan), dan aplikasi hasil build menulis ulang entri startup yang salah secara otomatis.
  **Jika entri lama masih ada di komputer Anda:** buka Task Manager (`Ctrl+Shift+Esc`) > tab *Startup apps*, matikan
  entri yang menunjuk ke `target\debug`, atau jalankan aplikasi hasil build sekali (entri lama akan diperbaiki otomatis).
  Mengecek isi startup: `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

- **Satu instance dan mode dev:** karena hanya satu salinan yang boleh berjalan, `npm run tauri dev` akan langsung
  tertutup bila versi terpasang sedang berjalan (dan sebaliknya). Tutup salah satunya lebih dulu.
- **Ikon otomatis tidak selalu ditemukan.** Banyak server stream tidak menyebut logonya. Jika tidak ketemu, widget memakai
  huruf awal; ganti lewat tombol 🖼. Pencarian ikon hanya mengambil gambar ≤ 400 KB.
- **Dialog pilih file** membuat jendela kehilangan fokus. Widget sudah diatur agar tidak menciut selama dialog terbuka;
  jika dialog muncul di belakang jendela widget di komputer Anda, kabari saya.

- **Mulai saat komputer menyala: aktifkan dari aplikasi hasil build** (`npm run tauri build`, lalu pasang installer-nya), bukan dari
  `npm run tauri dev`. Mode dev mendaftarkan file `.exe` sementara di folder `target/debug`, yang tidak cocok
  dipakai saat startup. Windows menyimpannya di registri milik pengguna (`HKCU\...\Run`), tanpa hak admin.
  Jika aplikasi dipindah atau dipasang ulang ke folder lain, matikan lalu nyalakan lagi sakelar tersebut.
  Windows juga dapat menonaktifkannya lewat Task Manager > tab Startup.

- Kode Rust sudah dikompilasi dan diuji di Linux (`cargo check`, dan uji `probe`/`detect` terhadap server
  lokal). **Belum dijalankan di Windows.** Hal yang paling perlu Anda perhatikan saat pertama kali
  menjalankan: (1) apakah mode notch (tinggi 6 piksel) tampil, dan (2) apakah suara keluar.
- `Cargo.lock` tidak disertakan; akan dibuat otomatis dengan versi terbaru saat build pertama.
- macOS: stream `http://` biasa dapat diblokir App Transport Security. Jika dipakai di macOS, tambahkan
  `NSAllowsArbitraryLoadsForMedia` = true pada `Info.plist` aplikasi (belum diuji).
- Linux: pemutaran AAC/MP3 membutuhkan paket GStreamer plugin (good/bad/libav) terpasang.
- Jika `tauri dev` gagal dengan pesan "requires rustc 1.xx", jalankan `rustup update`.
