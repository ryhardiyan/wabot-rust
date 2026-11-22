use crate::{bot_lib::downloader, plugins::Context};
crate::plugins::plugin!(
    &["cekganda"],
    "info",
    "Cek akun Mobile Legends Mobapay",
    execute
);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    anyhow::ensure!(
        !ctx.arg(0).is_empty() && !ctx.arg(1).is_empty(),
        "/cekganda <user_id> <zone_id>"
    );
    let v = downloader::get_json(
        &ctx.app.http,
        "https://api.mobapay.com/api/app_shop",
        &[
            ("app_id", "100000"),
            ("user_id", ctx.arg(0)),
            ("server_id", ctx.arg(1)),
            ("country", "ID"),
            ("language", "id"),
        ],
    )
    .await?;
    ctx.json(&v).await
}
