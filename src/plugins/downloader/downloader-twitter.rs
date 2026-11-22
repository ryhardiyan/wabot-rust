crate::plugins::plugin!(
    &["x", "twitter"],
    "downloader",
    "Unduh media X/Twitter",
    execute
);
async fn execute(ctx: &crate::plugins::Context) -> anyhow::Result<()> {
    super::platform(ctx, "tw").await
}
