crate::plugins::plugin!(&["tt", "tiktok"], "downloader", execute);
async fn execute(ctx: &crate::plugins::Context) -> anyhow::Result<()> {
    super::platform(ctx, "tt").await
}
