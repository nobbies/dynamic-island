// Sembunyikan jendela konsol di Windows pada build rilis.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod applog;
mod detect;
mod metrics;
mod secrets;
mod vault;

use std::time::Duration;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as _};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

// Ukuran tiap mode (piksel logis). Lebar = lebar pulau (320/380) + 2 x 14 px "telinga" cekung di sudut atas (lihat .shell di style.css).
//  Jendela = ukuran widget, jadi tidak ada area transparan yang menutupi layar.
fn size_of(mode: &str) -> (f64, f64) {
    match mode {
        "expanded" => (408.0, 304.0),
        "library" => (408.0, 540.0),
        "settings" => (408.0, 440.0),
        "notch" => (120.0, 6.0),
        _ => (348.0, 44.0), // compact
    }
}

// Tempatkan jendela di tengah-atas monitor yang sedang dipakai, menempel ke tepi atas.
fn place(win: &WebviewWindow, mode: &str) -> Result<(), String> {
    let (w, h) = size_of(mode);
    let mon = match win.current_monitor().map_err(|e| e.to_string())? {
        Some(m) => m,
        None => win
            .primary_monitor()
            .map_err(|e| e.to_string())?
            .ok_or("monitor tidak ditemukan")?,
    };
    let scale = mon.scale_factor();
    let (pw, ph) = ((w * scale).round() as u32, (h * scale).round() as u32);
    let x = mon.position().x + ((mon.size().width as f64 - pw as f64) / 2.0).round() as i32;
    win.set_size(PhysicalSize::new(pw, ph)).map_err(|e| e.to_string())?;
    win.set_position(PhysicalPosition::new(x, mon.position().y))
        .map_err(|e| e.to_string())?;
    // Hanya mode notch yang tembus-klik; hovernya dideteksi lewat pembaca kursor di bawah.
    win.set_ignore_cursor_events(mode == "notch").map_err(|e| e.to_string())
}

#[tauri::command]
fn set_mode(window: WebviewWindow, mode: String) -> Result<(), String> {
    place(&window, &mode)
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

// Mulai otomatis saat login (Windows: registri Run milik pengguna; macOS: LaunchAgent; Linux: .desktop autostart)
#[tauri::command]
fn autostart_status(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn autostart_set(app: AppHandle, enabled: bool) -> Result<bool, String> {
    // Build debug (npm run tauri dev) memuat halaman dari server dev dan membuka jendela konsol. Jika didaftarkan ke
    // startup, Windows akan menjalankannya saat login dan hasilnya rusak. Jadi MENYALAKAN hanya boleh dari build rilis.
    if enabled && cfg!(debug_assertions) {
        return Err("Startup hanya bisa dinyalakan dari aplikasi hasil build (npm run tauri build), bukan mode dev.".into());
    }
    let al = app.autolaunch();
    let r = if enabled { al.enable() } else { al.disable() };
    r.map_err(|e| e.to_string())?;
    al.is_enabled().map_err(|e| e.to_string())
}

// ---- Monitor sistem (sampler hanya hidup saat modul dibuka)
#[tauri::command]
fn metrics_start(app: AppHandle) {
    metrics::start(app);
}

#[tauri::command]
fn metrics_stop(app: AppHandle) {
    metrics::stop(&app);
}

// ---- Catatan & tugas: berkas terenkripsi di folder data aplikasi, kunci di Credential Manager
fn vault_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("notes.vault"))
}

#[tauri::command]
fn vault_load(app: AppHandle) -> Result<String, String> {
    let p = vault_path(&app)?;
    if !p.exists() {
        return Ok(String::new()); // belum ada catatan
    }
    vault::read(&p, &vault::key()?)
}

#[tauri::command]
fn vault_save(app: AppHandle, data: String) -> Result<(), String> {
    vault::write(&vault_path(&app)?, &vault::key()?, &data)
}

