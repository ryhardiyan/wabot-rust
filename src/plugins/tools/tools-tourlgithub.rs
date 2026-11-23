use crate::{
    bot_lib::{core::message, uploader},
    plugins::Context,
};
use anyhow::Result;
crate::plugins::plugin!(
    &["tourlgh", "tourlgithub"],
    "tools",
    execute,
    owner_only = true
);
async fn execute(ctx: &Context) -> Result<()> {
    let mime = message::mime(ctx.m.media());
    anyhow::ensure!(!mime.is_empty(), "Kirim atau balas media");
    let bytes = ctx
        .m
        .download(ctx.app.config.max_media_bytes.min(25 * 1024 * 1024))
        .await?;
    ctx.reply(uploader::github(&ctx.app.http, &ctx.app.config, bytes, mime).await?)
        .await
}
