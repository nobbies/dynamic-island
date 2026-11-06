//! Utilisasi GPU (%), dibaca seperti Task Manager: jumlah pemakaian per mesin (3D, Copy, Video Decode, ...) dari semua proses,
//! lalu diambil mesin yang paling sibuk. Hanya Windows (PDH); di sistem lain hasilnya None.
#[cfg(any(windows, force_pdh))]
#[path = "gpu_pdh.rs"]
mod pdh;
use std::collections::HashMap;

/// `items` = (nama instance PDH, persen). Contoh nama: "pid_4132_luid_0x00000000_0x0000AE1C_phys_0_eng_0_engtype_3D".
#[cfg_attr(not(any(windows, force_pdh)), allow(dead_code))]
pub fn busiest(items: &[(String, f64)]) -> f32 {
    let mut groups: HashMap<(String, String), f64> = HashMap::new();
    for (name, v) in items {
        let luid = name.split("_luid_").nth(1).and_then(|s| s.split("_phys").next()).unwrap_or("").to_string();
        let eng = name.split("_engtype_").nth(1).unwrap_or("").to_string();
        *groups.entry((luid, eng)).or_insert(0.0) += v;
    }
    groups.values().cloned().fold(0.0, f64::max).clamp(0.0, 100.0) as f32
}

#[cfg_attr(not(any(windows, force_pdh)), allow(dead_code))]
fn acpi_celsius(value: f64) -> Option<f32> {
    let celsius = (value / 10.0 - 273.15) as f32;
    (celsius.is_finite() && (-20.0..=150.0).contains(&celsius)).then_some(celsius)
}

pub struct Gpu {
    #[cfg(any(windows, force_pdh))]
    q: Option<pdh::Query>,
    #[cfg(any(windows, force_pdh))]
    age: u32,
    last: Option<f32>,
}

impl Gpu {
    pub fn new() -> Gpu {
        Gpu {
            #[cfg(any(windows, force_pdh))]
            q: pdh::Query::open(),
            #[cfg(any(windows, force_pdh))]
            age: 0,
            last: None,
        }
    }

    pub fn sample(&mut self) -> Option<f32> {
        #[cfg(any(windows, force_pdh))]
        {
            self.age += 1;
            // Buka ulang tiap 30 detik agar proses yang baru berjalan ikut terbaca. Pembacaan berikutnya langsung valid.
            if self.age >= 30 {
                self.age = 0;
                if let Some(q) = pdh::Query::open() {
                    self.q = Some(q);
                    return self.last;
                }
            }
            if let Some(items) = self.q.as_mut().and_then(|q| q.read()) {
                self.last = Some(busiest(&items));
            }
        }
        self.last
    }
}

pub struct Thermal {
    #[cfg(any(windows, force_pdh))]
    q: Option<pdh::Query>,
}

impl Thermal {
    pub fn new() -> Thermal {
        Thermal {
            #[cfg(any(windows, force_pdh))]
            q: pdh::Query::open_thermal(),
        }
    }

    pub fn cpu_sample(&mut self) -> Option<f32> {
        #[cfg(any(windows, force_pdh))]
        {
            return self.q.as_mut()?.read()?.iter()
                .filter_map(|(_, value)| acpi_celsius(*value))
                .max_by(|a, b| a.total_cmp(b));
        }
        #[allow(unreachable_code)]
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(n: &str, v: f64) -> (String, f64) {
        (n.to_string(), v)
    }

    #[test]
    fn converts_acpi_temperature_to_celsius() {
        assert!((acpi_celsius(3010.0).unwrap() - 27.85).abs() < 0.01);
        assert!(acpi_celsius(f64::NAN).is_none());
        assert!(acpi_celsius(0.0).is_none());
    }

    #[test]
    fn mesin_tersibuk_seperti_task_manager() {
        let a = "luid_0x00000000_0x0000AE1C_phys_0";
        let items = vec![
            s(&format!("pid_100_{a}_eng_0_engtype_3D"), 30.0),
            s(&format!("pid_200_{a}_eng_0_engtype_3D"), 25.0), // 3D total = 55
            s(&format!("pid_100_{a}_eng_3_engtype_VideoDecode"), 70.0), // mesin lain lebih sibuk
            s("pid_300_luid_0x00000000_0x0000BEEF_phys_0_eng_0_engtype_3D", 10.0), // GPU lain
        ];
        assert_eq!(busiest(&items), 70.0);
        assert_eq!(busiest(&items[..2]), 55.0, "pemakaian proses berbeda pada mesin yang sama dijumlahkan");
        assert_eq!(busiest(&[]), 0.0);
        assert_eq!(busiest(&[s("x_engtype_3D", 250.0)]), 100.0, "dibatasi 100%");
        assert_eq!(busiest(&[s("tanpa-pola", 12.0)]), 12.0, "nama tak dikenal tetap dihitung");
    }
}
