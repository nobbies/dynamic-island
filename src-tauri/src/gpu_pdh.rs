//! Pengikat tipis ke PDH (Performance Data Helper) Windows: sumber data yang sama dengan Task Manager untuk
//! "GPU Engine\Utilization Percentage". Hanya lima fungsi, tanpa dependensi tambahan.
use std::{ffi::c_void, ptr};

type Status = i32;
type Handle = *mut c_void;

#[repr(C)]
struct FmtValue {
    cstatus: u32,
    value: f64, // anggota union "doubleValue" (kita selalu meminta PDH_FMT_DOUBLE)
}

#[repr(C)]
struct Item {
    name: *const u16,
    fmt: FmtValue,
}

#[cfg_attr(windows, link(name = "pdh"))]
extern "system" {
    fn PdhOpenQueryW(source: *const u16, user: usize, query: *mut Handle) -> Status;
    fn PdhAddEnglishCounterW(query: Handle, path: *const u16, user: usize, counter: *mut Handle) -> Status;
    fn PdhCollectQueryData(query: Handle) -> Status;
    fn PdhGetFormattedCounterArrayW(counter: Handle, format: u32, size: *mut u32, count: *mut u32, items: *mut Item) -> Status;
    fn PdhCloseQuery(query: Handle) -> Status;
}

const PDH_FMT_DOUBLE: u32 = 0x0000_0200;
const PDH_MORE_DATA: u32 = 0x8000_07D2;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn read_name(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    while n < 512 && *p.add(n) != 0 {
        n += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
}

pub struct Query {
    q: Handle,
    c: Handle,
}

impl Query {
    /// None bila penghitung GPU tidak ada (Windows lama atau tanpa driver WDDM 2.x).
    pub fn open() -> Option<Query> {
        Self::open_counter(r"\GPU Engine(*)\Utilization Percentage")
    }

    /// Windows ACPI thermal zones, in tenths of a Kelvin.
    pub fn open_thermal() -> Option<Query> {
        Self::open_counter(r"\Thermal Zone Information(*)\High Precision Temperature")
    }

    fn open_counter(path: &str) -> Option<Query> {
        unsafe {
            let mut q: Handle = ptr::null_mut();
            if PdhOpenQueryW(ptr::null(), 0, &mut q) != 0 {
                return None;
            }
            let mut c: Handle = ptr::null_mut();
            let path = wide(path);
            if PdhAddEnglishCounterW(q, path.as_ptr(), 0, &mut c) != 0 {
                PdhCloseQuery(q);
                return None;
            }
            PdhCollectQueryData(q); // pembacaan pertama = titik awal; nilai baru valid setelah pembacaan kedua
            Some(Query { q, c })
        }
    }

    /// Semua instance mesin GPU beserta persen pemakaiannya: [("pid_..._luid_..._engtype_3D", 12.5), ...]
    pub fn read(&mut self) -> Option<Vec<(String, f64)>> {
        unsafe {
            if PdhCollectQueryData(self.q) != 0 {
                return None;
            }
            let (mut size, mut count) = (0u32, 0u32);
            let st = PdhGetFormattedCounterArrayW(self.c, PDH_FMT_DOUBLE, &mut size, &mut count, ptr::null_mut());
            if st as u32 != PDH_MORE_DATA || size == 0 {
                return None;
            }
            let mut buf = vec![0u64; (size as usize + 7) / 8]; // u64 agar rata 8 byte (alignment struct Item)
            if PdhGetFormattedCounterArrayW(self.c, PDH_FMT_DOUBLE, &mut size, &mut count, buf.as_mut_ptr() as *mut Item) != 0 {
                return None;
            }
            let items = std::slice::from_raw_parts(buf.as_ptr() as *const Item, count as usize);
            Some(
                items
                    .iter()
                    .filter(|i| i.fmt.cstatus <= 1) // 0 = valid, 1 = data baru
                    .map(|i| (read_name(i.name), i.fmt.value))
                    .collect(),
            )
        }
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.q);
        }
    }
}
