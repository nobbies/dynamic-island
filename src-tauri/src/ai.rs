//! Klien asisten AI. Permintaan dikirim dari Rust (bukan dari halaman web), sehingga API key tidak pernah
//! menyentuh JavaScript. Mendukung dua format: Anthropic Messages API dan "kompatibel OpenAI" (OpenAI, OpenRouter,
//! Groq, Ollama, LM Studio, dst.).
use reqwest::{Client, Url};
use serde_json::{json, Value};
use std::time::Duration;

const SYSTEM: &str = "Kamu asisten mini di widget desktop. Jawab singkat, jelas, dan padat dalam bahasa yang sama dengan pengguna \
(utamakan Bahasa Indonesia). Usahakan di bawah 120 kata kecuali pengguna meminta lebih rinci.";
pub const MAX_PROMPT: usize = 20_000;
const NEED_KEY: &str = "API key belum diisi (⚙ Pengaturan → Asisten AI).";

fn is_local(url: &Url) -> bool {
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]" | "::1"))
}

fn trunc(s: &str) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > 160 { format!("{}…", s.chars().take(160).collect::<String>()) } else { s }
}

/// `key` TIDAK pernah dimasukkan ke pesan kesalahan.
pub async fn ask(provider: &str, endpoint: &str, model: &str, key: Option<&str>, prompt: &str) -> Result<String, String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Pertanyaan kosong.".into());
    }
    if prompt.chars().count() > MAX_PROMPT {
        return Err(format!("Teks terlalu panjang (maksimal {MAX_PROMPT} karakter)."));
    }
    if model.trim().is_empty() {
        return Err("Nama model belum diisi (⚙ Pengaturan → Asisten AI).".into());
    }
    let url = Url::parse(endpoint.trim()).map_err(|_| "Alamat endpoint tidak valid.".to_string())?;
    let local = is_local(&url);
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        return Err("Endpoint harus https:// (http:// hanya untuk localhost).".into());
    }
    let client = Client::builder()
        .user_agent("DynamicIslandRadio/0.2")
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;

    let req = match provider {
        "anthropic" => {
            let key = key.ok_or(NEED_KEY)?;
            let body = json!({ "model": model, "max_tokens": 600, "system": SYSTEM,
                               "messages": [{ "role": "user", "content": prompt }] });
            client.post(url).header("x-api-key", key).header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json").body(body.to_string())
        }
        "openai" => {
            let body = json!({ "model": model, "max_tokens": 600,
                               "messages": [{ "role": "system", "content": SYSTEM }, { "role": "user", "content": prompt }] });
            let r = client.post(url).header("content-type", "application/json").body(body.to_string());
            match key {
                Some(k) => r.bearer_auth(k),
                None if local => r, // server lokal (Ollama/LM Studio) biasanya tanpa kunci
                None => return Err(NEED_KEY.into()),
            }
        }
        _ => return Err("Penyedia AI tidak dikenal.".into()),
    };

    let resp = req.send().await.map_err(|e| {
        if e.is_timeout() { "Waktu habis menunggu balasan AI.".to_string() } else { "Tidak bisa terhubung ke server AI. Periksa internet dan alamat endpoint.".to_string() }
    })?;
    let status = resp.status();
    let text = resp.text().await.map_err(|_| "Gagal membaca balasan server AI.".to_string())?;
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);

    if !status.is_success() {
        let msg = v["error"]["message"].as_str().or_else(|| v["error"].as_str()).or_else(|| v["message"].as_str()).unwrap_or(&text);
        return Err(match status.as_u16() {
            401 | 403 => "API key ditolak oleh server (HTTP 401/403). Periksa kunci di ⚙ Pengaturan → Asisten AI.".to_string(),
            429 => "Batas permintaan atau kuota terlampaui (HTTP 429). Coba lagi nanti.".to_string(),
            c => format!("Server AI menjawab HTTP {c}: {}", trunc(msg)),
        });
    }
    let out = if provider == "anthropic" {
        v["content"].as_array().map(|a| {
            a.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("")
        })
    } else {
        v["choices"][0]["message"]["content"].as_str().map(String::from)
    };
    match out.map(|s| s.trim().to_string()) {
        Some(s) if !s.is_empty() => Ok(s),
        _ => Err("Balasan AI kosong atau formatnya tidak dikenali (periksa penyedia dan endpoint).".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Dijalankan dengan: AI_TEST_BASE=http://127.0.0.1:8765 cargo test (butuh server uji; dilewati jika tidak diset)
    #[test]
    fn format_anthropic_dan_openai() {
        let Ok(base) = std::env::var("AI_TEST_BASE") else { return };
        tauri::async_runtime::block_on(async {
            let a = format!("{base}/anthropic/v1/messages");
            assert_eq!(ask("anthropic", &a, "m", Some("k1"), "Halo").await.unwrap(), "Halo dunia");
            let e = ask("anthropic", &a, "m", Some("salah"), "Halo").await.unwrap_err();
            assert!(e.contains("ditolak") && !e.contains("salah"), "pesan: {e}");
            assert!(ask("anthropic", &a, "m", None, "Halo").await.unwrap_err().contains("API key belum"));

            let o = format!("{base}/openai/v1/chat/completions");
            assert_eq!(ask("openai", &o, "m", Some("k2"), "Halo").await.unwrap(), "jawaban");
            assert!(ask("openai", &o, "m", None, "Halo").await.unwrap_err().contains("ditolak"), "localhost boleh tanpa kunci");

            assert!(ask("anthropic", &format!("{base}/rate"), "m", Some("k1"), "x").await.unwrap_err().contains("429"));
            assert!(ask("anthropic", &format!("{base}/empty"), "m", Some("k1"), "x").await.unwrap_err().contains("kosong"));
            assert!(ask("anthropic", &format!("{base}/boom"), "m", Some("k1"), "x").await.unwrap_err().contains("HTTP 500"));
            assert!(ask("anthropic", &a, "", Some("k1"), "x").await.unwrap_err().contains("model"));
            assert!(ask("anthropic", &a, "m", Some("k1"), "  ").await.unwrap_err().contains("kosong"));
            assert!(ask("anthropic", &a, "m", Some("k1"), &"a".repeat(MAX_PROMPT + 1)).await.unwrap_err().contains("panjang"));
            assert!(ask("anthropic", "http://example.com/v1", "m", Some("k1"), "x").await.unwrap_err().contains("https"));
            assert!(ask("anthropic", "bukan url", "m", Some("k1"), "x").await.unwrap_err().contains("tidak valid"));
            assert!(ask("lain", &a, "m", Some("k1"), "x").await.unwrap_err().contains("tidak dikenal"));
        });
    }
}
