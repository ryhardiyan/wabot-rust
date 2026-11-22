use super::Registry;

#[path = "downloader-download.rs"]
mod downloader_download;
#[path = "downloader-mediafire.rs"]
mod downloader_mediafire;
#[path = "downloader-pinterest.rs"]
mod downloader_pinterest;
#[path = "downloader-spotify.rs"]
mod downloader_spotify;
#[path = "downloader-tiktok.rs"]
mod downloader_tiktok;
#[path = "downloader-twitter.rs"]
mod downloader_twitter;
#[path = "downloader-xiaohongshu.rs"]
mod downloader_xiaohongshu;
#[path = "downloader-youtube.rs"]
mod downloader_youtube;
#[path = "downloader-ytplay.rs"]
mod downloader_ytplay;

pub(super) fn register(plugins: &mut Registry) {
    plugins.extend([
        Box::new(downloader_download::Command) as Box<dyn crate::plugins::Plugin>,
        Box::new(downloader_mediafire::Command),
        Box::new(downloader_pinterest::Command),
        Box::new(downloader_spotify::Command),
        Box::new(downloader_tiktok::Command),
        Box::new(downloader_twitter::Command),
        Box::new(downloader_xiaohongshu::Command),
        Box::new(downloader_youtube::Command),
        Box::new(downloader_ytplay::Command),
    ]);
}

use crate::{bot_lib::downloader as dl, plugins::Context};
use anyhow::{Context as _, Result};
use serde_json::Value;

pub(super) async fn platform(ctx: &Context, kind: &str) -> Result<()> {
    let text = if ctx.text.is_empty() {
        ctx.m.quoted.as_ref().map(|q| q.text.as_str()).unwrap_or("")
    } else {
        &ctx.text
    };
    let (detected, url) = dl::ext(text).context("Masukkan URL platform yang didukung")?;
    anyhow::ensure!(detected == kind, "URL tidak cocok dengan platform command");
    let result = dl::resolve(&ctx.app.http, kind, &url).await?;
    deliver(ctx, &result, kind).await
}
pub(super) async fn deliver(ctx: &Context, result: &Value, kind: &str) -> Result<()> {
    let title = result
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Unduhan media");
    let audio = matches!(kind, "yt" | "sp" | "sc");
    if let Some(images) = result.get("images").and_then(Value::as_array) {
        for image in images.iter().take(10) {
            if let Some(url) = image.as_str() {
                ctx.media_url(url, "image/jpeg", title).await?;
            }
        }
        return Ok(());
    }
    if let Some(url) = find_url(result) {
        return ctx
            .media_url(
                url,
                if audio {
                    "audio/mpeg"
                } else if kind == "mf" || kind == "sf" || kind == "mg" {
                    "application/octet-stream"
                } else {
                    "video/mp4"
                },
                title,
            )
            .await;
    }
    if let Some(items) = result
        .get("medias")
        .or_else(|| result.get("media"))
        .or_else(|| result.get("url"))
        .or_else(|| result.as_array().map(|_| result))
        .and_then(Value::as_array)
    {
        let mut sent = false;
        for item in items.iter().take(10) {
            if let Some(url) = find_url(item) {
                let mime = if item["type"] == "image" || kind == "ig" || kind == "pin" {
                    "image/jpeg"
                } else {
                    "video/mp4"
                };
                ctx.media_url(url, mime, title).await?;
                sent = true;
            }
        }
        if sent {
            return Ok(());
        }
    }
    ctx.json(result).await
}
pub(super) fn find_url(v: &Value) -> Option<&str> {
    if let Some(s) = v
        .as_str()
        .filter(|s| s.starts_with("https://") || s.starts_with("http://"))
    {
        return Some(s);
    }
    [
        "download_url",
        "downloadUrl",
        "play",
        "mp3",
        "url",
        "link",
        "download",
        "hd",
        "sd",
    ]
    .into_iter()
    .find_map(|key| {
        v.get(key)
            .and_then(Value::as_str)
            .filter(|s| s.starts_with("http"))
    })
}
