use serde::Deserialize;
#[cfg(windows)]
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
};
use tauri::AppHandle;
#[cfg(windows)]
use tauri::{path::BaseDirectory, Manager};

#[derive(Clone, Copy, Default, Deserialize)]
pub struct Temperatures {
    pub cpu_temp: Option<f32>,
    pub gpu_temp: Option<f32>,
}

#[cfg(windows)]
pub struct SensorReader {
    child: Option<std::process::Child>,
    rx: Option<Receiver<Temperatures>>,
    last: Temperatures,
    disconnected: bool,
}

#[cfg(not(windows))]
pub struct SensorReader;

#[cfg(windows)]
impl SensorReader {
    pub fn new(app: &AppHandle) -> SensorReader {
        use std::{
            io::{BufRead, BufReader},
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };

        let exe = app
            .path()
            .resolve("HardwareSensor.exe", BaseDirectory::Resource)
            .ok()
            .filter(|p| p.is_file())
            .or_else(|| {
                option_env!("CARGO_MANIFEST_DIR")
                    .map(|p| {
                        PathBuf::from(p)
                            .join("resources")
                            .join("HardwareSensor.exe")
                    })
                    .filter(|p| p.is_file())
            });
        let Some(exe) = exe else {
            crate::applog::log("Sensor suhu tidak tersedia: HardwareSensor.exe tidak ditemukan.");
            return SensorReader {
                child: None,
                rx: None,
                last: Temperatures::default(),
                disconnected: false,
            };
        };

        let mut child = match Command::new(&exe)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x0800_0000)
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                crate::applog::log(&format!("Gagal menjalankan sensor suhu: {error}"));
                return SensorReader {
                    child: None,
                    rx: None,
                    last: Temperatures::default(),
                    disconnected: false,
                };
            }
        };
        let stdout = child
            .stdout
            .take()
            .expect("stdout sensor diarahkan ke pipe");
        let stderr = child
            .stderr
            .take()
            .expect("stderr sensor diarahkan ke pipe");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => match serde_json::from_str::<Temperatures>(&line) {
                        Ok(reading) => {
                            if tx.send(reading).is_err() {
                                break;
                            }
                        }
                        Err(error) => {
                            crate::applog::log(&format!("Data sensor suhu tidak valid: {error}"))
                        }
                    },
                    Err(error) => {
                        crate::applog::log(&format!("Gagal membaca sensor suhu: {error}"));
                        break;
                    }
                }
            }
        });
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                match line {
                    Ok(line) => crate::applog::log(&format!("Sensor suhu: {line}")),
                    Err(error) => {
                        crate::applog::log(&format!("Gagal membaca log sensor suhu: {error}"));
                        break;
                    }
                }
            }
        });
        SensorReader {
            child: Some(child),
            rx: Some(rx),
            last: Temperatures::default(),
            disconnected: false,
        }
    }

    pub fn sample(&mut self) -> Temperatures {
        let mut disconnected = false;
        if let Some(rx) = self.rx.as_ref() {
            loop {
                match rx.try_recv() {
                    Ok(reading) => self.last = reading,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if disconnected {
            self.last = Temperatures::default();
            self.rx = None;
            if !self.disconnected {
                crate::applog::log("Proses sensor suhu berhenti.");
                self.disconnected = true;
            }
        }
        self.last
    }
}

#[cfg(not(windows))]
impl SensorReader {
    pub fn new(_: &AppHandle) -> SensorReader {
        SensorReader
    }

    pub fn sample(&mut self) -> Temperatures {
        Temperatures::default()
    }
}

impl Drop for SensorReader {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_optional_temperature_values() {
        let sample: Temperatures =
            serde_json::from_str(r#"{"cpu_temp":47.5,"gpu_temp":null}"#).unwrap();
        assert_eq!(sample.cpu_temp, Some(47.5));
        assert_eq!(sample.gpu_temp, None);
    }
}
