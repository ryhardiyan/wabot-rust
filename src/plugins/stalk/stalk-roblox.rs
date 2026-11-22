use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
use serde_json::json;
crate::plugins::plugin!(
    &["robloxstalk", "stalkroblox"],
    "stalk",
    "Profil dan statistik publik Roblox",
    execute
);
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
    for(key,endpoint)in[("friends",format!("https://friends.roblox.com/v1/users/{id}/friends/count")),("followers",format!("https://friends.roblox.com/v1/users/{id}/followers/count")),("following",format!("https://friends.roblox.com/v1/users/{id}/followings/count")),("badges",format!("https://badges.roblox.com/v1/users/{id}/badges?limit=10&sortOrder=Desc")),("groups",format!("https://groups.roblox.com/v1/users/{id}/groups/roles")),("games",format!("https://games.roblox.com/v2/users/{id}/games?accessFilter=Public&limit=10&sortOrder=Asc"))]{
        if let Ok(v)=downloader::get_json(&ctx.app.http,&endpoint,&[]).await{result[key]=v;}
    }
    ctx.json(&result).await
}
