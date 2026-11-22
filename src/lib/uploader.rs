use crate::{bot_lib::downloader, config::Config};
use anyhow::{bail, Context, Result};
use base64::Engine;
use reqwest::multipart::{Form, Part};
use serde_json::{json, Value};

pub async fn upload(client: &reqwest::Client, bytes: Vec<u8>, mime: &str) -> Result<String> {
    anyhow::ensure!(!bytes.is_empty(), "Media kosong");
    let mut errors = Vec::new();
    for (endpoint, field, pointer) in [
        ("https://catbox.moe/user/api.php", "fileToUpload", "text"),
        ("https://uguu.se/upload.php", "files[]", "/files/0/url"),
        ("https://qu.ax/upload.php", "files[]", "/files/0/url"),
        ("https://put.icu/upload/", "", "/direct_url"),
        ("https://tmpfiles.org/api/v1/upload", "file", "/data/url"),
    ] {
        let attempt = async {
            let response = if field.is_empty() {
                client
                    .put(endpoint)
                    .header("content-type", mime)
                    .body(bytes.clone())
                    .send()
                    .await?
            } else {
                let ext = match mime {
                    "image/jpeg" => "jpg",
                    "image/png" => "png",
                    "image/webp" => "webp",
                    "video/mp4" => "mp4",
                    "audio/mpeg" => "mp3",
                    _ => "bin",
                };
                let part = Part::bytes(bytes.clone())
                    .file_name(format!("media.{ext}"))
                    .mime_str(mime)?;
                let mut form = Form::new().part(field, part);
                if pointer == "text" {
                    form = form.text("reqtype", "fileupload");
                }
                client.post(endpoint).multipart(form).send().await?
            };
            let data = downloader::bounded(response, 256 * 1024).await?;
            let url = if pointer == "text" {
                String::from_utf8(data)?.trim().into()
            } else {
                serde_json::from_slice::<Value>(&data)?
                    .pointer(pointer)
                    .and_then(Value::as_str)
                    .context("URL upload kosong")?
                    .to_string()
            };
            let parsed = reqwest::Url::parse(&url)?;
            anyhow::ensure!(
                matches!(parsed.scheme(), "http" | "https"),
                "URL upload tidak valid"
            );
            if parsed.host_str() == Some("tmpfiles.org") && !parsed.path().starts_with("/dl/") {
                return Ok::<_, anyhow::Error>(format!("https://tmpfiles.org/dl{}", parsed.path()));
            }
            Ok(url)
        }
        .await;
        match attempt {
            Ok(url) => return Ok(url),
            Err(e) => errors.push(format!("{endpoint}: {e}")),
        }
    }
    bail!("Semua penyedia upload gagal: {}", errors.join("; "))
}
pub async fn github(
    client: &reqwest::Client,
    config: &Config,
    bytes: Vec<u8>,
    mime: &str,
) -> Result<String> {
    anyhow::ensure!(bytes.len() <= 25 * 1024 * 1024, "GitHub maksimum 25 MiB");
    let token = config.required("GITHUB_TOKEN")?;
    let user = downloader::json(
        client
            .get("https://api.github.com/user")
            .bearer_auth(token)
            .send()
            .await?,
    )
    .await?;
    let owner = user["login"].as_str().context("GitHub login kosong")?;
    let repo = config
        .env
        .get("GITHUB_MEDIA_REPO")
        .map(String::as_str)
        .unwrap_or("database");
    let endpoint = format!("https://api.github.com/repos/{owner}/{repo}");
    let response = client.get(&endpoint).bearer_auth(token).send().await?;
    let metadata = if response.status() == 404 {
        downloader::json(
            client
                .post("https://api.github.com/user/repos")
                .bearer_auth(token)
                .json(&json!({"name":repo,"private":false,"auto_init":true}))
                .send()
                .await?,
        )
        .await?
    } else {
        downloader::json(response).await?
    };
    let branch = metadata["default_branch"].as_str().unwrap_or("main");
    let ext = mime
        .split('/')
        .nth(1)
        .unwrap_or("bin")
        .split(';')
        .next()
        .unwrap_or("bin");
    let name = format!(
        "uploads/{}.{}",
        uuid::Uuid::new_v4(),
        ext.chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
    );
    let v = downloader::json(client.put(format!("{endpoint}/contents/{name}")).bearer_auth(token)
        .json(&json!({"message":format!("Upload {name}"),"content":base64::engine::general_purpose::STANDARD.encode(bytes),"branch":branch})).send().await?).await?;
    v.pointer("/content/download_url")
        .and_then(Value::as_str)
        .map(String::from)
        .context("GitHub tidak mengembalikan download_url")
}
