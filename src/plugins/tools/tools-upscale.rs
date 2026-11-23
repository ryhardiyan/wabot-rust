use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{bail, Context as _, Result};
use reqwest::multipart::{Form, Part};
use serde_json::json;
use std::time::Duration;
crate::plugins::plugin!(&["upscale", "hd", "remini", "enhancer"], "tools", execute);
async fn execute(ctx: &Context) -> Result<()> {
    let bytes = ctx.m.download(ctx.app.config.max_media_bytes).await?;
    let scale = if ctx.arg(0) == "4" { 4 } else { 2 };
    let result = match engine1(ctx, &bytes, scale).await {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(error=%e,"ImgLarger gagal");
            match ilove(ctx, &bytes, scale, false).await {
                Ok(v) => v,
                Err(_) => ilove(ctx, &bytes, scale, true).await?,
            }
        }
    };
    ctx.m
        .send_media(
            result,
            "image/jpeg",
            "upscaled.jpg",
            &format!("✅ Upscale {scale}x selesai"),
        )
        .await
}
async fn engine1(ctx: &Context, bytes: &[u8], scale: u32) -> Result<Vec<u8>> {
    let form = Form::new()
        .part("myfile", Part::bytes(bytes.to_vec()).file_name("image.jpg"))
        .text("scaleRadio", scale.to_string());
    let upload = downloader::json(
        ctx.app
            .http
            .post("https://get1.imglarger.com/api/UpscalerNew/UploadNew")
            .header("Origin", "https://imgupscaler.com")
            .multipart(form)
            .send()
            .await?,
    )
    .await?;
    let code = upload
        .pointer("/data/code")
        .context("ImgLarger code kosong")?;
    for _ in 0..15 {
        let v = downloader::json(
            ctx.app
                .http
                .post("https://get1.imglarger.com/api/UpscalerNew/CheckStatusNew")
                .json(&json!({"code":code,"scaleRadio":scale}))
                .send()
                .await?,
        )
        .await?;
        if v.pointer("/data/status").and_then(|s| s.as_str()) == Some("success") {
            let url = v
                .pointer("/data/downloadUrl")
                .and_then(|s| s.as_str())
                .context("URL upscale kosong")?;
            return downloader::download(&ctx.app.http, url, ctx.app.config.max_media_bytes).await;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    bail!("ImgLarger timeout")
}
async fn ilove(ctx: &Context, bytes: &[u8], scale: u32, third: bool) -> Result<Vec<u8>> {
    let html = String::from_utf8(
        downloader::download(
            &ctx.app.http,
            if third {
                "https://www.iloveimg.com/id/tingkatkan-gambar"
            } else {
                "https://www.iloveimg.com/upscale-image"
            },
            2 * 1024 * 1024,
        )
        .await?,
    )?;
    let token_re = regex::Regex::new(r#""token":"([^"]+)""#)?;
    let task_re = regex::Regex::new(r"ilovepdfConfig\.taskId\s*=\s*'([^']+)'")?;
    let token = token_re
        .captures(&html)
        .and_then(|c| c.get(1))
        .context("Token iLoveIMG kosong")?
        .as_str();
    let task = task_re
        .captures(&html)
        .and_then(|c| c.get(1))
        .context("Task iLoveIMG kosong")?
        .as_str();
    let api = if third {
        "https://api1g.iloveimg.com/v1"
    } else {
        "https://api29g.iloveimg.com/v1"
    };
    let form = Form::new()
        .text("name", "image.jpg")
        .text("chunk", "0")
        .text("chunks", "1")
        .text("task", task.to_string())
        .text("preview", "1")
        .text("v", "web.0")
        .part("file", Part::bytes(bytes.to_vec()).file_name("image.jpg"));
    let upload = downloader::json(
        ctx.app
            .http
            .post(format!("{api}/upload"))
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await?,
    )
    .await?;
    let filename = upload["server_filename"]
        .as_str()
        .context("Server filename kosong")?;
    let form = if third {
        Form::new()
            .text("packaged_filename", "upscaled")
            .text("multiplier", scale.to_string())
            .text("task", task.to_string())
            .text("tool", "upscaleimage")
            .text("files[0][server_filename]", filename.to_string())
            .text("files[0][filename]", "image.jpg")
    } else {
        Form::new()
            .text("task", task.to_string())
            .text("server_filename", filename.to_string())
            .text("scale", scale.to_string())
    };
    let response = ctx
        .app
        .http
        .post(format!(
            "{api}/{}",
            if third { "process" } else { "upscale" }
        ))
        .bearer_auth(token)
        .header("Origin", "https://www.iloveimg.com")
        .multipart(form)
        .send()
        .await?;
    if third {
        let v = downloader::json(response).await?;
        anyhow::ensure!(v["status"] == "TaskSuccess", "iLoveIMG gagal");
        downloader::download(
            &ctx.app.http,
            &format!("{api}/download/{task}"),
            ctx.app.config.max_media_bytes,
        )
        .await
    } else {
        downloader::bounded(response, ctx.app.config.max_media_bytes).await
    }
}