// Membuang berkas catatan yang tidak bisa dibuka (mis. kunci hilang) agar bisa mulai baru.
#[tauri::command]
fn vault_reset(app: AppHandle) -> Result<(), String> {
    let p = vault_path(&app)?;
    if p.exists() {
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---- Asisten AI: API key hanya di Credential Manager; JavaScript hanya bisa bertanya "ada/tidak"
#[tauri::command]
fn ai_key_status() -> bool {
    secrets::get("ai-api-key").ok().flatten().is_some()
}

#[tauri::command]
fn ai_key_set(key: String) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() || key.len() > 2000 || key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("API key tidak valid (kosong, terlalu panjang, atau mengandung spasi).".into());
    }
    secrets::set("ai-api-key", key)
}

#[tauri::command]
fn ai_key_clear() -> Result<(), String> {
    secrets::delete("ai-api-key")
}

#[tauri::command]
async fn ai_ask(provider: String, endpoint: String, model: String, prompt: String) -> Result<String, String> {
    let key = secrets::get("ai-api-key")?;
    ai::ask(&provider, &endpoint, &model, key.as_deref(), &prompt).await
}

// ---- Diagnostik: info proses + isi log, untuk mencari penyebab masalah (mis. aplikasi lain terbuka saat startup)
#[cfg(windows)]
fn run_keys() -> String {
    use std::os::windows::process::CommandExt;
    let mut out = String::new();
    for key in [r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", r"HKLM\Software\Microsoft\Windows\CurrentVersion\Run"] {
        let r = std::process::Command::new("reg").args(["query", key]).creation_flags(0x0800_0000).output();
        out += &format!("[{key}]\n");
        match r {
            // hanya entri milik aplikasi ini dan Microsoft Edge yang relevan
            Ok(o) => {
                for l in String::from_utf8_lossy(&o.stdout).lines() {
                    let low = l.to_lowercase();
                    if low.contains("dynamic") || low.contains("edge") {
                        out += &format!("  {}\n", l.trim());
                    }
                }
            }
            Err(e) => out += &format!("  gagal membaca: {e}\n"),
        }
    }
    out
}
#[cfg(not(windows))]
fn run_keys() -> String {
    "(hanya di Windows)\n".into()
}

#[tauri::command]
fn diag_text(app: AppHandle) -> String {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    let mut s = format!(
        "Versi: {}\nProses: {}\nArgumen: {:?}\nDimulai otomatis (--autostart): {}\nStartup aktif: {}\nWebView2: {}\nLog: {}\n\n--- entri startup Windows (Run) yang berkaitan ---\n",
        app.package_info().version,
        exe,
        std::env::args().skip(1).collect::<Vec<_>>(),
        std::env::args().any(|a| a == "--autostart"),
        app.autolaunch().is_enabled().unwrap_or(false),
        tauri::webview_version().unwrap_or_else(|e| format!("tidak terbaca ({e})")),
        applog::path(),
    );
    s += &run_keys();
    s += "\n--- log terakhir ---\n";
    s += &applog::tail(40);
    s
}

#[tauri::command]
async fn probe(url: String) -> detect::Probe {
    detect::probe(&url).await
}

// Unduh gambar ikon dari URL dan kembalikan sebagai data URL (tersimpan lokal, tidak bergantung situs asal)
#[tauri::command]
async fn fetch_icon(url: String, referer: Option<String>) -> Result<String, String> {
    detect::fetch_icon(&url, referer.as_deref()).await
}

#[tauri::command]
async fn detect(url: String) -> Option<detect::Found> {
    detect::detect(&url).await
}

// Halaman aplikasi hanya boleh berada di alamat aplikasi sendiri. Navigasi ke luar atau pembukaan jendela/baru ditolak dan dicatat.
fn app_url(url: &tauri::Url) -> bool {
    matches!(url.scheme(), "tauri" | "about" | "data" | "blob")
        || matches!(url.host_str(), Some("tauri.localhost" | "localhost" | "127.0.0.1"))
}

