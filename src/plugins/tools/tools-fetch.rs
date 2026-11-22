use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
use std::time::{Duration, Instant};

crate::plugins::plugin!(
    &["fetch", "get"],
    "tools",
    "HTTP client dengan method, body, header, timeout",
    execute
);
async fn execute(ctx: &Context) -> Result<()> {
    if ctx.text.is_empty() {
        return ctx.reply("/fetch <url> [--method=POST] [--json={...}] [--header=Key:Value] [--headers] [--no-redirect] [--timeout=30000]").await;
    }
    let mut args = ctx.text.split_whitespace();
    let url = args.next().context("URL kosong")?;
    let parsed = reqwest::Url::parse(url)?;
    anyhow::ensure!(
        matches!(parsed.scheme(), "http" | "https"),
        "URL harus HTTP/HTTPS"
    );
    let mut method = reqwest::Method::GET;
    let mut timeout = 30000;
    let mut body = None;
    let mut content_type = None;
    let mut headers = Vec::new();
    let mut redirect = true;
    let mut verbose = false;
    let mut show_headers = false;
    for flag in args {
        if let Some(v) = flag.strip_prefix("--method=") {
            method = v.parse()?;
        } else if let Some(v) = flag.strip_prefix("--timeout=") {
            timeout = v.parse::<u64>()?.clamp(100, 60000);
        } else if let Some(v) = flag.strip_prefix("--json=") {
            serde_json::from_str::<serde_json::Value>(v)?;
            body = Some(v.to_string());
            content_type = Some("application/json".to_string());
        } else if let Some(v) = flag
            .strip_prefix("--body=")
            .or_else(|| flag.strip_prefix("--data="))
        {
            body = Some(v.to_string());
        } else if let Some(v) = flag.strip_prefix("--content-type=") {
            content_type = Some(v.to_string());
        } else if let Some(v) = flag.strip_prefix("--header=") {
            let (k, v) = v.split_once(':').context("Header harus Key:Value")?;
            headers.push((k.to_string(), v.to_string()));
        } else if flag == "--no-redirect" {
            redirect = false;
        } else if ["--verbose", "-v", "--debug", "-d"].contains(&flag) {
            verbose = true;
        } else if ["--headers", "-h"].contains(&flag) {
            show_headers = true;
        } else if ["--stream", "-s"].contains(&flag) {
            // Seluruh respons dibaca per chunk dengan batas keras, bukan arrayBuffer tanpa batas.
        } else {
            anyhow::bail!("Flag fetch tidak dikenal: {flag}");
        }
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout))
        .redirect(if redirect {
            reqwest::redirect::Policy::limited(5)
        } else {
            reqwest::redirect::Policy::none()
        })
        .build()?;
    let mut request = client.request(method.clone(), parsed);
    for (k, v) in headers {
        request = request.header(k, v);
    }
    if let Some(v) = content_type {
        request = request.header("content-type", v);
    }
    if let Some(v) = body {
        request = request.body(v);
    }
    let start = Instant::now();
    let response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        return ctx.reply(format!("HTTP ERROR {status}\n{url}")).await;
    }
    let mime = response
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("application/octet-stream")
        .split(';')
        .next()
        .unwrap_or("application/octet-stream")
        .to_string();
    let header_text = if show_headers {
        format!("{:#?}", response.headers())
    } else {
        String::new()
    };
    let data = downloader::bounded(response, ctx.app.config.max_media_bytes).await?;
    let caption = format!(
        "HTTP {} · {} bytes · {} ms\n{}",
        status,
        data.len(),
        start.elapsed().as_millis(),
        mime
    );
    ctx.m
        .send_media(data, &mime, "fetch_result.bin", &caption)
        .await?;
    if show_headers {
        ctx.reply(header_text).await?;
    }
    if verbose {
        ctx.reply(format!("{method} {url}\n{caption}")).await?;
    }
    Ok(())
}
