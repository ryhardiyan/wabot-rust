crate::plugins::plugin!(
    &["tt", "tiktok"],
    "downloader",
    "Unduh video/slideshow TikTok",
    execute
);
async fn execute(ctx: &crate::plugins::Context) -> anyhow::Result<()> {
    super::platform(ctx, "tt").await
}