fn main() {
    let play = Shortcut::new(None, Code::MediaPlayPause);
    let next = Shortcut::new(None, Code::MediaTrackNext);
    let prev = Shortcut::new(None, Code::MediaTrackPrevious);
    let alt_r = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyR);
    let panel = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space); // buka/tutup panel (FR-04.2)
    let ask_ai = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyA); // langsung ke asisten AI (FR-03.1)
    let (id_play, id_next, id_prev, id_alt) = (play.id(), next.id(), prev.id(), alt_r.id());
    let (id_panel, id_ai) = (panel.id(), ask_ai.id());

    tauri::Builder::default()
        // Harus plugin PERTAMA. Jika aplikasi dibuka lagi, salinan kedua langsung keluar dan salinan pertama
        // dibangunkan (supaya suara tidak dobel).
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            applog::log(&format!("instance kedua dicegah, argumen {args:?}"));
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            let _ = app.emit("second-instance", ());
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--autostart"])))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, sc, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let id = sc.id();
                    if id == id_panel || id == id_ai {
                        // tampilkan dan fokuskan jendela agar bisa langsung mengetik
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                        let _ = app.emit("hotkey", if id == id_panel { "panel" } else { "ai" });
                        return;
                    }
                    let action = if id == id_play || id == id_alt {
                        "toggle"
                    } else if id == id_next {
                        "next"
                    } else if id == id_prev {
                        "prev"
                    } else {
                        return;
                    };
                    let _ = app.emit("media", action);
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            diag_text, set_mode, quit_app, autostart_status, autostart_set, fetch_icon, probe, detect,
            metrics_start, metrics_stop, vault_load, vault_save, vault_reset,
            ai_key_status, ai_key_set, ai_key_clear, ai_ask
        ])
        .on_page_load(|_, payload| applog::log(&format!("halaman {:?}: {}", payload.event(), payload.url())))
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(false) = event {
                let _ = window.emit("window-blurred", ());
            }
        })
        .setup(move |app| {
            app.manage(metrics::Control::default());
            applog::init(&app.path().app_log_dir()?);
            std::panic::set_hook(Box::new(|info| applog::log(&format!("PANIC: {info}"))));
            applog::log(&format!(
                "mulai v{} | argumen {:?} | otomatis-saat-login: {}",
                app.package_info().version,
                std::env::args().skip(1).collect::<Vec<_>>(),
                std::env::args().any(|a| a == "--autostart")
            ));

            let cfg = app.config().app.windows.first().cloned().ok_or("konfigurasi jendela tidak ada")?;
            let win = tauri::WebviewWindowBuilder::from_config(app.handle(), &cfg)?
                .on_navigation(|url| {
                    let ok = app_url(url);
                    if !ok {
                        applog::log(&format!("navigasi ke luar DITOLAK: {url}"));
                    }
                    ok
                })
                .on_new_window(|url, _| {
                    applog::log(&format!("jendela baru DITOLAK: {url}"));
                    tauri::webview::NewWindowResponse::Deny
                })
                .build()?;
            place(&win, "compact")?;
            win.show()?;

            // Perbaiki otomatis: jika startup sudah aktif, tulis ulang dengan lokasi aplikasi yang sedang berjalan.
            // (Menimpa entri lama yang salah menunjuk ke build dev.) Hanya di build rilis.
            #[cfg(not(debug_assertions))]
            {
                let al = app.autolaunch();
                if al.is_enabled().unwrap_or(false) {
                    let _ = al.enable();
                }
            }

            // Tombol media keyboard + Ctrl/Cmd+Alt+R (jika sudah dipakai aplikasi lain, abaikan saja)
            let gs = app.global_shortcut();
            for s in [play, next, prev, alt_r, panel, ask_ai] {
                let _ = gs.register(s);
            }

            // Saat mode notch jendela tembus-klik, sehingga mouseenter tidak terkirim ke halaman.
            // Rust memantau posisi kursor dan memberi tahu halaman saat kursor masuk/keluar jendela.
            let watcher = win.clone();
            std::thread::spawn(move || {
                let mut was_inside = false;
                loop {
                    std::thread::sleep(Duration::from_millis(80));
                    let (Ok(c), Ok(p), Ok(s)) =
                        (watcher.cursor_position(), watcher.outer_position(), watcher.outer_size())
                    else {
                        continue;
                    };
                    let inside = c.x >= p.x as f64
                        && c.x < p.x as f64 + s.width as f64
                        && c.y >= p.y as f64
                        && c.y < p.y as f64 + s.height as f64;
                    if inside != was_inside {
                        was_inside = inside;
                        let _ = watcher.emit("cursor-inside", inside);
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("gagal menjalankan aplikasi Tauri");
}
