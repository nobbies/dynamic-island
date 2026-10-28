# Status Pemenuhan SRS — Dynamic Island Desktop (Tauri Edition) v0.2.0

Legenda: ✅ dipenuhi · ⚠️ dipenuhi sebagian / dengan catatan · ❓ belum bisa dibuktikan.
Kolom **Bukti** menyebut apa yang benar-benar sudah diuji. **Belum ada yang dijalankan di Windows**: pengujian dilakukan di
Linux (kompilasi + uji unit Rust), DOM tiruan (logika antarmuka), dan WebKit dengan data contoh (tampilan).

## Persyaratan fungsional

| ID | Persyaratan | Status | Implementasi | Bukti / catatan |
|---|---|---|---|---|
| FR-01.1 | Frameless + always-on-top | ✅ | `tauri.conf.json` (decorations, alwaysOnTop, transparent, skipTaskbar) | Konfigurasi tervalidasi saat kompilasi |
| FR-01.2 | Transisi morphing mulus | ✅ | CSS spring `cubic-bezier(.34,1.4,.64,1)`, 0,2 dtk | Tampilan diperiksa di WebKit |
| FR-01.3 | Posisi tengah-atas | ✅ | `place()` di `main.rs` (tengah-atas monitor aktif) | Logika dikompilasi; posisi nyata belum dilihat di Windows |
| FR-02.1 | Radio: play/pause/mute + waveform | ⚠️ | Pemutar penuh, mute, volume. Waveform: efek animasi (bawaan) atau data audio asli (opsional) | **Audio asli hanya terbaca bila server stream mengirim header CORS** (aturan browser). Selain itu waveform berupa animasi. Opsi: ⚙ Pengaturan → "Waveform dari audio asli". Jalur asli + fallback diuji dengan 24 pemeriksaan (DOM tiruan) |
| FR-02.2 | Monitor CPU/RAM tiap 1–2 dtk | ✅ | `metrics.rs` (sysinfo), interval 1 dtk, grafik 60 sampel | Uji unit Linux lulus; sampler hanya hidup saat modul dibuka |
| FR-02.3 | Catatan tersimpan otomatis, terenkripsi | ✅ | `vault.rs`: AES-256-GCM, kunci acak di Credential Manager; berkas `notes.vault` di folder data aplikasi; simpan otomatis 600 ms | Uji unit: isi tidak terbaca di disk, kunci salah / data diubah ditolak, penulisan atomik. **Jalur Credential Manager Windows belum diuji** (di Linux dikompilasi dengan keyutils) |
| FR-02.4 | Navigasi modul | ✅ | Tab, `Alt+1–4`, `Ctrl+←/→`, roda mouse di tab bar, geser horizontal | Diuji (DOM tiruan) |
| FR-03.1 | Panggil prompt AI lewat pintasan/ikon | ✅ | Tab ✨ dan `Ctrl+Shift+A` | Diuji |
| FR-03.2 | Kirim ke endpoint AI, tampilkan balasan singkat | ⚠️ | `ai.rs`: Claude (Anthropic Messages) dan "kompatibel OpenAI" (OpenAI, OpenRouter, Groq, Ollama, LM Studio…) | Format permintaan/balasan dan semua pemetaan error diuji terhadap server tiruan. **Belum diuji dengan API sungguhan** (tanpa kunci/jaringan). Model bawaan Claude `claude-haiku-5-5` dapat diubah |
| FR-04.1 | Sakelar autostart | ✅ | `tauri-plugin-autostart` di ⚙ Pengaturan | Sudah ada sebelumnya |
| FR-04.2 | Hotkey `Ctrl+Shift+Space` | ✅ | Pintasan global (`main.rs`) | Jika kombinasi sudah dipakai aplikasi lain, pendaftaran gagal diam-diam |

## Persyaratan non-fungsional

| ID | Persyaratan | Status | Catatan |
|---|---|---|---|
| NFR-01 | RAM idle ≤ 80 MB | ❓ | **Tidak bisa dibuktikan dari sini.** WebView2 berjalan sebagai proses terpisah dan banyak aplikasi sejenis melampaui 80 MB. Modul Monitor menampilkan angka aktual (aplikasi + proses WebView) dengan warna hijau/kuning terhadap target 80 MB. Untuk mengukur "idle": tutup Monitor, tunggu 1 menit, lihat grup "Dynamic Island Radio" di Task Manager. Penghemat yang sudah ada: sampler sistem hanya hidup saat modul Monitor terbuka; animasi waveform hanya berjalan saat terlihat |
| NFR-02 | Respons morphing saat hover < 200 ms | ⚠️ | Hover pada notch terdeteksi tiap 80 ms, animasi berdurasi 200 ms. Diartikan sebagai "animasi mulai < 200 ms". Belum diukur di Windows |
| NFR-03 | Tanpa kredensial teks biasa | ✅ | API key dan kunci enkripsi hanya di Credential Manager; API key dikirim ke Rust, tidak pernah dibaca kembali oleh halaman web. Diuji: kunci tidak muncul di `localStorage` maupun DOM |
| NFR-04 | Tanpa modul pihak ketiga tak terverifikasi | ⚠️ | Hanya pustaka umum (Tauri, reqwest, sysinfo, aes-gcm, keyring); tidak diaudit. Koneksi keluar: pemutaran stream yang Anda pilih, pengunduhan ikon, dan permintaan AI yang Anda kirim |
| NFR-05 | Tema Holographic Glassmorphism | ✅ | Kaca blur + bingkai neon bergradasi + kilau tepi; diperiksa di WebKit |
| NFR-06 | Indikator status neon | ✅ | Kilau tepi: aksen stasiun = memutar, kuning = menyambung, merah = gagal, abu-abu = idle |

## Batasan (SRS 2.4)
- Integrasi WhatsApp **tidak dibuat** (sesuai SRS).
- Target Windows 10/11 64-bit dengan WebView2.

## Penyimpangan dari teks SRS
- "Local storage terenkripsi" diwujudkan sebagai **berkas terenkripsi di folder data aplikasi**, bukan `localStorage` browser
  (yang tidak bisa dienkripsi dengan kunci di Credential Manager).
- Asisten AI **tidak memakai riwayat percakapan** (satu pertanyaan per permintaan) agar ringan dan murah.
