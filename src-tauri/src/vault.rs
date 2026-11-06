//! Brankas catatan: AES-256-GCM. Kunci acak 256-bit disimpan di Credential Manager, isi berkas hanya berupa
//! data terenkripsi ("DIN1" + nonce 12 byte + ciphertext/tag). Berkas yang diubah/dirusak ditolak (GCM memverifikasi).
use crate::secrets;
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::{fs, path::Path};

const MAGIC: &[u8; 4] = b"DIN1";
const KEY_USER: &str = "vault-key";
pub const MAX_BYTES: usize = 1_000_000;

pub fn encrypt(key: &[u8], plain: &[u8]) -> Result<Vec<u8>, String> {
    let c = Aes256Gcm::new_from_slice(key).map_err(|_| "kunci enkripsi tidak valid".to_string())?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = c.encrypt(&nonce, plain).map_err(|_| "enkripsi gagal".to_string())?;
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 4 + 12 + 16 || &data[..4] != MAGIC {
        return Err("format berkas catatan tidak dikenal".into());
    }
    let c = Aes256Gcm::new_from_slice(key).map_err(|_| "kunci enkripsi tidak valid".to_string())?;
    c.decrypt(Nonce::from_slice(&data[4..16]), &data[16..])
        .map_err(|_| "catatan tidak bisa dibuka (berkas rusak atau kunci di Credential Manager tidak cocok)".to_string())
}

/// Ambil kunci dari Credential Manager; buat yang baru bila belum ada.
pub fn key() -> Result<Vec<u8>, String> {
    match secrets::get(KEY_USER)? {
        Some(b64) => STANDARD
            .decode(b64)
            .ok()
            .filter(|k| k.len() == 32)
            .ok_or_else(|| "kunci enkripsi di Credential Manager rusak".to_string()),
        None => {
            let k = Aes256Gcm::generate_key(OsRng);
            secrets::set(KEY_USER, &STANDARD.encode(k))?;
            Ok(k.to_vec())
        }
    }
}

pub fn read(path: &Path, key: &[u8]) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| format!("gagal membaca berkas catatan: {e}"))?;
    String::from_utf8(decrypt(key, &data)?).map_err(|_| "isi catatan bukan teks UTF-8".to_string())
}

/// Tulis secara atomik (berkas sementara lalu rename) supaya catatan lama tidak rusak bila terputus di tengah.
pub fn write(path: &Path, key: &[u8], text: &str) -> Result<(), String> {
    if text.len() > MAX_BYTES {
        return Err("catatan terlalu besar (maksimal 1 MB)".into());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("gagal membuat folder data: {e}"))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, encrypt(key, text.as_bytes())?).map_err(|e| format!("gagal menulis catatan: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("gagal menyimpan catatan: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enkripsi_dan_berkas() {
        let key = [7u8; 32];
        let teks = "Catatan rahasia ✓ — {\"tasks\":[]}";
        let enc = encrypt(&key, teks.as_bytes()).unwrap();
        assert!(!String::from_utf8_lossy(&enc).contains("rahasia"), "isi tidak boleh terbaca");
        assert_eq!(decrypt(&key, &enc).unwrap(), teks.as_bytes());
        // nonce acak: dua enkripsi dari teks yang sama berbeda
        assert_ne!(enc, encrypt(&key, teks.as_bytes()).unwrap());
        // kunci salah dan data diubah ditolak
        assert!(decrypt(&[8u8; 32], &enc).is_err());
        let mut rusak = enc.clone();
        *rusak.last_mut().unwrap() ^= 1;
        assert!(decrypt(&key, &rusak).is_err());
        assert!(decrypt(&key, b"bukan berkas").is_err());

        let p = std::env::temp_dir().join(format!("din-vault-test-{}.vault", std::process::id()));
        write(&p, &key, teks).unwrap();
        assert!(!fs::read(&p).unwrap().windows(7).any(|w| w == "rahasia".as_bytes()), "berkas di disk terenkripsi");
        assert_eq!(read(&p, &key).unwrap(), teks);
        assert!(read(&p, &[9u8; 32]).is_err());
        assert!(write(&p, &key, &"x".repeat(MAX_BYTES + 1)).is_err());
        let _ = fs::remove_file(&p);
    }
}
