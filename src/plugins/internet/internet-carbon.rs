use crate::{bot_lib::downloader, plugins::Context};
crate::plugins::plugin!(
    &["carbon", "carbonara", "codeimg", "toviewcode"],
    "internet",
    execute
);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let text = ctx
        .m
        .quoted
        .as_ref()
        .map(|q| q.text.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(&ctx.text);
    anyhow::ensure!(
        !text.is_empty() && text.len() <= 16000,
        "/carbon <kode maksimal 16000 byte>"
    );
    let response = ctx
        .app
        .http
        .post("https://carbonara.solopov.dev/api/cook")
        .json(&serde_json::json!({"code":text}))
        .send()
        .await?;
    let data = downloader::bounded(response, ctx.app.config.max_media_bytes).await?;
    let caption = format!("*{}* · Carbon", ctx.app.config.brand());
    ctx.m
        .send_media(data, "image/png", "carbon.png", &caption)
        .await
}
