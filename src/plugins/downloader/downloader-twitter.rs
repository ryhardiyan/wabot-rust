crate::plugins::plugin!(&["x", "twitter"], "downloader", execute);
async fn execute(ctx: &crate::plugins::Context) -> anyhow::Result<()> {
    super::platform(ctx, "tw").await
}
