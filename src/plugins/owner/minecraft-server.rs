use crate::{bot_lib::downloader, plugins::Context};
crate::plugins::plugin!(&["server"], "owner", "Status Minecraft Java", execute);
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
    ctx.json(&value).await
}
