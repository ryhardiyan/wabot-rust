use crate::{
    bot_lib::{downloader, sticker},
    plugins::Context,
};
use anyhow::{Context as _, Result};
use reqwest::multipart::{Form, Part};
crate::plugins::plugin!(
    &["whatmusic", "findsong", "judul", "judullagu", "cek-lagu"],
    "tools",
    execute
);
async fn execute(ctx: &Context) -> Result<()> {
    let bytes = ctx.m.download(ctx.app.config.max_media_bytes).await?;
    let bytes = sticker::transcode(
        bytes,
        &[
            "-t",
            "20",
            "-vn",
            "-c:a",
            "libmp3lame",
            "-b:a",
            "128k",
            "-f",
            "mp3",
        ],
    )
    .await?;
    match tokio::time::timeout(std::time::Duration::from_secs(90), doreso(ctx, &bytes)).await {
        Ok(Ok(music)) => return send_song(ctx, &music).await,
        _ => tracing::debug!("Doreso gagal/timeout; mencoba ACRCloud jika dikonfigurasi"),
    }
    let music = acrcloud(ctx, bytes).await?;
    send_song(ctx, &music).await
}

fn song_text(v: &serde_json::Value) -> Option<String> {
    let pick = |keys: &[&str]| -> String {
        for key in keys {
            let value = &v[*key];
            if let Some(s) = value.as_str() {
                if !s.is_empty() && s != "-" {
                    return s.into();
                }
            }
        }
        "-".into()
    };
    let title = pick(&["title", "song", "name"]);
    if title == "-" {
        return None;
    }
    let artist = pick(&["artist", "artists", "singer", "author"]);
    let album = pick(&["album", "album_name"]);
    let duration = v["duration_ms"]
        .as_u64()
        .or_else(|| v["duration"].as_u64())
        .unwrap_or(0)
        / 1000;
    Some(format!(
        "🎵 *LAGU TERDETEKSI*\n\n*{}*\n👤 Artis : {}\n💿 Album : {}\n⏱️ Durasi: {}:{:02}",
        title,
        artist,
        album,
        duration / 60,
        duration % 60
    ))
}

async fn send_song(ctx: &Context, v: &serde_json::Value) -> Result<()> {
    match song_text(v) {
        Some(text) => ctx.reply(text).await,
        None => ctx.json(v).await,
    }
}
async fn doreso(ctx: &Context, bytes: &[u8]) -> Result<serde_json::Value> {
    let form = Form::new().part(
        "file",
        Part::bytes(bytes.to_vec())
            .file_name("lagu.mp3")
            .mime_str("audio/mpeg")?,
    );
    let upload = downloader::json(
        ctx.app
            .http
            .post("https://api.doreso.com/upload")
            .header("Origin", "https://www.aha-music.com")
            .header("Referer", "https://www.aha-music.com/")
            .multipart(form)
            .send()
            .await?,
    )
    .await?;
    let id = upload
        .pointer("/data/id")
        .context("Doreso tidak mengembalikan ID")?;
    let id = id
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| id.to_string());
    for _ in 0..10 {
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
        let value = downloader::get_json(
            &ctx.app.http,
            &format!("https://api.doreso.com/file/{id}"),
            &[],
        )
        .await?;
        if let Some(music) = value.pointer("/data/0/results/music/0/result") {
            return Ok(music.clone());
        }
    }
    anyhow::bail!("Lagu belum teridentifikasi; gunakan potongan suara yang lebih jelas")
}

async fn acrcloud(ctx: &Context, bytes: Vec<u8>) -> Result<serde_json::Value> {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    let key = ctx.app.config.required("ACRCLOUD_ACCESS_KEY")?;
    let secret = ctx.app.config.required("ACRCLOUD_ACCESS_SECRET")?;
    let host = ctx
        .app
        .config
        .env
        .get("ACRCLOUD_HOST")
        .map(String::as_str)
        .unwrap_or("identify-us-west-2.acrcloud.com");
    anyhow::ensure!(
        host.ends_with(".acrcloud.com") && !host.contains('/'),
        "ACRCLOUD_HOST tidak valid"
    );
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let signing = format!("POST\n/v1/identify\n{key}\naudio\n1\n{timestamp}");
    let mut mac = Hmac::<sha1::Sha1>::new_from_slice(secret.as_bytes())
        .map_err(|_| anyhow::anyhow!("ACRCloud HMAC key tidak valid"))?;
    mac.update(signing.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    let length = bytes.len().to_string();
    let form = Form::new()
        .text("access_key", key.to_string())
        .text("sample_bytes", length)
        .text("data_type", "audio")
        .text("signature_version", "1")
        .text("signature", signature)
        .text("timestamp", timestamp)
        .part("sample", Part::bytes(bytes).file_name("sample.mp3"));
    let v = downloader::json(
        ctx.app
            .http
            .post(format!("https://{host}/v1/identify"))
            .multipart(form)
            .send()
            .await?,
    )
    .await?;
    v.pointer("/metadata/music/0")
        .cloned()
        .context("ACRCloud belum mengidentifikasi lagu")
}
