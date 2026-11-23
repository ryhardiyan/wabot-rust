use crate::{
    bot_lib::{core::message, uploader},
    plugins::Context,
};
use anyhow::Result;
crate::plugins::plugin!(&["tourl", "url", "upload"], "tools", execute);
async fn execute(ctx: &Context) -> Result<()> {
    let mime = message::mime(ctx.m.media());
    anyhow::ensure!(!mime.is_empty(), "Kirim atau balas media untuk upload");
    let bytes = ctx.m.download(ctx.app.config.max_media_bytes).await?;
    let url = uploader::upload(&crate::bot_lib::downloader::client()?, bytes, mime).await?;
    ctx.reply(url).await
}
