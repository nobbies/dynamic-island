//! Pemeriksa alamat stream + pencari ikon stasiun (port dari detect.js versi Electron, dengan tambahan ikon).
use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures::future::join_all;
use regex::Regex;
use reqwest::{Client, Url};
use serde::Serialize;
use std::{collections::HashSet, time::Duration};

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const MAX_ICON: usize = 400_000;

#[derive(Serialize)]
pub struct Probe {
    pub kind: String, // "audio" | "html" | "unknown"
    #[serde(rename = "type")]
    pub ctype: String,
    pub icon: Option<String>, // data URL, bila server menyebutkan logonya
}

#[derive(Serialize)]
pub struct Found {
    pub url: String,
    #[serde(rename = "type")]
    pub ctype: String,
    pub icon: Option<String>, // ikon yang ditemukan di halaman web
}

struct Head {
    ok: bool,
    ctype: String,
    is_audio: bool,
    body: String,
    url: String,
    icy_logo: Option<String>,
    icy_url: Option<String>,
}

fn client() -> Result<Client, reqwest::Error> {
    Client::builder().user_agent(UA).build()
}

fn header(resp: &reqwest::Response, name: &str) -> Option<String> {
    resp.headers().get(name).and_then(|v| v.to_str().ok()).map(String::from)
}

// Ambil header (dan isi bila diminta). Isi stream audio tidak pernah dibaca: koneksi langsung ditutup.
async fn head(c: &Client, url: &str, ms: u64, want_text: bool) -> Result<Head, reqwest::Error> {
    let resp = c.get(url).timeout(Duration::from_millis(ms)).send().await?;
    let ctype = header(&resp, "content-type").unwrap_or_default().to_lowercase();
    let is_audio = ctype.starts_with("audio/")
        || ctype.starts_with("application/ogg")
        || resp.headers().contains_key("icy-name");
    let (ok, final_url) = (resp.status().is_success(), resp.url().to_string());
    let (icy_logo, icy_url) = (header(&resp, "icy-logo"), header(&resp, "icy-url"));
    let body = if want_text || ctype.starts_with("text/html") {
        resp.text().await.unwrap_or_default().chars().take(1_500_000).collect()
    } else {
        String::new() // resp dilepas di sini -> koneksi stream ditutup
    };
    Ok(Head { ok, ctype, is_audio, body, url: final_url, icy_logo, icy_url })
}

// ---------- Ikon ----------

fn attr(tag: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"(?i)\b{name}\s*=\s*["']([^"']*)["']"#)).ok()?;
    re.captures(tag).map(|m| m[1].to_string())
}

// Kandidat ikon dari HTML, urut dari yang paling mungkin berupa logo stasiun.
fn page_icons(html: &str, base: &Url) -> Vec<Url> {
    let mut out: Vec<(u8, Url)> = Vec::new();
    let mut push = |prio: u8, href: &str| {
        if let Ok(u) = base.join(href.trim()) {
            if matches!(u.scheme(), "http" | "https") {
                out.push((prio, u));
            }
        }
    };
    if let Ok(re) = Regex::new(r"(?is)<img\b[^>]*>") {
        for m in re.find_iter(html) {
            if let Some(src) = attr(m.as_str(), "src") {
                if src.to_lowercase().contains("logo") {
                    push(0, &src); // gambar yang namanya mengandung "logo"
                }
            }
        }
    }
    if let Ok(re) = Regex::new(r"(?is)<link\b[^>]*>") {
        for m in re.find_iter(html) {
            let t = m.as_str();
            if let (Some(rel), Some(href)) = (attr(t, "rel"), attr(t, "href")) {
                let rel = rel.to_lowercase();
                if rel.contains("apple-touch-icon") {
                    push(1, &href);
                } else if rel.contains("icon") {
                    push(2, &href);
                }
            }
        }
    }
    if let Ok(re) = Regex::new(r"(?is)<meta\b[^>]*>") {
        for m in re.find_iter(html) {
            let t = m.as_str();
            if attr(t, "property").is_some_and(|p| p.eq_ignore_ascii_case("og:image")) {
                if let Some(content) = attr(t, "content") {
                    push(3, &content);
                }
            }
        }
    }
    push(4, "/favicon.ico");
    out.sort_by_key(|(p, _)| *p);
    let mut seen = HashSet::new();
    out.into_iter().map(|(_, u)| u).filter(|u| seen.insert(u.to_string())).collect()
}

