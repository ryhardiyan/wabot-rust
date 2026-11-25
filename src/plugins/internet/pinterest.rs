use crate::plugins::Context;
use anyhow::{bail, Context as _, Result};
use hmac13::{Hmac, KeyInit, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use std::fmt::Write as _;

crate::plugins::plugin!(&["pinterest", "pin"], "internet", execute);

const BASE_API: &str = "https://api.pinterest.com/v3";
const DEFAULT_CLIENT_ID: &str = "1431602";
const DEFAULT_CLIENT_SECRET: &str = "492124fd20e80e0f678f7a03344875f9b6234e2b";
const API_UA: &str = "Pinterest for Android/14.39.0 (Linux; Android 14; SM-G998B)";
const WEB_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
const RESULT_LIMIT: usize = 5;

type HmacSha256 = Hmac<Sha256>;

/// Percent-encoding ala `encodeURIComponent` + penggantian `!'()*`, yaitu
/// hanya `A-Z a-z 0-9 - . _ ~` yang dibiarkan.
fn pct(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &byte in input.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

fn hmac_hex(secret: &str, message: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC menerima key apa pun");
    mac.update(message.as_bytes());
    let digest = mac.finalize().into_bytes();
    let mut out = String::with_capacity(digest.len() * 2);
    for &byte in digest.iter() {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn s(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn first_str(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| s(value, key))
}

fn count(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn verified(value: &Value) -> bool {
    match value {
        Value::Bool(flag) => *flag,
        Value::Object(object) => object
            .get("verified")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        _ => false,
    }
}

fn image_url(pin: &Value) -> Option<String> {
    let signature = s(pin, "image_signature");
    let mut url = first_str(pin, &["image_large_url"]);
    if let Some(orig) = pin
        .get("images")
        .and_then(|images| images.get("orig"))
        .and_then(|orig| orig.get("url"))
        .and_then(Value::as_str)
    {
        url = Some(orig.to_string());
    }
    if let (Some(signature), Some(current)) = (signature.as_deref(), url.as_deref()) {
        if signature.len() >= 6 && !current.contains("/originals/") {
            return Some(format!(
                "https://i.pinimg.com/originals/{}/{}/{}/{}.jpg",
                &signature[..2],
                &signature[2..4],
                &signature[4..6],
                signature
            ));
        }
    }
    url
}

fn video_url(pin: &Value) -> Option<String> {
    let id = pin
        .get("videos")
        .and_then(|videos| videos.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            let pages = pin.get("story_pin_data")?.get("pages")?.as_array()?;
            for page in pages {
                for block in page.get("blocks")?.as_array()? {
                    if let Some(id) = block.get("video")?.get("id")?.as_str() {
                        return Some(id.to_string());
                    }
                }
            }
            None
        })?;
    if id.len() < 6 {
        return None;
    }
    Some(format!(
        "https://v1.pinimg.com/videos/mc/720p/{}/{}/{}/{}.mp4",
        &id[..2],
        &id[2..4],
        &id[4..6],
        id
    ))
}

fn pin_id_from(input: &str) -> Option<String> {
    let index = input.find("pin/")?;
    let rest = &input[index + 4..];
    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!id.is_empty()).then_some(id)
}

async fn resolve_pin_id(http: &reqwest::Client, input: &str) -> String {
    let value = input.trim();
    if !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) {
        return value.to_string();
    }
    if let Some(id) = pin_id_from(value) {
        return id;
    }
    if value.contains("pin.it") {
        if let Ok(response) = http
            .get(value)
            .header(reqwest::header::USER_AGENT, WEB_UA)
            .send()
            .await
        {
            if let Some(id) = pin_id_from(response.url().as_str()) {
                return id;
            }
        }
    }
    value.to_string()
}

fn keyword_terms(text: &str, limit: usize) -> String {
    const STOP: &[&str] = &[
        "that", "with", "this", "from", "have", "there", "what", "some", "where", "photo",
        "picture", "image", "top",
    ];
    let mut words = Vec::new();
    for word in text
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| word.len() > 2 && !STOP.contains(word))
    {
        words.push(word.to_string());
        if words.len() == limit {
            break;
        }
    }
    words.join(" ")
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

struct Pinterest<'a> {
    http: &'a reqwest::Client,
    client_id: String,
    client_secret: String,
}

impl<'a> Pinterest<'a> {
    fn new(ctx: &'a Context) -> Self {
        let env = &ctx.app.config.env;
        let setting = |key: &str, default: &str| {
            env.get(key)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .unwrap_or(default)
                .to_string()
        };
        Self {
            http: &ctx.app.http,
            client_id: setting("PINTEREST_CLIENT_ID", DEFAULT_CLIENT_ID),
            client_secret: setting("PINTEREST_CLIENT_SECRET", DEFAULT_CLIENT_SECRET),
        }
    }

    async fn get(&self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        let url = if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else if path.starts_with('/') {
            format!("{BASE_API}{path}")
        } else {
            format!("{BASE_API}/{path}")
        };
        let mut ordered: Vec<(String, String)> = params
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        ordered.push(("client_id".to_string(), self.client_id.clone()));
        ordered.push((
            "timestamp".to_string(),
            chrono::Utc::now().timestamp().to_string(),
        ));

        // Basis signature memakai kunci terurut, sedangkan query dikirim sesuai
        // urutan JS agar identik dengan implementasi referensi.
        let mut sorted = ordered.clone();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let param_string = sorted
            .iter()
            .map(|(key, value)| format!("{key}={}", pct(value)))
            .collect::<Vec<_>>()
            .join("&");
        let base = format!("GET&{}&{}", pct(&url), param_string);
        let signature = hmac_hex(&self.client_secret, &base);
        ordered.push(("oauth_signature".to_string(), signature));

        let query = ordered
            .iter()
            .map(|(key, value)| format!("{}={}", pct(key), pct(value)))
            .collect::<Vec<_>>()
            .join("&");
        let response = self
            .http
            .get(format!("{url}?{query}"))
            .header(reqwest::header::USER_AGENT, API_UA)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .context("Gagal menghubungi Pinterest")?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .context("Gagal membaca respons Pinterest")?;
        anyhow::ensure!(
            body.len() <= 16 * 1024 * 1024,
            "Respons Pinterest terlalu besar"
        );
        if !status.is_success() {
            let message = serde_json::from_slice::<Value>(&body)
                .ok()
                .and_then(|json| {
                    json.get("message")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_else(|| format!("Pinterest API status {}", status.as_u16()));
            bail!("{message}");
        }
        serde_json::from_slice(&body).context("Respons Pinterest bukan JSON")
    }

    async fn search(&self, query: &str, bookmark: Option<&str>, limit: usize) -> Result<Value> {
        anyhow::ensure!(!query.trim().is_empty(), "Kata kunci pencarian kosong");
        let mut params: Vec<(&str, &str)> = vec![("query", query.trim())];
        if let Some(bookmark) = bookmark {
            params.push(("bookmark", bookmark));
        }
        let response = self.get("/search/pins/", &params).await?;
        let items = response
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let results: Vec<Value> = items
            .iter()
            .take(limit.max(1))
            .map(|pin| {
                let id = s(pin, "id").unwrap_or_default();
                json!({
                    "id": id,
                    "title": first_str(pin, &["title", "grid_title", "closeup_unified_title"]),
                    "description": first_str(pin, &["description", "grid_description"]),
                    "link": s(pin, "shareable_url")
                        .unwrap_or_else(|| format!("https://www.pinterest.com/pin/{id}/")),
                    "image": image_url(pin),
                    "large": s(pin, "image_large_url"),
                    "medium": s(pin, "image_medium_url"),
                    "thumbnail": s(pin, "image_square_url"),
                    "repins": count(pin, "repin_count"),
                    "likes": count(pin, "like_count"),
                })
            })
            .collect();
        Ok(json!({
            "query": query,
            "count": results.len(),
            "bookmark": s(&response, "bookmark"),
            "results": results,
        }))
    }

    async fn download(&self, input: &str) -> Result<PinMedia> {
        let id = resolve_pin_id(self.http, input).await;
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            bail!("Pinterest tidak menyediakan detail untuk ID format baru (e1_...). Gunakan link pin numerik atau ambil via -stalk.");
        }
        let response = self.get(&format!("/pins/{id}/"), &[]).await?;
        let pin = response
            .get("data")
            .context("Pin Pinterest tidak ditemukan")?;
        Ok(PinMedia {
            id: s(pin, "id").unwrap_or(id),
            title: first_str(pin, &["title", "closeup_unified_title"]),
            description: first_str(pin, &["description", "closeup_unified_description"]),
            share_url: s(pin, "shareable_url").unwrap_or_else(|| {
                format!(
                    "https://www.pinterest.com/pin/{}",
                    s(pin, "id").unwrap_or_default()
                )
            }),
            image: image_url(pin),
            video: video_url(pin),
        })
    }

    async fn stalk(&self, username: &str) -> Result<Value> {
        let username = username.trim().trim_start_matches('@');
        anyhow::ensure!(!username.is_empty(), "Username kosong");
        let profile = self.get(&format!("/users/{username}/"), &[]).await?;
        let user = profile
            .get("data")
            .context("User Pinterest tidak ditemukan")?;

        let mut boards = Vec::new();
        if let Ok(response) = self.get(&format!("/users/{username}/boards/"), &[]).await {
            if let Some(items) = response.get("data").and_then(Value::as_array) {
                boards = items
                    .iter()
                    .take(10)
                    .map(|board| {
                        json!({
                            "name": s(board, "name"),
                            "pins": count(board, "pin_count"),
                            "url": s(board, "url")
                                .map(|url| format!("https://www.pinterest.com{url}")),
                        })
                    })
                    .collect();
            }
        }

        let mut pins = Vec::new();
        if let Ok(response) = self.get(&format!("/users/{username}/pins/"), &[]).await {
            if let Some(items) = response.get("data").and_then(Value::as_array) {
                pins = items
                    .iter()
                    .take(10)
                    .map(|pin| {
                        let id = s(pin, "id").unwrap_or_default();
                        json!({
                            "id": id,
                            "title": first_str(pin, &["title", "grid_title"]),
                            "image": image_url(pin),
                            "link": s(pin, "shareable_url")
                                .unwrap_or_else(|| format!("https://www.pinterest.com/pin/{id}/")),
                        })
                    })
                    .collect();
            }
        }

        Ok(json!({
            "user": {
                "id": s(user, "id"),
                "username": s(user, "username"),
                "full_name": s(user, "full_name"),
                "bio": s(user, "about"),
                "avatar": first_str(user, &["image_xlarge_url", "image_large_url"]),
                "website": s(user, "website_url"),
                "verified": user.get("is_verified_merchant").map(verified).unwrap_or(false)
                    || user.get("verified_identity").map(verified).unwrap_or(false),
                "followers": count(user, "follower_count"),
                "following": count(user, "following_count"),
                "pins": count(user, "pin_count"),
                "boards": count(user, "board_count"),
            },
            "boards": boards,
            "pins": pins,
        }))
    }

    async fn lens(&self, input: &str, keywords: Option<&str>, limit: usize) -> Result<Value> {
        let input = input.trim();
        anyhow::ensure!(!input.is_empty(), "Input Pinterest Lens kosong");
        let keyword = keywords.map(str::trim).filter(|value| !value.is_empty());
        let is_pin = input.contains("pin.it")
            || input.contains("pinterest.com/pin/")
            || input.starts_with("e1_")
            || (!input.is_empty() && input.chars().all(|c| c.is_ascii_digit()));
        if is_pin {
            let id = resolve_pin_id(self.http, input).await;
            let numeric = !id.is_empty() && id.chars().all(|c| c.is_ascii_digit());
            if !numeric {
                if let Some(keyword) = keyword {
                    let response = self.search(keyword, None, limit).await?;
                    return Ok(json!({
                        "mode": "keyword_fallback",
                        "sourcePinId": id,
                        "detectedVisualTerms": keyword,
                        "count": response["count"].clone(),
                        "results": response["results"].clone(),
                    }));
                }
                bail!("Pin memakai ID format baru (e1_...) yang tidak bisa dibaca API. Sertakan kata kunci, mis. /pinterest -lens anime wallpaper, atau gunakan pin numerik.");
            }
            let response = self.get(&format!("/pins/{id}/"), &[]).await?;
            let pin = response
                .get("data")
                .context("Pin Pinterest tidak ditemukan")?;
            let raw = [
                s(pin, "auto_alt_text"),
                s(pin, "seo_alt_text"),
                s(pin, "title"),
                s(pin, "closeup_unified_title"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
            let detected = keyword_terms(&raw, 5);
            let query = if detected.is_empty() {
                first_str(pin, &["title"]).unwrap_or_else(|| "aesthetic".to_string())
            } else {
                detected.clone()
            };
            let response = self.search(&query, None, limit).await?;
            return Ok(json!({
                "mode": "visual_semantic",
                "sourcePinId": id,
                "detectedVisualTerms": detected,
                "sourceImage": s(pin, "image_large_url"),
                "count": response["count"].clone(),
                "results": response["results"].clone(),
            }));
        }
        let query = if let Some(keyword) = keyword {
            keyword.to_string()
        } else if input.contains("pinimg.com") {
            input
                .rsplit('/')
                .next()
                .and_then(|name| name.split('.').next())
                .filter(|name| name.len() >= 10)
                .map(|name| name[..8].to_string())
                .unwrap_or_else(|| "aesthetic wallpaper".to_string())
        } else {
            "aesthetic wallpaper".to_string()
        };
        let response = self.search(&query, None, limit).await?;
        Ok(json!({
            "mode": "visual_image_search",
            "sourceImageUrl": input,
            "keywords": query,
            "count": response["count"].clone(),
            "results": response["results"].clone(),
        }))
    }
}

struct PinMedia {
    #[allow(dead_code)] // Bagian dari kontrak data Pinterest.
    id: String,
    title: Option<String>,
    description: Option<String>,
    share_url: String,
    image: Option<String>,
    video: Option<String>,
}

fn usage() -> &'static str {
    "📌 *Pinterest*\n\
     • /pinterest -search <kata kunci>\n\
     • /pinterest -download <link pin numerik>\n\
     • /pinterest -stalk <username>\n\
     • /pinterest -lens <link/pin> | balas/kirim foto (opsional: kata kunci)\n\n\
     Catatan: ID pin format baru (e1_...) tidak bisa di-download; gunakan pin numerik atau hasil -stalk."
}

fn format_results(header: &str, response: &Value) -> String {
    let mut out = format!("{header}\n");
    match response.get("results").and_then(Value::as_array) {
        Some(items) if !items.is_empty() => {
            for (index, item) in items.iter().enumerate() {
                out.push_str(&format!(
                    "{}. {}\n   {}\n",
                    index + 1,
                    item.get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("(tanpa judul)"),
                    item.get("link").and_then(Value::as_str).unwrap_or("-")
                ));
            }
        }
        _ => out.push_str("Tidak ada hasil.\n"),
    }
    out
}

fn format_profile(profile: &Value) -> String {
    let user = &profile["user"];
    let mut out = String::from("📌 *PINTEREST PROFILE*\n");
    out.push_str(&format!(
        "• Nama      : {}\n",
        user["full_name"].as_str().unwrap_or("-")
    ));
    out.push_str(&format!(
        "• Username  : @{}\n",
        user["username"].as_str().unwrap_or("-")
    ));
    if user["verified"].as_bool() == Some(true) {
        out.push_str("• Status    : ✅ Verified\n");
    }
    out.push_str(&format!(
        "• Followers : {}\n",
        user["followers"].as_u64().unwrap_or(0)
    ));
    out.push_str(&format!(
        "• Following : {}\n",
        user["following"].as_u64().unwrap_or(0)
    ));
    out.push_str(&format!(
        "• Pins      : {}\n",
        user["pins"].as_u64().unwrap_or(0)
    ));
    out.push_str(&format!(
        "• Boards    : {}\n",
        user["boards"].as_u64().unwrap_or(0)
    ));
    if let Some(bio) = user["bio"].as_str().filter(|bio| !bio.is_empty()) {
        out.push_str(&format!("• Bio       : {}\n", truncate(bio, 200)));
    }
    if let Some(website) = user["website"].as_str().filter(|url| !url.is_empty()) {
        out.push_str(&format!("• Website   : {website}\n"));
    }
    if let Some(boards) = profile["boards"]
        .as_array()
        .filter(|items| !items.is_empty())
    {
        out.push_str("\n*BOARD*\n");
        for (index, board) in boards.iter().enumerate() {
            out.push_str(&format!(
                "{}. {} ({} pin)\n   {}\n",
                index + 1,
                board["name"].as_str().unwrap_or("-"),
                board["pins"].as_u64().unwrap_or(0),
                board["url"].as_str().unwrap_or("-")
            ));
        }
    }
    if let Some(pins) = profile["pins"].as_array().filter(|items| !items.is_empty()) {
        out.push_str("\n*PIN TERBARU*\n");
        for (index, pin) in pins.iter().enumerate() {
            out.push_str(&format!(
                "{}. {}\n   {}\n",
                index + 1,
                pin["title"].as_str().unwrap_or("(tanpa judul)"),
                pin["link"].as_str().unwrap_or("-")
            ));
        }
    }
    out
}

async fn fetch_media(ctx: &Context, url: &str) -> Result<Vec<u8>> {
    let response = ctx
        .app
        .http
        .get(url)
        .header(reqwest::header::USER_AGENT, WEB_UA)
        .send()
        .await
        .context("Gagal mengunduh media Pinterest")?;
    let status = response.status();
    anyhow::ensure!(
        status.is_success(),
        "Pinterest media status {}",
        status.as_u16()
    );
    crate::bot_lib::downloader::bounded(response, ctx.app.config.max_media_bytes)
        .await
        .context("Gagal membaca media Pinterest")
}

async fn send_download(ctx: &Context, pin: &PinMedia) -> Result<()> {
    let caption = match (&pin.title, &pin.description) {
        (Some(title), _) => format!("{title}\n{}", pin.share_url),
        (None, Some(description)) => {
            format!("{}\n{}", truncate(description, 200), pin.share_url)
        }
        (None, None) => pin.share_url.clone(),
    };
    if let Some(video) = pin.video.as_deref() {
        if let Ok(bytes) = fetch_media(ctx, video).await {
            if !bytes.is_empty() {
                return ctx
                    .m
                    .send_media(bytes, "video/mp4", "pinterest.mp4", &caption)
                    .await;
            }
        }
    }
    let image = pin
        .image
        .as_deref()
        .context("Pin tidak memiliki gambar atau video")?;
    let bytes = fetch_media(ctx, image).await?;
    ctx.m
        .send_media(bytes, "image/jpeg", "pinterest.jpg", &caption)
        .await
}

async fn lens_command(ctx: &Context, pin: &Pinterest<'_>, rest: &str) -> Result<()> {
    let mime = crate::bot_lib::core::message::mime(ctx.m.media());
    if mime.starts_with("image/") {
        // Tidak ada API visual-search tanpa token; teks setelah -lens dipakai
        // sebagai kata kunci (default: aesthetic wallpaper).
        let query = if rest.is_empty() {
            "aesthetic wallpaper"
        } else {
            rest
        };
        let response = pin.search(query, None, RESULT_LIMIT).await?;
        return ctx
            .reply(format_results(
                &format!("🧪 *Pinterest Lens* (berbasis kata kunci) — {query}"),
                &response,
            ))
            .await;
    }

    let is_link = rest.starts_with("http://") || rest.starts_with("https://");
    let is_pin = rest.contains("pinterest.com/pin/")
        || rest.contains("pin.it")
        || rest.starts_with("e1_")
        || (!rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()));

    if is_link || is_pin {
        return match pin.lens(rest, None, RESULT_LIMIT).await {
            Ok(response) => {
                let mode = response["mode"].as_str().unwrap_or("lens");
                let terms = response["detectedVisualTerms"].as_str().unwrap_or("-");
                ctx.reply(format_results(
                    &format!("🧪 *Pinterest Lens* ({mode}) — {terms}"),
                    &response,
                ))
                .await
            }
            Err(error) => ctx.reply(error.to_string()).await,
        };
    }

    if !rest.is_empty() {
        let response = pin.search(rest, None, RESULT_LIMIT).await?;
        return ctx
            .reply(format_results(
                &format!("🧪 *Pinterest Lens* — kata kunci: {rest}"),
                &response,
            ))
            .await;
    }

    ctx.reply(
        "Balas/kirim foto, atau berikan link pin/URL gambar.\n\nContoh: /pinterest -lens anime wallpaper",
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc4231_vector() {
        // RFC 4231 test case 2.
        assert_eq!(
            hmac_hex("Jefe", "what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn percent_encoding_matches_reference() {
        assert_eq!(pct("a b/c!d"), "a%20b%2Fc%21d");
        assert_eq!(pct("abc-._~"), "abc-._~");
    }

    #[test]
    fn pin_ids_include_new_style_and_numeric() {
        assert_eq!(
            pin_id_from("https://www.pinterest.com/pin/123456/").as_deref(),
            Some("123456")
        );
        assert_eq!(
            pin_id_from("https://www.pinterest.com/pin/e1_ab-cd/").as_deref(),
            Some("e1_ab-cd")
        );
        assert_eq!(pin_id_from("https://pin.it/abc"), None);
    }

    #[test]
    fn keyword_terms_filters_stopwords_and_limits() {
        assert_eq!(
            keyword_terms("Meet our latest Fair Isle picture with style", 3),
            "meet our latest"
        );
    }
}

async fn execute(ctx: &Context) -> Result<()> {
    let sub = ctx.arg(0).trim_start_matches('-').to_ascii_lowercase();
    let rest = ctx
        .args
        .iter()
        .skip(1)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let rest = rest.trim();
    let pin = Pinterest::new(ctx);
    match sub.as_str() {
        "" | "help" | "menu" => ctx.reply(usage()).await,
        "search" | "s" => {
            if rest.is_empty() {
                return ctx.reply("Contoh: /pinterest -search history mlbb").await;
            }
            let response = pin.search(rest, None, RESULT_LIMIT).await?;
            ctx.reply(format_results(
                &format!("🔎 *Pinterest Search* — {rest}"),
                &response,
            ))
            .await
        }
        "download" | "dl" | "d" => {
            if rest.is_empty() {
                return ctx
                    .reply("Contoh: /pinterest -download https://www.pinterest.com/pin/123.../")
                    .await;
            }
            match pin.download(rest).await {
                Ok(media) => send_download(ctx, &media).await,
                Err(error) => ctx.reply(error.to_string()).await,
            }
        }
        "stalk" | "user" | "u" => {
            if rest.is_empty() {
                return ctx.reply("Contoh: /pinterest -stalk username").await;
            }
            let profile = pin.stalk(rest).await?;
            ctx.reply(format_profile(&profile)).await
        }
        "lens" => lens_command(ctx, &pin, rest).await,
        other => {
            ctx.reply(format!(
                "Subcommand `-{other}` tidak dikenal.\n\n{}",
                usage()
            ))
            .await
        }
    }
}
