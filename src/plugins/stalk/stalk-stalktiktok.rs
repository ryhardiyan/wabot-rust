use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
crate::plugins::plugin!(
    &["ttstalk", "tiktokstalk", "ttstalker", "stalktt"],
    "stalk",
    execute
);
pub(super) async fn lookup(ctx: &Context, username: &str) -> Result<()> {
    let username = username.trim_start_matches('@');
    anyhow::ensure!(
        !username.is_empty()
            && username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._".contains(&b)),
        "Masukkan username TikTok"
    );
    let html = String::from_utf8(
        downloader::bounded(
            ctx.app
                .http
                .get(format!("https://www.tiktok.com/@{username}"))
                .header(
                    "User-Agent",
                    "Mozilla/5.0 (Linux; Android 14) Chrome/130.0 Mobile Safari/537.36",
                )
                .send()
                .await?,
            4 * 1024 * 1024,
        )
        .await?,
    )?;
    let value = {
        let doc = scraper::Html::parse_document(&html);
        let selector = scraper::Selector::parse(
            "script#__UNIVERSAL_DATA_FOR_REHYDRATION__, script#SIGI_STATE",
        )
        .map_err(|_| anyhow::anyhow!("Selector TikTok tidak valid"))?;
        let script = doc
            .select(&selector)
            .next()
            .context("TikTok tidak mengembalikan data profil publik")?;
        serde_json::from_str::<serde_json::Value>(&script.inner_html())?
    };
    let user = value
        .pointer("/__DEFAULT_SCOPE__/webapp.user-detail/userInfo")
        .or_else(|| value.pointer("/DEFAULT_SCOPE/webapp.user-detail/userInfo"))
        .or_else(|| value.get("UserModule"))
        .context("TikTok user tidak ditemukan")?;
    let detail = user.get("user").unwrap_or(user);
    let stats = &user["stats"];
    let str_val = |key: &str| detail[key].as_str().unwrap_or("-");
    let num = |key: &str| stats[key].as_u64().unwrap_or(0);
    let text = format!(
        "━━━━━━━━━━━━━━━━━━━━━━━━\n\
         *PROFIL TIKTOK*\n\
         ━━━━━━━━━━━━━━━━━━━━━━━━\n\
         👤 Username : *@{}*\n\
         📛 Nama     : {}\n\
         ✅ Terverifikasi: {}\n\
         📝 Bio      : {}\n\
         ❤️ Followers : *{}*\n\
         ➡️ Following : *{}*\n\
         💚 Likes    : *{}*\n\
         🎬 Video    : *{}*",
        str_val("uniqueId"),
        str_val("nickname"),
        if detail["verified"].as_bool() == Some(true) {
            "Ya"
        } else {
            "Tidak"
        },
        str_val("signature"),
        num("followerCount"),
        num("followingCount"),
        num("heartCount"),
        num("videoCount")
    );
    ctx.reply(text).await
}
async fn execute(ctx: &Context) -> Result<()> {
    lookup(ctx, &ctx.text).await
}
