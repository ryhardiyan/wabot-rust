use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
use reqwest::multipart::{Form, Part};
use serde_json::{json, Value};
crate::plugins::plugin!(&["wink"], "tools", execute, owner_only = true);
fn string(v: &Value) -> String {
    v.as_str()
        .map(String::from)
        .unwrap_or_else(|| v.to_string())
}
fn required(v: &Value, key: &str) -> Result<String> {
    v.get(key)
        .filter(|v| !v.is_null())
        .map(string)
        .with_context(|| format!("Wink: field {key} kosong"))
}
fn params(id: &str) -> Vec<(String, String)> {
    [
        ("client_id", "1189857605"),
        ("version", "5.1.2"),
        ("country_code", "ID"),
        ("gnum", id),
        ("client_language", "en_US"),
        ("client_channel_id", ""),
        ("client_timezone", "Asia/Jakarta"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect()
}
async fn get(client: &reqwest::Client, path: &str, p: &[(String, String)]) -> Result<Value> {
    downloader::json(
        client
            .get(format!("https://wink.ai{path}"))
            .query(p)
            .send()
            .await?,
    )
    .await
}
async fn post(client: &reqwest::Client, path: &str, p: &[(String, String)]) -> Result<Value> {
    downloader::json(
        client
            .post(format!("https://wink.ai{path}"))
            .form(p)
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
    let jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
    let url = reqwest::Url::parse("https://wink.ai")?;
    jar.add_cookie_str(&format!("_sm={id}; Path=/; Domain=wink.ai"), &url);
    jar.add_cookie_str(
        &format!("meitustat=%7B%22wgid%22%3A%22{id}%22%7D; Path=/; Domain=wink.ai"),
        &url,
    );
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("origin", "https://wink.ai".parse()?);
    headers.insert("referer", "https://wink.ai/video-enhancer/upload".parse()?);
    headers.insert("ab_info", r#"{"ab_codes":[],"version":"1.4.4"}"#.parse()?);
    let client = reqwest::Client::builder()
        .cookie_provider(jar)
        .default_headers(headers)
        .user_agent("Mozilla/5.0")
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    let mut p = params(&id);
    p.extend([
        ("suffix".into(), ".mp4".into()),
        ("type".into(), "temp".into()),
        ("count".into(), "1".into()),
    ]);
    let sign = get(&client, "/api/file/get_maat_sign.json", &p).await?;
    anyhow::ensure!(sign["code"] == 0, "Wink otorisasi gagal");
    let sign = &sign["data"];
    let mut fields = Vec::new();
    for (k, target) in [
        ("app", "app"),
        ("count", "count"),
        ("sig", "sig"),
        ("sig_time", "sigTime"),
        ("sig_version", "sigVersion"),
        ("suffix", "suffix"),
        ("type", "type"),
    ] {
        fields.push((target, required(sign, k)?));
    }
    let policy = downloader::json(
        client
            .get("https://strategy.app.meitudata.com/upload/policy")
            .query(&fields)
            .send()
            .await?,
    )
    .await?;
    let policy = policy
        .pointer("/0/qiniu")
        .context("Wink upload policy kosong")?;
    let key = required(policy, "key")?;
    let name = format!("{id}.mp4");
    let form = Form::new()
        .part(
            "file",
            Part::bytes(bytes)
                .file_name(name.clone())
                .mime_str("video/mp4")?,
        )
        .text("token", required(policy, "token")?)
        .text("key", key.clone())
        .text("fname", name);
    let upload = downloader::json(
        client
            .post(required(policy, "url")?)
            .multipart(form)
            .send()
            .await?,
    )
    .await?;
    let source = upload
        .get("url")
        .or_else(|| upload.get("data"))
        .or_else(|| policy.get("data"))
        .and_then(Value::as_str)
        .context("Wink source URL kosong")?
        .to_string();
    let mut p = params(&id);
    p.push(("file_key".into(), key));
    post(
        &client,
        "/api/file/video_cover_and_display_info_ext.json",
        &p,
    )
    .await?;
    let trans = post(&client, "/api/file/video_trans_start.json", &p).await?;
    let trans_id = required(&trans["data"], "id")?;
    let mut query = params(&id);
    query.push(("id".into(), trans_id));
    let mut transcoded = source.clone();
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let v = get(&client, "/api/file/video_trans_query.json", &query).await?;
        if let Some(s) = v
            .pointer("/data/video_transcoded")
            .or_else(|| v.pointer("/data/transcoded_url"))
            .and_then(Value::as_str)
        {
            transcoded = s.into();
            break;
        }
    }
    let mut p = params(&id);
    p.extend([
        ("type".into(),"11".into()),("content_type".into(),"2".into()),("source_url".into(),source),
        ("type_params".into(),json!({"is_mirror":0,"orientation_tag":1,"j_420_trans":"1","return_ext":"2"}).to_string()),
        ("right_detail".into(),json!({"source":"1","touch_type":"4","function_id":"630","material_id":"63011","url":"https://wink.ai/video-enhancer/upload"}).to_string()),
        ("ext_params".into(),json!({"task_name":format!("Enhancer-{id}"),"records":"11","video_transcoded":transcoded}).to_string()),("with_prepare".into(),"1".into())]);
    let delivery = post(&client, "/api/meitu_ai/delivery.json", &p).await?;
    let mut msg_id = delivery
        .pointer("/data/msg_id")
        .or_else(|| delivery.pointer("/data/prepare_msg_id"))
        .map(string)
        .context("Wink task ID kosong")?;
    ctx.reply("⏳ Wink AI sedang memproses video Ultra HD...")
        .await?;
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let mut p = params(&id);
        p.push(("msg_ids".into(), msg_id.clone()));
        let value = get(&client, "/api/meitu_ai/query_batch.json", &p).await?;
        let item = value
            .pointer("/data/item_list/0")
            .context("Wink item kosong")?;
        let result = &item["result"];
        if let Some(url) = result
            .pointer("/media_info_list/0/media_data")
            .or_else(|| result.get("result_url"))
            .or_else(|| result.get("url"))
            .and_then(Value::as_str)
        {
            return ctx
                .media_url(url, "video/mp4", "🎬 Wink Ultra HD selesai")
                .await;
        }
        if let Some(code) = result["error_code"].as_i64() {
            anyhow::ensure!(code == 0 || code == 29901, "Wink gagal: {code}");
        }
        if let Some(next) = result
            .get("result")
            .or_else(|| result.get("msg_id"))
            .or_else(|| item.get("msg_id"))
            .and_then(Value::as_str)
        {
            if next != msg_id && !next.starts_with("http") && !next.starts_with("wpr_") {
                msg_id = next.into();
            }
        }
    }
    anyhow::bail!("Wink timeout")
}
