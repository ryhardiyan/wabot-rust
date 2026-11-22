use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
crate::plugins::plugin!(
    &["rednote", "xhs", "xiaohongshu"],
    "downloader",
    "Unduh RedNote/Xiaohongshu",
    execute
);
async fn execute(ctx: &Context) -> Result<()> {
    let url = ctx
        .text
        .split_whitespace()
        .find(|s| s.starts_with("https://"))
        .context("/xhs <URL>")?;
    let v = downloader::json(
        ctx.app
            .http
            .post("https://rednote.savevideodown.com/api/download")
            .header("origin", "https://rednote.savevideodown.com")
            .header("referer", "https://rednote.savevideodown.com/")
            .json(&serde_json::json!({"url":url}))
            .send()
            .await?,
    )
    .await?;
    anyhow::ensure!(v["success"] == true, "RedNote API gagal");
    let title = v["title"].as_str().unwrap_or("RedNote");
    if v["type"] == "video" {
        return ctx
            .media_url(
                v["directVideoUrl"]
                    .as_str()
                    .or_else(|| v["downloadUrl"].as_str())
                    .context("URL video kosong")?,
                "video/mp4",
                title,
            )
            .await;
    }
    for item in v["directImages"]
        .as_array()
        .context("Gambar kosong")?
        .iter()
        .take(10)
    {
        let url = item
            .as_str()
            .or_else(|| item["url"].as_str())
            .context("URL gambar kosong")?;
        ctx.media_url(url, "image/jpeg", title).await?;
    }
    Ok(())
}
