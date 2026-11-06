//! Pemantau sistem: CPU, RAM, GPU, jaringan (Ethernet/Wi-Fi), dan 10 aplikasi teratas. Pengambil sampel (thread) HANYA berjalan
//! selama modul Monitor dibuka, jadi tidak membebani CPU/RAM saat aplikasi idle.
use crate::{
    gpu::{Gpu, Thermal},
    sensor::SensorReader,
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use sysinfo::{Networks, Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub struct Control {
    active: AtomicBool,  // apakah ada yang menonton
    running: AtomicBool, // apakah thread sedang hidup
}

#[derive(Serialize, Clone, Debug)]
pub struct Net {
    pub kind: String, // "Ethernet" | "Wi-Fi" | "Jaringan" | "" (tidak ada adaptor fisik)
    pub rx: u64,      // byte/detik masuk
    pub tx: u64,      // byte/detik keluar
}

#[derive(Serialize, Clone, Debug)]
pub struct Row {
    pub name: String,
    pub cpu: f32, // % dari seluruh CPU (0-100), seperti Task Manager
    pub mem: u64, // byte
    pub ours: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Procs {
    pub cpu: Vec<Row>,
    pub mem: Vec<Row>,
}

#[derive(Serialize, Clone)]
pub struct Sample {
    pub cpu: f32,
    pub cpu_temp: Option<f32>,
    pub ram_used: u64,
    pub ram_total: u64,
    pub gpu: Option<f32>,
    pub gpu_temp: Option<f32>,
    pub net: Net,
    pub procs: Option<Procs>, // diperbarui tiap 2 detik
}

// ---------------- jaringan ----------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Ethernet = 0,
    WiFi = 1,
    Other = 2,
}

/// Jenis adaptor dari namanya; None = adaptor virtual/loopback yang diabaikan (VPN, Hyper-V, WSL, Docker, Bluetooth, dst.).
pub fn net_kind(name: &str) -> Option<Kind> {
    let n = name.to_lowercase();
    const SKIP: [&str; 24] = [
        "loopback", "virtual", "vethernet", "vmware", "vmnet", "hyper-v", "bluetooth", "tap-", "tunnel", "teredo", "isatap", "wsl",
        "docker", "veth", "br-", "pseudo", "wintun", "tailscale", "zerotier", "vpn", "npcap", "wan miniport", "kernel debug", "utun",
    ];
    if n == "lo" || SKIP.iter().any(|s| n.contains(s)) {
        return None;
    }
    if ["wi-fi", "wifi", "wireless", "wlan", "802.11"].iter().any(|s| n.contains(s)) || n.starts_with("wl") {
        return Some(Kind::WiFi);
    }
    if n.contains("ethernet") || n.contains("gbe") || n.contains("family controller") || n.contains("local area connection")
        || n.starts_with("eth") || n.starts_with("en")
    {
        return Some(Kind::Ethernet);
    }
    Some(Kind::Other)
}

/// Pilih jenis adaptor yang paling banyak dipakai (total lalu lintas terbesar; seri: Ethernet, Wi-Fi, lainnya) dan hitung kecepatan.
/// Item = (nama, total byte sejak boot, byte masuk sejak pembacaan terakhir, byte keluar sejak pembacaan terakhir).
pub fn pick_net<'a>(items: impl Iterator<Item = (&'a str, u64, u64, u64)>, dt: f64) -> Net {
    let mut acc: HashMap<u8, (u64, u64, u64)> = HashMap::new();
    for (name, total, rx, tx) in items {
        if let Some(k) = net_kind(name) {
            let e = acc.entry(k as u8).or_insert((0, 0, 0));
            e.0 += total;
            e.1 += rx;
            e.2 += tx;
        }
    }
    let best = [Kind::Ethernet, Kind::WiFi, Kind::Other]
        .into_iter()
        .filter_map(|k| acc.get(&(k as u8)).map(|v| (k, *v)))
        .max_by_key(|(k, (total, _, _))| (*total, std::cmp::Reverse(*k as u8)));
    match best {
        Some((k, (_, rx, tx))) => Net {
            kind: match k {
                Kind::Ethernet => "Ethernet",
                Kind::WiFi => "Wi-Fi",
                Kind::Other => "Jaringan",
            }
            .into(),
            rx: (rx as f64 / dt) as u64,
            tx: (tx as f64 / dt) as u64,
        },
        None => Net { kind: String::new(), rx: 0, tx: 0 },
    }
}

// ---------------- proses ----------------

