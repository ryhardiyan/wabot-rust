use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
use serde_json::Value;
crate::plugins::plugin!(&["ytmp4", "ytmp3"], "downloader", execute);
async fn proxy(ctx: &Context, url: &str) -> Result<Value> {
    downloader::json(
        ctx.app
            .http
            .post("https://app.ytdown.to/proxy.php")
            .header("Origin", "https://app.ytdown.to")
            .header("Referer", "https://app.ytdown.to/id2/")
            .header("X-Requested-With", "XMLHttpRequest")
            .form(&[("url", url)])
            .send()
            .await?,
    )
    .await
}
async fn execute(ctx: &Context) -> Result<()> {
    let url = ctx.arg(0);
    anyhow::ensure!(!url.is_empty(), "/ytmp3 atau /ytmp4 <URL>");
    let v = proxy(ctx, url).await?;
    let api = &v["api"];
    let video = ctx.command == "ytmp4";
    let item = api["mediaItems"]
        .as_array()
        .context("Pilihan media kosong")?
        .iter()
        .find(|m| m["type"] == if video { "Video" } else { "Audio" })
        .context("Media yang diminta tidak tersedia")?;
    let media_url = item["mediaUrl"].as_str().context("Media URL kosong")?;
    for _ in 0..50 {
        let v = proxy(ctx, media_url).await?;
        if v.pointer("/api/status").and_then(Value::as_str) == Some("completed") {
            return ctx
                .media_url(
                    v.pointer("/api/fileUrl")
                        .and_then(Value::as_str)
                        .context("File URL kosong")?,
                    if video {
                        "video/mp4"
                    } else if item["mediaExtension"] == "m4a" {
                        "audio/mp4"
                    } else {
                        "audio/mpeg"
                    },
                    api["title"].as_str().unwrap_or("YouTube"),
                )
                .await;
        }
        anyhow::ensure!(
            v.pointer("/api/status").and_then(Value::as_str) == Some("queued"),
            "YTDown gagal memproses media"
        );
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    anyhow::bail!("YTDown timeout")
}
