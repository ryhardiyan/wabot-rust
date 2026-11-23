use crate::{bot_lib::downloader, plugins::Context};
crate::plugins::plugin!(&["ghstalk", "githubstalk", "stalkgh"], "stalk", execute);

async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let user = ctx.arg(0);
    anyhow::ensure!(
        !user.is_empty() && user.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "/ghstalk <username>"
    );
    let v = downloader::get_json(
        &ctx.app.http,
        &format!("https://api.github.com/users/{user}"),
        &[],
    )
    .await?;
    anyhow::ensure!(v["message"].is_null(), "User GitHub tidak ditemukan");

    let get = |key: &str| v[key].as_str().unwrap_or("-");
    let num = |key: &str| v[key].as_u64().unwrap_or(0);
    let text = format!(
        "━━━━━━━━━━━━━━━━━━━━━━━━\n\
         *PROFIL GITHUB*\n\
         ━━━━━━━━━━━━━━━━━━━━━━━━\n\
         👤 Username : *{}*\n\
         📛 Nama     : {}\n\
         🏢 Perusahaan: {}\n\
         📍 Lokasi   : {}\n\
         📝 Bio      : {}\n\
         📦 Repo     : *{}*\n\
         👥 Followers: *{}*\n\
         ➡️ Following : *{}*\n\
         🔗 Profil   : {}",
        get("login"),
        get("name"),
        get("company"),
        get("location"),
        get("bio"),
        num("public_repos"),
        num("followers"),
        num("following"),
        get("html_url")
    );
    ctx.reply(text).await
}
