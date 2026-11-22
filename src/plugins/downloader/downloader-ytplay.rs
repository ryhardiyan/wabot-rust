use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
crate::plugins::plugin!(
    &["play", "p"],
    "downloader",
    "Cari video atau putar audio YouTube",
    execute
);
async fn execute(ctx: &Context) -> Result<()> {
    let text = if ctx.text.is_empty() {
        ctx.m.quoted.as_ref().map(|q| q.text.as_str()).unwrap_or("")
    } else {
        &ctx.text
    };
    anyhow::ensure!(
        !text.is_empty(),
        "/play <query|URL> atau /play s <query> atau /play v <URL>"
    );
    let (endpoint, key, value, mime) = if let Some(s) = text.strip_prefix("v ") {
        ("ytmp4", "url", s, "video/mp4")
    } else if let Some(s) = text.strip_prefix("s ") {
        ("youtube", "q", s, "")
    } else if text.starts_with("http") {
        ("ytmp3", "url", text, "audio/mpeg")
    } else {
        ("ytplay", "query", text, "audio/mpeg")
    };
    let v = downloader::get_json(
        &ctx.app.http,
        &format!("https://api-faa.my.id/faa/{endpoint}"),
        &[(key, value)],
    )
    .await?;
    anyhow::ensure!(v["status"] != false, "YouTube API gagal");
    if mime.is_empty() {
        return ctx.json(&v["result"]).await;
    }
    let result = &v["result"];
    let url = super::find_url(result).context("URL hasil YouTube kosong")?;
    ctx.media_url(url, mime, result["title"].as_str().unwrap_or("YouTube"))
        .await
}
