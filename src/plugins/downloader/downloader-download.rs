use crate::{bot_lib::downloader, plugins::Context};
use anyhow::Context as _;
crate::plugins::plugin!(&["download", "dl"], "downloader", execute);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let text = if ctx.text.is_empty() {
        ctx.m.quoted.as_ref().map(|q| q.text.as_str()).unwrap_or("")
    } else {
        &ctx.text
    };
    let (kind, _) = downloader::ext(text).context("Masukkan URL unduhan")?;
    super::platform(ctx, kind).await
}