fn short(e: impl ToString) -> String {
    e.to_string().chars().take(60).collect()
}

// Kenali jenis gambar dari isinya (tidak percaya begitu saja pada Content-Type, yang sering salah di CDN).
fn sniff(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if b.starts_with(b"GIF8") {
        Some("image/gif")
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else if b.starts_with(&[0, 0, 1, 0]) {
        Some("image/x-icon")
    } else if String::from_utf8_lossy(&b[..b.len().min(512)]).to_lowercase().contains("<svg") {
        Some("image/svg+xml")
    } else {
        None
    }
}

async fn image_result(c: &Client, url: &str, referer: Option<&str>) -> Result<String, String> {
    let mut req = c
        .get(url)
        .timeout(Duration::from_millis(6000))
        .header("Accept", "image/avif,image/webp,image/png,image/*,*/*;q=0.8");
    if let Some(r) = referer {
        req = req.header("Referer", r); // sebagian CDN hanya melayani gambar jika dirujuk dari situs aslinya
    }
    let resp = req.send().await.map_err(|e| format!("tidak terhubung ({})", short(e)))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let declared = header(&resp, "content-type").unwrap_or_default().split(';').next().unwrap_or("").trim().to_lowercase();
    if resp.content_length().is_some_and(|n| n as usize > MAX_ICON) {
        return Err("gambar terlalu besar".into());
    }
    let bytes = resp.bytes().await.map_err(|e| format!("gagal membaca ({})", short(e)))?;
    if bytes.is_empty() || bytes.len() > MAX_ICON {
        return Err("gambar kosong atau terlalu besar".into());
    }
    let mime = match sniff(&bytes) {
        Some(m) => m.to_string(),
        None if declared.starts_with("image/") => declared,
        None => return Err(format!("bukan gambar ({})", if declared.is_empty() { "tanpa tipe" } else { &declared })),
    };
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(&bytes)))
}

async fn image_data_url(c: &Client, url: &str) -> Option<String> {
    image_result(c, url, None).await.ok()
}

async fn icon_from_html(c: &Client, html: &str, base: &Url) -> Option<String> {
    for u in page_icons(html, base).into_iter().take(6) {
        if let Some(d) = image_data_url(c, u.as_str()).await {
            return Some(d);
        }
    }
    None
}

// Server Icecast/Shoutcast kadang menyebut logo ("icy-logo") atau situsnya ("icy-url") di header stream.
async fn icon_for_stream(c: &Client, h: &Head) -> Option<String> {
    let base = Url::parse(&h.url).ok()?;
    if let Some(u) = h.icy_logo.as_ref().and_then(|l| base.join(l).ok()) {
        if let Some(d) = image_data_url(c, u.as_str()).await {
            return Some(d);
        }
    }
    let page = base.join(h.icy_url.as_ref()?).ok()?;
    let ph = head(c, page.as_str(), 6000, true).await.ok()?;
    icon_from_html(c, &ph.body, &Url::parse(&ph.url).ok()?).await
}

/// Unduh ikon dari URL; `referer` (opsional) = halaman asal stasiun. Mengembalikan data URL, atau alasan kegagalan.
pub async fn fetch_icon(url: &str, referer: Option<&str>) -> Result<String, String> {
    let c = client().map_err(|e| e.to_string())?;
    image_result(&c, url, referer).await
}

// ---------- Stream ----------

/// Apa isi alamat ini? Server Shoutcast lama (balasan "ICY 200 OK") tidak terbaca -> "unknown".
pub async fn probe(url: &str) -> Probe {
    let unknown = || Probe { kind: "unknown".into(), ctype: String::new(), icon: None };
    let Ok(c) = client() else { return unknown() };
    match head(&c, url, 6000, false).await {
        Ok(h) => {
            let icon = if h.is_audio { icon_for_stream(&c, &h).await } else { None };
            let kind = if h.is_audio { "audio" } else if h.ctype.starts_with("text/html") { "html" } else { "unknown" };
            Probe { kind: kind.into(), ctype: h.ctype, icon }
        }
        Err(_) => unknown(),
    }
}

