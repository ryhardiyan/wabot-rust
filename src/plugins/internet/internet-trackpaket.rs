use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
crate::plugins::plugin!(&["resi", "cekresi", "track"], "internet", execute);
async fn execute(ctx: &Context) -> Result<()> {
    let resi = ctx.arg(0);
    anyhow::ensure!(!resi.is_empty(), "/resi <nomor resi> <kurir>");
    let courier = if !ctx.arg(1).is_empty() {
        ctx.arg(1)
    } else if resi.starts_with("SPX") {
        "spx"
    } else if resi.starts_with("JP") {
        "jnt"
    } else if resi.starts_with("TKP") {
        "tokopedia"
    } else {
        anyhow::bail!("Tentukan kurir: jne, jnt, spx, sicepat, pos, tiki, wahana, dll.")
    };
    let mut page = reqwest::Url::parse("https://www.cekpengiriman.com/cek-resi")?;
    page.query_pairs_mut()
        .append_pair("resi", resi)
        .append_pair("kurir", courier);
    let html = String::from_utf8(
        downloader::bounded(
            ctx.app.http.get(page.clone()).send().await?,
            2 * 1024 * 1024,
        )
        .await?,
    )?;
    let re = regex::Regex::new(r#"formData\.append\("token",\s*"([^"]+)"\)"#)?;
    let token = re
        .captures(&html)
        .and_then(|c| c.get(1))
        .context("Token cek resi kosong")?
        .as_str();
    let html=String::from_utf8(downloader::bounded(ctx.app.http.post("https://www.cekpengiriman.com/wp-content/themes/simple/includes/widget/resultResi.php").header("referer",page.as_str()).form(&[("token",token),("resi",resi),("kurir",courier)]).send().await?,2*1024*1024).await?)?;
    let text = {
        let doc = scraper::Html::parse_document(&html);
        doc.root_element()
            .text()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    anyhow::ensure!(!text.is_empty(), "Resi tidak ditemukan");
    let label = courier.to_ascii_uppercase();
    let text = format!("*CEK RESI — {label}*\n`{resi}` · {courier}\n\n{text}");
    ctx.reply(text.chars().take(12000).collect::<String>())
        .await
}
