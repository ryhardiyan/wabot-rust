use crate::{bot_lib::downloader, plugins::Context};
use serde_json::Value;
crate::plugins::plugin!(&["server"], "owner", execute);

fn status_text(v: &Value, host: &str) -> String {
    let motd = v["motd"]["clean"]
        .as_str()
        .or_else(|| v["motd"]["raw"].as_str())
        .unwrap_or("-");
    let players = &v["players"];
    let online = players["online"].as_u64().unwrap_or(0);
    let max = players["max"].as_u64().unwrap_or(0);
    let version = v["version"]["name"]
        .as_str()
        .or_else(|| v["version"].as_str())
        .unwrap_or("-");
    let list = players["list"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|p| p["name"].as_str())
                .take(10)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "-".into());
    let state = if v["online"].as_bool() == Some(true) {
        "🟢 ONLINE"
    } else {
        "🔴 OFFLINE"
    };
    format!(
        "━━━━━━━━━━━━━━━━━━━━━━━━\n\
         *MINECRAFT SERVER*\n\
         ━━━━━━━━━━━━━━━━━━━━━━━━\n\
         🖥️ Host    : `{host}`\n\
         📊 Status  : *{state}*\n\
         ⚙️ Versi   : {version}\n\
         👥 Pemain  : *{online}/{max}*\n\
         📢 MOTD    : {motd}\n\
         🧩 Pemain  : {list}\n\
         ⏱️ Ping    : {} ms",
        v["ping"].as_u64().unwrap_or(0)
    )
}

async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let host = ctx
        .app
        .config
        .env
        .get("MINECRAFT_SERVER")
        .map(String::as_str)
        .unwrap_or("103.42.116.182:25045");
    let value = downloader::get_json(
        &ctx.app.http,
        &format!("https://api.mcstatus.io/v2/status/java/{host}"),
        &[],
    )
    .await?;
    if value.is_object() && (value["online"].is_boolean() || value["players"].is_object()) {
        return ctx.reply(status_text(&value, host)).await;
    }
    ctx.json(&value).await
}
