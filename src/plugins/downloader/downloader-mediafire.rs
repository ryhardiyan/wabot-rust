crate::plugins::plugin!(
    &["mediafire"],
    "downloader",
    "Unduh file MediaFire",
    execute
);
async fn execute(ctx: &crate::plugins::Context) -> anyhow::Result<()> {
    super::platform(ctx, "mf").await
}
