use if_addrs::{get_if_addrs, IfAddr};
use serde::Serialize;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, UdpSocket},
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};

const MAX_SUBNET_ADDRESSES: u32 = 256;
const WORKERS: usize = 16;

#[derive(Serialize)]
pub struct Device {
    pub ip: String,
    pub name: String,
    pub is_this_device: bool,
    pub active: bool,
    pub ping_ms: Option<u64>,
    pub mac: Option<String>,
}

#[derive(Serialize)]
pub struct ScanResult {
    pub local_ip: String,
    pub subnet: String,
    pub devices: Vec<Device>,
}

#[derive(Serialize, Clone)]
pub struct ScanProgress {
    pub completed: usize,
    pub total: usize,
    pub active: usize,
    pub current_ip: String,
}

struct PingResult {
    ip: Ipv4Addr,
    active: bool,
    ping_ms: Option<u64>,
}

fn route_ipv4() -> Result<Ipv4Addr, String> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("Tidak bisa memilih antarmuka jaringan: {e}"))?;
    socket
        .connect("192.0.2.1:80")
        .map_err(|e| format!("Tidak bisa menentukan jaringan lokal: {e}"))?;
    match socket.local_addr().map_err(|e| e.to_string())?.ip() {
        std::net::IpAddr::V4(ip) if ip.is_private() => Ok(ip),
        _ => Err("Tidak ditemukan koneksi jaringan IPv4 privat.".into()),
    }
}

fn local_subnet() -> Result<(Ipv4Addr, Ipv4Addr, u8), String> {
    let local_ip = route_ipv4()?;
    let interfaces =
        get_if_addrs().map_err(|e| format!("Gagal membaca antarmuka jaringan: {e}"))?;
    let netmask = interfaces
        .into_iter()
        .find_map(|interface| match interface.addr {
            IfAddr::V4(addr) if addr.ip == local_ip => Some(addr.netmask),
            _ => None,
        })
        .ok_or("Subnet IPv4 lokal tidak ditemukan.")?;

    let mask = u32::from(netmask);
    let prefix = mask.leading_ones() as u8;
    let expected_mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    if mask != expected_mask || !(24..=30).contains(&prefix) {
        return Err("Pemindaian dibatasi ke subnet IPv4 /24 hingga /30.".into());
    }
    let addresses = 1u32 << (32 - prefix);
    if addresses > MAX_SUBNET_ADDRESSES {
        return Err("Subnet terlalu besar untuk pemindaian lokal yang dibatasi.".into());
    }
    Ok((local_ip, netmask, prefix))
}

fn ping(ip: Ipv4Addr) -> Result<bool, String> {
    let mut command = Command::new("ping");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command
            .args(["-n", "1", "-w", "600", &ip.to_string()])
            .creation_flags(0x0800_0000);
    }
    #[cfg(target_os = "macos")]
    command.args(["-n", "-c", "1", "-W", "600", &ip.to_string()]);
    #[cfg(all(unix, not(target_os = "macos")))]
    command.args(["-n", "-c", "1", "-W", "1", &ip.to_string()]);

    let output = command
        .output()
        .map_err(|e| format!("Gagal menjalankan ping: {e}"))?;
    Ok(output.status.success())
}

fn device_name(ip: Ipv4Addr) -> String {
    dns_lookup::lookup_addr(&std::net::IpAddr::V4(ip))
        .ok()
        .map(|name| name.trim_end_matches('.').to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| ip.to_string())
}

fn parse_arp_table(text: &str) -> HashMap<Ipv4Addr, String> {
    let mut entries = HashMap::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        for (index, field) in fields.iter().enumerate() {
            let Ok(IpAddr::V4(ip)) = field.trim_matches(['(', ')']).parse::<IpAddr>() else {
                continue;
            };
            if let Some(mac) = fields.get(index + 1).filter(|value| {
                value.matches(['-', ':']).count() == 5
                    && value
                        .split(['-', ':'])
                        .all(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_hexdigit()))
            }) {
                entries.insert(ip, mac.to_ascii_uppercase().replace('-', ":"));
            }
        }
    }
    entries
}

fn arp_table() -> Result<HashMap<Ipv4Addr, String>, String> {
    let output = Command::new("arp")
        .arg("-a")
        .output()
        .map_err(|e| format!("Tidak bisa membaca tabel ARP untuk alamat MAC: {e}"))?;
    if !output.status.success() {
        return Err("Gagal membaca tabel ARP untuk alamat MAC.".into());
    }
    Ok(parse_arp_table(&String::from_utf8_lossy(&output.stdout)))
}

