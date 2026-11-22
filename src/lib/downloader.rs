use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::time::Duration;

pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent("MyWa-Sockets/11.0 Rust")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(2)
        .cookie_store(true)
        .build()?)
}
pub async fn bounded(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    response.error_for_status_ref()?;
    if response.content_length().is_some_and(|n| n > limit as u64) {
        bail!("Respons melebihi batas ukuran");
    }
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > limit.saturating_sub(data.len()) {
            bail!("Respons melebihi batas ukuran");
        }
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}
pub async fn json(response: reqwest::Response) -> Result<Value> {
    Ok(serde_json::from_slice(
        &bounded(response, 2 * 1024 * 1024).await?,
    )?)
}
pub async fn get_json(
    client: &reqwest::Client,
    endpoint: &str,
    params: &[(&str, &str)],
) -> Result<Value> {
    json(client.get(endpoint).query(params).send().await?).await
}
pub async fn download(client: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>> {
    let url = reqwest::Url::parse(url)?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("Protokol URL harus HTTP/HTTPS");
    }
    bounded(client.get(url).send().await?, limit).await
}
pub fn ext(text: &str) -> Option<(&'static str, String)> {
    for word in text.split_whitespace() {
        let word = word.trim_end_matches(['.', ',', '!', '?']);
        let Ok(url) = reqwest::Url::parse(word) else {
            continue;
        };
        let host = url.host_str()?.trim_start_matches("www.");
        for (domain, kind) in [
            ("tiktok.com", "tt"),
            ("instagram.com", "ig"),
            ("pinterest.com", "pin"),
            ("pin.it", "pin"),
            ("facebook.com", "fb"),
            ("twitter.com", "tw"),
            ("x.com", "tw"),
            ("videy.co", "vd"),
            ("threads.net", "th"),
            ("threads.com", "th"),
            ("mega.nz", "mg"),
            ("soundcloud.com", "sc"),
            ("spotify.com", "sp"),
            ("youtube.com", "yt"),
            ("youtu.be", "yt"),
            ("sfile.co", "sf"),
            ("mediafire.com", "mf"),
        ] {
            if host == domain || host.ends_with(&format!(".{domain}")) {
                return Some((kind, word.into()));
            }
        }
    }
    None
}
pub async fn resolve(client: &reqwest::Client, kind: &str, url: &str) -> Result<Value> {
    let endpoint = match kind {
        "tt" => "https://tikwm.com/api/",
        "ig" => "https://api-faa.my.id/faa/igdl",
        "pin" => "https://api-faa.my.id/faa/pin-down",
        "fb" => "https://api-faa.my.id/faa/fbdownload",
        "mf" => "https://api-faa.my.id/faa/mediafire",
        "tw" => "https://api.nexray.web.id/downloader/twitter",
        "vd" => "https://api.nexray.web.id/downloader/videy",
        "th" => "https://api.nexray.web.id/downloader/threads",
        "mg" => "https://api.nexray.web.id/downloader/mega",
        "sc" => "https://api.nexray.web.id/downloader/soundcloud",
        "sp" => "https://api.nexray.web.id/downloader/spotify",
        "yt" => "https://api.nexray.web.id/downloader/ytmp3",
        "sf" => "https://api.nexray.web.id/downloader/sfile",
        _ => bail!("Platform unduhan belum dikenal"),
    };
    let value = get_json(client, endpoint, &[("url", url)]).await?;
    if kind == "tt" {
        if value["code"] != 0 {
            bail!("TikTok API mengembalikan kegagalan");
        }
        return value.get("data").cloned().context("Data TikTok kosong");
    }
    if value["status"] == false || value["success"] == false {
        bail!("API unduhan mengembalikan kegagalan");
    }
    value
        .get("result")
        .cloned()
        .context("Respons unduhan tidak memiliki result")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_lookalike_hosts() {
        assert!(ext("https://tiktok.com.attacker.invalid/x").is_none());
        assert_eq!(ext("https://vt.tiktok.com/abc.").map(|x| x.0), Some("tt"));
    }
    #[tokio::test]
    async fn local_http_is_bounded_and_json_checked() -> Result<()> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let router = axum::Router::new()
            .route(
                "/json",
                axum::routing::get(|| async { axum::Json(serde_json::json!({"ok":true})) }),
            )
            .route("/large", axum::routing::get(|| async { vec![b'x'; 1024] }));
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
        let http = client()?;
        assert_eq!(
            get_json(&http, &format!("http://{address}/json"), &[]).await?["ok"],
            true
        );
        assert!(download(&http, &format!("http://{address}/large"), 8)
            .await
            .is_err());
        server.abort();
        let _ = server.await;
        Ok(())
    }
}
