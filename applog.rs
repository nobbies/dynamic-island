//! Log teks sederhana di folder log aplikasi (bukan data pribadi): dibuat untuk mencari penyebab masalah seperti
//! "aplikasi lain terbuka saat startup". Berputar otomatis bila lebih dari 200 KB.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::OnceLock,
};

static FILE: OnceLock<PathBuf> = OnceLock::new();

pub fn init(dir: &Path) {
    let _ = fs::create_dir_all(dir);
    let p = dir.join("app.log");
    if fs::metadata(&p).map(|m| m.len() > 200_000).unwrap_or(false) {
        let _ = fs::rename(&p, dir.join("app.old.log"));
    }
    let _ = FILE.set(p);
}

pub fn path() -> String {
    FILE.get().map(|p| p.display().to_string()).unwrap_or_else(|| "(belum siap)".into())
}

pub fn log(msg: &str) {
    if let Some(p) = FILE.get() {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{} {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
        }
    }
}

pub fn tail(n: usize) -> String {
    let Some(p) = FILE.get() else { return String::new() };
    let text = fs::read_to_string(p).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tulis_dan_baca_ekor() {
        let dir = std::env::temp_dir().join(format!("din-log-test-{}", std::process::id()));
        init(&dir);
        for i in 0..10 {
            log(&format!("baris {i}"));
        }
        let t = tail(3);
        assert_eq!(t.lines().count(), 3);
        assert!(t.contains("baris 9") && !t.contains("baris 6"));
        assert!(path().ends_with("app.log"));
        let _ = fs::remove_dir_all(&dir);
    }
}
