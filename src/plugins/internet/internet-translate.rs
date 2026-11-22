use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
crate::plugins::plugin!(
    &["translate", "tr", "terjemah", "terjemahkan"],
    "internet",
    "Terjemahkan teks ke kode bahasa tujuan",
    execute
);
async fn execute(ctx: &Context) -> Result<()> {
    let (lang, text) = ctx.text.split_once(char::is_whitespace).unwrap_or((
        ctx.text.as_str(),
        ctx.m.quoted.as_ref().map(|q| q.text.as_str()).unwrap_or(""),
    ));
    anyhow::ensure!(
        !lang.is_empty() && !text.is_empty(),
        "/translate id <teks> atau balas pesan"
    );
    let v = downloader::get_json(
        &ctx.app.http,
        "https://translate.googleapis.com/translate_a/single",
        &[
            ("client", "gtx"),
            ("sl", "auto"),
            ("tl", lang),
            ("dt", "t"),
            ("q", text),
        ],
    )
    .await?;
    let out = v[0]
        .as_array()
        .context("Terjemahan kosong")?
        .iter()
        .filter_map(|v| v[0].as_str())
        .collect::<String>();
    ctx.reply(out).await
}
