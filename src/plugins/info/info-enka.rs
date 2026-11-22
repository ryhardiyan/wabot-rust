use crate::{bot_lib::downloader, plugins::Context};
crate::plugins::plugin!(
    &["enka", "genshin", "gi"],
    "info",
    "Profil Genshin Enka",
    execute
);
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
    ctx.json(&v).await
}
