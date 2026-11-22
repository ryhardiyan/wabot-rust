use crate::{
    bot_lib::{core::message, sticker},
    plugins::Context,
};
crate::plugins::plugin!(
    &["s", "stiker", "sticker"],
    "maker",
    "Konversi gambar/video ke WebP dengan metadata pack",
    execute
);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let mime = message::mime(ctx.m.media());
    let bytes = ctx.m.download(ctx.app.config.max_media_bytes).await?;
    let c = &ctx.app.config;
    let bytes = sticker::convert(
        bytes,
        mime.starts_with("video/"),
        c.stickpack.clone(),
        c.stickauth.clone(),
    )
    .await?;
    anyhow::ensure!(
        bytes.len() <= 1024 * 1024,
        "Stiker melebihi 1 MiB; gunakan video lebih pendek"
    );
    ctx.m
        .send_media(bytes, "image/webp", "sticker.webp", "")
        .await
}
