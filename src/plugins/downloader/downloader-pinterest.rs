use crate::{bot_lib::downloader, plugins::Context};
use anyhow::Result;
use serde_json::json;
crate::plugins::plugin!(&["pinterest", "pint", "pinsearch"], "downloader", execute);
async fn execute(ctx: &Context) -> Result<()> {
    anyhow::ensure!(!ctx.text.is_empty(), "/pinterest <kata pencarian>");
    let client = &ctx.app.http;
    downloader::bounded(
        client.get("https://id.pinterest.com/").send().await?,
        2 * 1024 * 1024,
    )
    .await?;
    let source = format!("/search/pins/?q={}", ctx.text);
    let data=json!({"options":{"query":ctx.text,"scope":"pins","page_size":10,"source_url":source},"context":{}}).to_string();
    let v = downloader::get_json(
        client,
        "https://id.pinterest.com/resource/BaseSearchResource/get/",
        &[("source_url", &source), ("data", &data)],
    )
    .await?;
    if let Some(items) = v
        .pointer("/resource_response/data/results")
        .and_then(|v| v.as_array())
    {
        for item in items.iter().take(5) {
            if let Some(url) = item
                .pointer("/images/orig/url")
                .or_else(|| item.pointer("/images/736x/url"))
                .and_then(|v| v.as_str())
            {
                ctx.media_url(
                    url,
                    "image/jpeg",
                    item["title"].as_str().unwrap_or("Pinterest"),
                )
                .await?;
            }
        }
        return Ok(());
    }
    ctx.json(&v).await
}
