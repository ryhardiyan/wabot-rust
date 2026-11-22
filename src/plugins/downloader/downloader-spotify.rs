crate::plugins::plugin!(&["spotify"], "downloader", "Unduh audio Spotify", execute);
async fn execute(ctx: &crate::plugins::Context) -> anyhow::Result<()> {
    super::platform(ctx, "sp").await
}