pub fn scan(mut report_progress: impl FnMut(ScanProgress)) -> Result<ScanResult, String> {
    let (local_ip, netmask, prefix) = local_subnet()?;
    let network = u32::from(local_ip) & u32::from(netmask);
    let broadcast = network | !u32::from(netmask);
    let targets: Vec<Ipv4Addr> = ((network + 1)..broadcast)
        .filter(|ip| *ip != u32::from(local_ip))
        .map(Ipv4Addr::from)
        .collect();
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let (tx, rx) = mpsc::channel();
    let mut results = Vec::with_capacity(targets.len());
    let mut scan_error = None;
    let mut active = 0;
    let total = targets.len();

    thread::scope(|scope| {
        for _ in 0..WORKERS.min(targets.len()) {
            let tx = tx.clone();
            let targets = &targets;
            let next = &next;
            let failed = &failed;
            scope.spawn(move || {
                while !failed.load(Ordering::Relaxed) {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(ip) = targets.get(index).copied() else {
                        break;
                    };
                    let start = Instant::now();
                    match ping(ip) {
                        Ok(is_active) => {
                            let result = PingResult {
                                ip,
                                active: is_active,
                                ping_ms: is_active.then(|| start.elapsed().as_millis() as u64),
                            };
                            let _ = tx.send(Ok(result));
                        }
                        Err(error) => {
                            failed.store(true, Ordering::Relaxed);
                            let _ = tx.send(Err(error));
                            break;
                        }
                    }
                }
            });
        }
        drop(tx);
        for result in rx {
            match result {
                Ok(result) => {
                    if result.active {
                        active += 1;
                    }
                    results.push(result);
                    report_progress(ScanProgress {
                        completed: results.len(),
                        total,
                        active,
                        current_ip: results.last().unwrap().ip.to_string(),
                    });
                }
                Err(error) => scan_error = Some(error),
            }
        }
    });

    if let Some(error) = scan_error {
        return Err(error);
    }
    let arp = arp_table()?;
    let mut devices = Vec::with_capacity(results.len() + 1);
    devices.push(Device {
        ip: local_ip.to_string(),
        name: std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "Komputer ini".into()),
        is_this_device: true,
        active: true,
        ping_ms: None,
        mac: arp.get(&local_ip).cloned(),
    });
    for result in results {
        devices.push(Device {
            ip: result.ip.to_string(),
            name: if result.active {
                device_name(result.ip)
            } else {
                result.ip.to_string()
            },
            is_this_device: false,
            active: result.active,
            ping_ms: result.ping_ms,
            mac: arp.get(&result.ip).cloned(),
        });
    }
    devices.sort_by_key(|device| {
        device
            .ip
            .parse::<Ipv4Addr>()
            .map(u32::from)
            .unwrap_or_default()
    });
    let subnet = format!("{}/{}", Ipv4Addr::from(network), prefix);
    Ok(ScanResult {
        local_ip: local_ip.to_string(),
        subnet,
        devices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_scope_excludes_network_and_broadcast() {
        let local = Ipv4Addr::new(192, 168, 1, 25);
        let mask = Ipv4Addr::new(255, 255, 255, 0);
        let network = u32::from(local) & u32::from(mask);
        let broadcast = network | !u32::from(mask);
        let hosts: Vec<Ipv4Addr> = ((network + 1)..broadcast)
            .filter(|ip| *ip != u32::from(local))
            .map(Ipv4Addr::from)
            .collect();
        assert_eq!(hosts.len(), 253);
        assert!(!hosts.contains(&Ipv4Addr::new(192, 168, 1, 0)));
        assert!(!hosts.contains(&Ipv4Addr::new(192, 168, 1, 255)));
        assert!(!hosts.contains(&local));
    }

    #[test]
    fn arp_parser_extracts_ipv4_and_mac() {
        let text = "  192.168.1.5          aa-bb-cc-dd-ee-ff     dynamic\n  192.168.1.6          incomplete\n";
        let entries = parse_arp_table(text);
        assert_eq!(
            entries.get(&Ipv4Addr::new(192, 168, 1, 5)),
            Some(&"AA:BB:CC:DD:EE:FF".to_string())
        );
        assert!(!entries.contains_key(&Ipv4Addr::new(192, 168, 1, 6)));
    }
}
