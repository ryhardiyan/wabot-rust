use crate::{bot_lib::downloader, plugins::Context};
crate::plugins::plugin!(&["cekganda"], "info", execute);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let (user, zone) = (ctx.arg(0), ctx.arg(1));
    anyhow::ensure!(
        !user.is_empty()
            && !zone.is_empty()
            && user.bytes().all(|b| b.is_ascii_digit())
            && zone.bytes().all(|b| b.is_ascii_digit()),
        "Gunakan /cekganda <user_id> <zone_id> (angka saja)"
    );
    let v = downloader::get_json(
        &ctx.app.http,
        "https://api.mobapay.com/api/app_shop",
        &[
            ("app_id", "100000"),
            ("user_id", user),
            ("server_id", zone),
            ("country", "ID"),
            ("language", "id"),
        ],
    )
    .await?;
    ctx.json(&v).await
}