/// Cari alamat audio di dalam halaman web (HTML + skrip situs yang sama), lalu buktikan dengan memeriksanya.
pub async fn detect(page_url: &str) -> Option<Found> {
    let c = client().ok()?;
    let base = Url::parse(page_url).ok()?;
    let page = head(&c, page_url, 8000, true).await.ok()?;

    let script_re = Regex::new(r#"(?i)<script[^>]+src=["']([^"']+)["']"#).ok()?;
    let srcs: Vec<String> = script_re
        .captures_iter(&page.body)
        .filter_map(|m| base.join(&m[1]).ok())
        .filter(|u| u.origin() == base.origin())
        .map(|u| u.to_string())
        .take(4)
        .collect();
    let scripts = join_all(srcs.iter().map(|u| head(&c, u, 6000, true))).await;

    let mut texts = vec![page.body.clone()];
    texts.extend(scripts.into_iter().filter_map(|r| r.ok()).map(|h| h.body));

    let url_re = Regex::new(r#"https?://[^\s"'<>\\)`]+"#).ok()?;
    let skip = Regex::new(r"(?i)\.(png|jpe?g|gif|svg|webp|ico|css|js|mjs|json|html?|woff2?|ttf|map)(\?|#|$)|facebook|instagram|tiktok|youtube|youtu\.be|wa\.me|whatsapp|twitter|google|gstatic|w3\.org|cloudflare|jsdelivr|cdnjs|unpkg|bootstrap").ok()?;

    let mut seen = HashSet::new();
    let mut cands: Vec<String> = Vec::new();
    for t in &texts {
        for m in url_re.find_iter(&t.replace("\\/", "/")) {
            let u = m.as_str().trim_end_matches(',').to_string();
            if !skip.is_match(&u) && seen.insert(u.clone()) {
                cands.push(u);
            }
        }
    }
    for p in ["/stream", "/live", "/radio", "/listen", "/stream.mp3", "/stream.aac", "/;", "/;stream.mp3"] {
        if let Ok(u) = base.join(p) {
            if seen.insert(u.to_string()) {
                cands.push(u.to_string());
            }
        }
    }
    cands.truncate(30);

    let results = join_all(cands.iter().map(|u| head(&c, u, 6000, false))).await;
    for r in results.into_iter().flatten() {
        if r.ok && r.is_audio {
            let page_base = Url::parse(&page.url).unwrap_or_else(|_| base.clone());
            let icon = icon_from_html(&c, &page.body, &page_base).await;
            return Some(Found { url: r.url, ctype: r.ctype, icon });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Dijalankan dengan: DETECT_TEST_BASE=http://127.0.0.1:8765 cargo test  (butuh server uji; dilewati jika tidak diset)
    #[test]
    fn probe_detect_dan_ikon() {
        let Ok(base) = std::env::var("DETECT_TEST_BASE") else { return };
        tauri::async_runtime::block_on(async {
            assert_eq!(probe(&format!("{base}/")).await.kind, "html");
            let p = probe(&format!("{base}/audio/phx.aac")).await;
            assert_eq!(p.kind, "audio");
            assert!(p.icon.is_none(), "stream tanpa header icy tidak punya ikon");
            let p = probe(&format!("{base}/audio/icy.aac")).await;
            assert!(p.icon.as_deref().is_some_and(|i| i.starts_with("data:image/png;base64,")), "icy-logo harus terbaca");
            assert_eq!(probe("http://127.0.0.1:1/").await.kind, "unknown");

            let f = detect(&format!("{base}/")).await.expect("stream harus ditemukan");
            assert!(f.url.ends_with("/audio/phx.aac"), "url: {}", f.url);
            assert!(f.ctype.starts_with("audio/"));
            assert!(f.icon.as_deref().is_some_and(|i| i.starts_with("data:image/png;base64,")), "logo halaman harus terbaca");
            assert!(detect(&format!("{base}/zzz")).await.is_none());

            assert!(fetch_icon(&format!("{base}/img/logo-phx.png"), None).await.is_ok());
            assert!(fetch_icon(&format!("{base}/img/octet.png"), None).await.is_ok(), "Content-Type salah tapi isinya PNG");
            assert!(fetch_icon(&format!("{base}/img/protected.png"), None).await.is_err(), "tanpa Referer harus ditolak");
            assert!(fetch_icon(&format!("{base}/img/protected.png"), Some("https://allowed.example/")).await.is_ok(), "dengan Referer harus lolos");
            let e = fetch_icon(&format!("{base}/zzz"), None).await.unwrap_err();
            assert!(e.contains("404"), "alasan: {e}");
            let e = fetch_icon(&format!("{base}/"), None).await.unwrap_err();
            assert!(e.contains("bukan gambar"), "alasan: {e}");
        });
    }
}
