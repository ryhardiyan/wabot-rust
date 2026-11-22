use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
use reqwest::multipart::Form;
use serde_json::Value;
crate::plugins::plugin!(
    &["hdvid", "hdvideo"],
    "tools",
    "UnblurImage video enhancer 2K",
    execute,
    owner_only = true
);
async fn post(ctx: &Context, path: &str, fields: &[(&str, String)], id: &str) -> Result<Value> {
    let mut form = Form::new();
    for (k, v) in fields {
        form = form.text(k.to_string(), v.clone());
    }
    downloader::json(
        ctx.app
            .http
            .post(format!("https://api.unwatermark.ai{path}"))
            .header("origin", "https://unblurimage.ai")
            .header("referer", "https://unblurimage.ai/")
            .header("product-code", "067003")
            .header("product-serial", &id[..6])
            .header("x-request-id", id)
            .multipart(form)
            .send()
            .await?,
    )
    .await
}
async fn execute(ctx: &Context) -> Result<()> {
    anyhow::ensure!(
        crate::bot_lib::core::message::mime(ctx.m.media()).starts_with("video/"),
        "Kirim atau balas video"
    );
    let bytes = ctx.m.download(ctx.app.config.max_media_bytes).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let upload = post(
        ctx,
        "/api/web/common/upload/video",
        &[("video_file_name", format!("{id}.mp4"))],
        &id,
    )
    .await?;
    let url = upload
        .pointer("/result/url")
        .and_then(Value::as_str)
        .context("Signed URL kosong")?;
    ctx.app
        .http
        .put(url)
        .header("content-type", "video/mp4")
        .body(bytes)
        .send()
        .await?
        .error_for_status()?;
    let public = url.split('?').next().context("URL kosong")?;
    let job = post(
        ctx,
        "/api/web/unblurimage/v1/video-enhancer/create-job",
        &[
            ("original_video_url", public.into()),
            ("resolution", "2k".into()),
            ("is_preview", "false".into()),
        ],
        &id,
    )
    .await?;
    let job_id = job
        .pointer("/result/job_id")
        .and_then(Value::as_str)
        .context("Job ID kosong")?;
    ctx.reply("⏳ Video sedang diproses ke 2K...").await?;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
        let v=downloader::json(ctx.app.http.get(format!("https://api.unwatermark.ai/api/web/unblurimage/v1/video-enhancer/get-job/{job_id}")).header("product-code","067003").header("product-serial",&id[..6]).header("x-request-id",&id).header("origin","https://unblurimage.ai").send().await?).await?;
        let output = v
            .pointer("/result/output_url/0")
            .or_else(|| v.pointer("/result/download_url"))
            .and_then(Value::as_str);
        if let Some(url) = output {
            return ctx.media_url(url, "video/mp4", "✅ Video 2K selesai").await;
        }
        anyhow::ensure!(
            v.pointer("/result/status") != Some(&Value::from(3)),
            "Pemrosesan video gagal"
        );
    }
    anyhow::bail!("Video enhancer timeout")
}