/// Proses ini beserta semua keturunannya (proses WebView2 adalah anak proses aplikasi).
pub fn family(sys: &System) -> HashSet<Pid> {
    let Ok(me) = sysinfo::get_current_pid() else { return HashSet::new() };
    let mut fam: HashSet<Pid> = HashSet::from([me]);
    loop {
        let before = fam.len();
        for (pid, p) in sys.processes() {
            if p.parent().is_some_and(|par| fam.contains(&par)) {
                fam.insert(*pid);
            }
        }
        if fam.len() == before {
            break;
        }
    }
    fam
}

fn clean(name: &str) -> String {
    let n = name.trim();
    if n.len() > 4 && n[n.len() - 4..].eq_ignore_ascii_case(".exe") { n[..n.len() - 4].to_string() } else { n.to_string() }
}

/// 10 aplikasi teratas menurut CPU dan menurut RAM. Proses bernama sama digabung (satu aplikasi bisa banyak proses);
/// aplikasi ini (termasuk WebView-nya) dihitung sebagai satu baris "Dynamic Island Radio".
pub fn top_procs(sys: &System) -> Procs {
    let ncpu = sys.cpus().len().max(1) as f32;
    let fam = family(sys);
    let mut by: HashMap<String, Row> = HashMap::new();
    let mut ours = Row { name: "Dynamic Island Radio".into(), cpu: 0.0, mem: 0, ours: true };
    for (pid, p) in sys.processes() {
        let name = p.name().to_string_lossy();
        if name.eq_ignore_ascii_case("System Idle Process") {
            continue;
        }
        if fam.contains(pid) {
            ours.cpu += p.cpu_usage() / ncpu;
            ours.mem += p.memory();
            continue;
        }
        let key = clean(&name);
        let e = by.entry(key.to_lowercase()).or_insert(Row { name: key, cpu: 0.0, mem: 0, ours: false });
        e.cpu += p.cpu_usage() / ncpu;
        e.mem += p.memory();
    }
    let mut all: Vec<Row> = by.into_values().collect();
    if ours.mem > 0 {
        all.push(ours);
    }
    let top = |mut v: Vec<Row>, by_cpu: bool| {
        v.sort_by(|a, b| if by_cpu { b.cpu.total_cmp(&a.cpu) } else { b.mem.cmp(&a.mem) });
        v.truncate(10);
        v
    };
    Procs { cpu: top(all.clone(), true), mem: top(all, false) }
}

// ---------------- pengambil sampel ----------------

