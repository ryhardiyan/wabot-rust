use crate::{bot_lib::downloader, plugins::Context};
crate::plugins::plugin!(
    &["ccgen", "cardgen"],
    "maker",
    "Generator nomor kartu uji dari LambdaTest",
    execute
);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let amount = ctx
        .arg(1)
        .parse::<u32>()
        .unwrap_or(1)
        .clamp(1, 10)
        .to_string();
    let typ = if ctx.arg(0).is_empty() {
        "visa"
    } else {
        ctx.arg(0)
    };
    let v = downloader::get_json(
        &ctx.app.http,
        "https://backend.lambdatest.com/api/dev-tools/credit-card-generator",
        &[("type", typ), ("no-of-cards", &amount)],
    )
    .await?;
    ctx.json(&v).await
}
