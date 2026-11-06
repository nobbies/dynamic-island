//! Penyimpanan rahasia (API key, kunci enkripsi catatan) di Windows Credential Manager
//! (macOS: Keychain, Linux: keyutils) lewat crate `keyring`. Tidak pernah ditulis ke berkas teks biasa.
use keyring::Entry;

const SERVICE: &str = "com.example.dynamicislandradio";

fn entry(user: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, user).map_err(|e| format!("Credential Manager tidak tersedia: {e}"))
}

pub fn get(user: &str) -> Result<Option<String>, String> {
    match entry(user)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Gagal membaca Credential Manager: {e}")),
    }
}

pub fn set(user: &str, value: &str) -> Result<(), String> {
    entry(user)?
        .set_password(value)
        .map_err(|e| format!("Gagal menyimpan ke Credential Manager: {e}"))
}

pub fn delete(user: &str) -> Result<(), String> {
    match entry(user)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Gagal menghapus dari Credential Manager: {e}")),
    }
}