pub fn start(app: AppHandle) {
    let ctl = app.state::<Control>();
    ctl.active.store(true, Ordering::SeqCst);
    if ctl.running.swap(true, Ordering::SeqCst) {
        return; // thread sudah hidup
    }
    std::thread::spawn(move || {
        let mut sys = System::new();
        let mut nets = Networks::new_with_refreshed_list();
        let mut gpu = Gpu::new();
        let mut thermal = Thermal::new();
        let mut sensor = SensorReader::new(&app);
        let kind = ProcessRefreshKind::nothing().with_cpu().with_memory();
        sys.refresh_cpu_usage(); // pembacaan pertama sebagai titik awal
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        let (mut tick, mut last) = (0u32, Instant::now());
        loop {
            std::thread::sleep(Duration::from_millis(1000));
            let ctl = app.state::<Control>();
            if !ctl.active.load(Ordering::SeqCst) {
                ctl.running.store(false, Ordering::SeqCst);
                break;
            }
            let dt = last.elapsed().as_secs_f64().max(0.2);
            last = Instant::now();
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            nets.refresh(true);
            let net = pick_net(nets.iter().map(|(n, d)| (n.as_str(), d.total_received() + d.total_transmitted(), d.received(), d.transmitted())), dt);
            let procs = if tick % 2 == 1 {
                sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
                Some(top_procs(&sys))
            } else {
                None
            };
            tick += 1;
            let temperatures = sensor.sample();
            let cpu_temp = temperatures.cpu_temp.or_else(|| thermal.cpu_sample());
            let _ = app.emit(
                "metrics",
                Sample {
                    cpu: sys.global_cpu_usage(),
                    cpu_temp,
                    ram_used: sys.used_memory(),
                    ram_total: sys.total_memory(),
                    gpu: gpu.sample(),
                    gpu_temp: temperatures.gpu_temp,
                    net,
                    procs,
                },
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
    fn jenis_adaptor() {
        use Kind::*;
        let cases: [(&str, Option<Kind>); 22] = [
            ("Ethernet", Some(Ethernet)), ("Ethernet 2", Some(Ethernet)), ("Realtek PCIe GbE Family Controller", Some(Ethernet)),
            ("Intel(R) Ethernet Connection (7) I219-V", Some(Ethernet)), ("Realtek USB FE Family Controller", Some(Ethernet)),
            ("Local Area Connection", Some(Ethernet)), ("eth0", Some(Ethernet)), ("enp3s0", Some(Ethernet)),
            ("Wi-Fi", Some(WiFi)), ("Intel(R) Wi-Fi 6 AX201 160MHz", Some(WiFi)), ("wlan0", Some(WiFi)), ("Wireless Network Connection", Some(WiFi)),
            ("vEthernet (WSL)", None), ("Hyper-V Virtual Ethernet Adapter", None), ("VirtualBox Host-Only Ethernet Adapter", None),
            ("Loopback Pseudo-Interface 1", None), ("Bluetooth Network Connection", None), ("lo", None), ("docker0", None),
            ("Microsoft Wi-Fi Direct Virtual Adapter", None), ("TAP-Windows Adapter V9", None), ("Mystery Adapter", Some(Other)),
        ];
        for (name, want) in cases {
            assert_eq!(net_kind(name), want, "nama: {name}");
        }
    }

    #[test]
    fn pilih_adaptor_dan_kecepatan() {
        // Ethernet lebih banyak dipakai daripada Wi-Fi; adaptor virtual diabaikan walau trafiknya besar
        let items = vec![("Ethernet", 9_000u64, 2_048u64, 512u64), ("Wi-Fi", 100, 10, 10), ("vEthernet (WSL)", 99_999, 99_999, 99_999)];
        let n = pick_net(items.iter().map(|(a, b, c, d)| (*a, *b, *c, *d)), 2.0);
        assert_eq!((n.kind.as_str(), n.rx, n.tx), ("Ethernet", 1024, 256), "{n:?}");
        // hanya Wi-Fi
        let n = pick_net([("Wi-Fi", 5u64, 100u64, 50u64)].into_iter(), 1.0);
        assert_eq!((n.kind.as_str(), n.rx, n.tx), ("Wi-Fi", 100, 50));
        // gabungan beberapa adaptor sejenis dijumlahkan
        let n = pick_net([("Ethernet", 1u64, 10u64, 1u64), ("Ethernet 2", 1, 20, 2)].into_iter(), 1.0);
        assert_eq!((n.rx, n.tx), (30, 3));
        // tidak ada adaptor fisik
        let n = pick_net([("lo", 1u64, 1u64, 1u64)].into_iter(), 1.0);
        assert!(n.kind.is_empty() && n.rx == 0);
    }

    #[test]
    fn angka_sistem_dan_aplikasi_teratas() {
        let kind = ProcessRefreshKind::nothing().with_cpu().with_memory();
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        std::thread::sleep(Duration::from_millis(400));
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        assert!((0.0..=100.0).contains(&sys.global_cpu_usage()));
        assert!(sys.total_memory() > 0 && sys.used_memory() <= sys.total_memory());

        let before = family(&sys).len();
        let mut child = std::process::Command::new("sleep").arg("5").spawn().expect("sleep tersedia");
        std::thread::sleep(Duration::from_millis(200));
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        let after = family(&sys).len();
        let p = top_procs(&sys);
        let _ = child.kill();
        assert!(after > before, "proses anak harus ikut keluarga aplikasi ({before} -> {after})");

        assert!(p.cpu.len() <= 10 && p.mem.len() <= 10 && !p.mem.is_empty());
        assert!(p.mem.windows(2).all(|w| w[0].mem >= w[1].mem), "urut menurut RAM");
        assert!(p.cpu.windows(2).all(|w| w[0].cpu >= w[1].cpu), "urut menurut CPU");
        assert!(p.mem.iter().all(|r| !r.name.to_lowercase().ends_with(".exe") && r.name != "System Idle Process"));
        let all: Vec<&Row> = p.mem.iter().chain(p.cpu.iter()).collect();
        assert!(all.iter().filter(|r| r.ours).all(|r| r.name == "Dynamic Island Radio" && r.mem > 0));
        let nama: Vec<String> = p.mem.iter().map(|r| r.name.to_lowercase()).collect();
        assert_eq!(nama.len(), nama.iter().collect::<HashSet<_>>().len(), "nama yang sama digabung");
    }

    #[test]
    fn nama_tanpa_exe() {
        assert_eq!(clean("chrome.exe"), "chrome");
        assert_eq!(clean("MSEDGE.EXE"), "MSEDGE");
        assert_eq!(clean("Code"), "Code");
        assert_eq!(clean(".exe"), ".exe");
    }
}
