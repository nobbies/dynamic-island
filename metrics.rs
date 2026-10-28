//! Pemantau CPU/RAM. Pengambil sampel (thread) HANYA berjalan selama modul Monitor dibuka,
//! jadi tidak membebani CPU/RAM saat aplikasi idle.
use serde::Serialize;
use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use sysinfo::{Pid, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub struct Control {
    active: AtomicBool,  // apakah ada yang menonton
    running: AtomicBool, // apakah thread sedang hidup
}

#[derive(Serialize, Clone)]
pub struct Sample {
    pub cpu: f32,
    pub ram_used: u64,
    pub ram_total: u64,
    pub app_mb: Option<u64>, // memori aplikasi + proses anaknya (WebView2); dihitung tiap 5 detik
}

// Total memori proses ini beserta semua keturunannya (proses WebView2 adalah anak proses aplikasi).
fn app_memory_mb(sys: &mut System) -> u64 {
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let Ok(me) = sysinfo::get_current_pid() else { return 0 };
    let mut family: HashSet<Pid> = HashSet::from([me]);
    loop {
        let before = family.len();
        for (pid, p) in sys.processes() {
            if p.parent().is_some_and(|par| family.contains(&par)) {
                family.insert(*pid);
            }
        }
        if family.len() == before {
            break;
        }
    }
    family.iter().filter_map(|p| sys.process(*p)).map(|p| p.memory()).sum::<u64>() / 1_048_576
}

pub fn start(app: AppHandle) {
    let ctl = app.state::<Control>();
    ctl.active.store(true, Ordering::SeqCst);
    if ctl.running.swap(true, Ordering::SeqCst) {
        return; // thread sudah hidup
    }
    std::thread::spawn(move || {
        let mut sys = System::new();
        sys.refresh_cpu_usage(); // pembacaan pertama sebagai titik awal
        let mut tick = 0u32;
        loop {
            std::thread::sleep(Duration::from_millis(1000));
            let ctl = app.state::<Control>();
            if !ctl.active.load(Ordering::SeqCst) {
                ctl.running.store(false, Ordering::SeqCst);
                break;
            }
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            let app_mb = if tick % 5 == 0 { Some(app_memory_mb(&mut sys)) } else { None };
            tick += 1;
            let _ = app.emit(
                "metrics",
                Sample { cpu: sys.global_cpu_usage(), ram_used: sys.used_memory(), ram_total: sys.total_memory(), app_mb },
            );
        }
    });
}

pub fn stop(app: &AppHandle) {
    app.state::<Control>().active.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angka_sistem_masuk_akal() {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        std::thread::sleep(Duration::from_millis(300));
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let cpu = sys.global_cpu_usage();
        assert!((0.0..=100.0).contains(&cpu), "cpu: {cpu}");
        assert!(sys.total_memory() > 0 && sys.used_memory() > 0 && sys.used_memory() <= sys.total_memory());

        // memori proses ini; lalu jalankan proses anak dan pastikan ikut dihitung (seperti WebView2)
        let tanpa_anak = app_memory_mb(&mut sys);
        assert!(tanpa_anak > 0, "memori aplikasi harus > 0");
        let mut child = std::process::Command::new("sleep").arg("5").spawn().expect("sleep tersedia");
        std::thread::sleep(Duration::from_millis(200));
        let dengan_anak = app_memory_mb(&mut sys);
        let _ = child.kill();
        assert!(dengan_anak >= tanpa_anak, "proses anak harus ikut dihitung ({tanpa_anak} -> {dengan_anak})");
    }
}
