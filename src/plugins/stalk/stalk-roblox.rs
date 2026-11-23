use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
use serde_json::{json, Value};
crate::plugins::plugin!(&["robloxstalk", "stalkroblox"], "stalk", execute);

fn count(v: &Value) -> String {
    v["count"].as_u64().unwrap_or(0).to_string()
}

fn names(v: &Value, key: &str) -> String {
    let list = v["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item[key].as_str().or_else(|| item["group"][key].as_str()))
                .take(5)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    if list.is_empty() {
        "-".into()
    } else {
        list
    }
}

async fn execute(ctx: &Context) -> Result<()> {
    anyhow::ensure!(!ctx.text.is_empty(), "/robloxstalk <username>");
    let found = downloader::json(
        ctx.app
            .http
            .post("https://users.roblox.com/v1/usernames/users")
            .json(&json!({"usernames":[ctx.text],"excludeBannedUsers":false}))
            .send()
            .await?,
    )
    .await?;
    let id = found
        .pointer("/data/0/id")
        .and_then(|v| v.as_u64())
        .context("Roblox user tidak ditemukan")?;
    let base = format!("https://users.roblox.com/v1/users/{id}");
    let mut result = json!({"profile":downloader::get_json(&ctx.app.http,&base,&[]).await?});
    for (key, endpoint) in [
        (
            "friends",
            format!("https://friends.roblox.com/v1/users/{id}/friends/count"),
        ),
        (
            "followers",
            format!("https://friends.roblox.com/v1/users/{id}/followers/count"),
        ),
        (
            "following",
            format!("https://friends.roblox.com/v1/users/{id}/followings/count"),
        ),
        (
            "badges",
            format!("https://badges.roblox.com/v1/users/{id}/badges?limit=10&sortOrder=Desc"),
        ),
        (
            "groups",
            format!("https://groups.roblox.com/v1/users/{id}/groups/roles"),
        ),
        (
            "games",
            format!("https://games.roblox.com/v2/users/{id}/games?accessFilter=Public&limit=10&sortOrder=Asc"),
        ),
    ] {
        if let Ok(v) = downloader::get_json(&ctx.app.http, &endpoint, &[]).await {
            result[key] = v;
        }
    }

    let profile = &result["profile"];
    let name = profile["name"].as_str().unwrap_or("-");
    let display = profile["displayName"].as_str().unwrap_or(name);
    let bio = profile["description"].as_str().unwrap_or("-");
    let text = format!(
        "━━━━━━━━━━━━━━━━━━━━━━━━\n\
         *PROFIL ROBLOX*\n\
         ━━━━━━━━━━━━━━━━━━━━━━━━\n\
         👤 Nama     : *{}* (`{}`)\n\
         🆔 ID       : `{}`\n\
         📝 Bio      : {}\n\
         👥 Teman    : *{}*\n\
         ❤️ Followers : *{}*\n\
         ➡️ Following : *{}*\n\
         🏅 Badge     : {}\n\
         👥 Grup      : {}\n\
         🎮 Game      : {}",
        display,
        name,
        id,
        bio,
        count(&result["friends"]),
        count(&result["followers"]),
        count(&result["following"]),
        names(&result["badges"], "name"),
        names(&result["groups"], "name"),
        names(&result["games"], "name")
    );
    ctx.reply(text).await
}
