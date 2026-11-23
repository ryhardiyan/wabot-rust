use crate::{bot_lib::downloader, plugins::Context};
use serde_json::Value;
crate::plugins::plugin!(&["enka", "genshin", "gi"], "info", execute);

fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut skip = false;
    for c in s.chars() {
        match c {
            '<' => skip = true,
            '>' => skip = false,
            _ if !skip => out.push(c),
            _ => (),
        }
    }
    out
}

fn profile_text(v: &Value, uid: &str) -> Option<String> {
    let info = v.get("playerInfo")?;
    let nick = strip_html(info["nickname"].as_str()?);
    let level = info["level"].as_u64().unwrap_or(0);
    let signature = info["signature"]
        .as_str()
        .map(strip_html)
        .unwrap_or_default();
    let namecard = v["playerInfo"]["namecardId"].as_u64().unwrap_or(0);
    let characters = v["characters"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|c| c["name"].as_str().or_else(|| c["id"].as_str()))
                .take(8)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "-".into());
    let mut text = format!(
        "━━━━━━━━━━━━━━━━━━━━━━━━\n\
         *PROFIL GENSHIN*\n\
         ━━━━━━━━━━━━━━━━━━━━━━━━\n\
         👤 Nama   : *{nick}*\n\
         🆔 UID     : `{uid}`\n\
         ⭐ Level   : *{level}*\n\
         🖼️ Namecard: {namecard}\n\
         ⚔️ Karakter: {characters}"
    );
    if !signature.is_empty() {
        text.push_str(&format!("\n📝 Bio     : {signature}"));
    }
    Some(text)
}

async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let uid = ctx.arg(0);
    anyhow::ensure!(
        uid.len() == 9 && uid.bytes().all(|b| b.is_ascii_digit()),
        "Gunakan /enka <UID 9 digit>"
    );
    let v = downloader::get_json(
        &ctx.app.http,
        &format!("https://enka.network/api/uid/{uid}/"),
        &[],
    )
    .await?;
    match profile_text(&v, uid) {
        Some(text) => ctx.reply(text).await,
        None => ctx.json(&v).await,
    }
}
