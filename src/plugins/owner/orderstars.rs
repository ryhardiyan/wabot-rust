use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
use serde_json::json;
crate::plugins::plugin!(
    &["orderstars"],
    "owner",
    "Buat pesanan Telegram Stars dan pantau pembayaran",
    execute
);
async fn execute(ctx: &Context) -> Result<()> {
    let (user, count) = ctx
        .text
        .split_once(',')
        .context("/orderstars username,50")?;
    let amount = count.trim().parse::<u32>()?;
    anyhow::ensure!(
        amount >= 50 && !user.trim().is_empty(),
        "Minimal 50 Stars dan username wajib diisi"
    );
    let check = downloader::get_json(
        &ctx.app.http,
        "https://fund.tg/api/verify-username",
        &[("username", user.trim())],
    )
    .await?;
    anyhow::ensure!(check["exists"] == true, "Username tidak ditemukan");
    let value = downloader::json(
        ctx.app
            .http
            .post("https://fund.tg/api/create-payment")
            .header("Referer", "https://fund.tg/")
            .json(&json!({"username":user.trim(),"amount":amount}))
            .send()
            .await?,
    )
    .await?;
    let payment = value["paymentUrl"].as_str().context("Payment URL kosong")?;
    let status = value["statusUrl"].as_str().context("Status URL kosong")?;
    let url = reqwest::Url::parse(status)?;
    anyhow::ensure!(
        url.scheme() == "https"
            && url
                .host_str()
                .is_some_and(|h| h == "fund.tg" || h.ends_with(".fund.tg")),
        "Gateway status URL tidak valid"
    );
    ctx.reply(format!(
        "✨ Pesanan {}\n{} Stars untuk @{}\nHarga: {}\nBayar: {}",
        value["externalId"],
        amount,
        user.trim(),
        value["price"],
        payment
    ))
    .await?;
    for _ in 0..30 {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        let v = downloader::get_json(&ctx.app.http, status, &[]).await?;
        if v.pointer("/invoice/status").and_then(|v| v.as_str()) == Some("TRANSFERRED") {
            return ctx.json(&v["invoice"]).await;
        }
    }
    ctx.reply("Pemantauan berakhir setelah 5 menit. Periksa tautan status pesanan.")
        .await
}
