use crate::{
    bot_lib::{downloader, sticker},
    plugins::Context,
};
crate::plugins::plugin!(&["brat"], "maker", execute);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let text = if ctx.text.is_empty() {
        ctx.m.quoted.as_ref().map(|q| q.text.as_str()).unwrap_or("")
    } else {
        &ctx.text
    };
    anyhow::ensure!(
        !text.is_empty() && text.len() <= 1000,
        "/brat [--anime|--video|--gif] <teks>"
    );
    let (endpoint, text, video) = if let Some(t) = text.strip_prefix("--anime ") {
        ("bratAnime", t, false)
    } else if let Some(t) = text.strip_prefix("--video ") {
        ("bratAnime", t, true)
    } else if let Some(t) = text.strip_prefix("--gif ") {
        ("bratGif", t, true)
    } else {
        ("brat", text, false)
    };
    let key = ctx
        .app
        .config
        .env
        .get("ANABOT_API_KEY")
        .map(String::as_str)
        .unwrap_or("freeApikey");
    let response = ctx
        .app
        .http
        .get(format!("https://anabot.my.id/api/maker/{endpoint}"))
        .query(&[
            ("text", text),
            ("video", if video { "true" } else { "false" }),
            ("apikey", key),
        ])
        .send()
        .await?;
    let bytes = downloader::bounded(response, ctx.app.config.max_media_bytes).await?;
    let bytes = sticker::convert(
        bytes,
        video,
        ctx.app.config.stickpack.clone(),
        ctx.app.config.stickauth.clone(),
    )
    .await?;
    ctx.m.send_media(bytes, "image/webp", "brat.webp", "").await
}
