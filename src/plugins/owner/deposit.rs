use crate::{bot_lib::downloader, plugins::Context};
use anyhow::{Context as _, Result};
use serde_json::json;
crate::plugins::plugin!(
    &["depo"],
    "owner",
    "Buat QRIS deposit dan pantau status",
    execute
);
async fn execute(ctx: &Context) -> Result<()> {
    let nominal = ctx
        .text
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>()
        .parse::<u32>()?;
    anyhow::ensure!(
        (1000..=499000).contains(&nominal),
        "Deposit Rp1.000–499.000"
    );
    let key = ctx.app.config.required("PAYMENTKU_API_KEY")?;
    let v = downloader::json(
        ctx.app
            .http
            .post("https://tools.techwiz.id/api/paymentku/create")
            .header("X-API-Key", key)
            .json(&json!({"nominal":nominal,"apiKey":key}))
            .send()
            .await?,
    )
    .await?;
    anyhow::ensure!(v["success"] == true, "Gateway gagal membuat pembayaran");
    let pay = &v["payment"];
    let id = pay["id"].as_str().context("Payment ID kosong")?;
    let qr = qrcode::QrCode::new(
        pay["qrisString"]
            .as_str()
            .context("QRIS kosong")?
            .as_bytes(),
    )?
    .render::<image::Luma<u8>>()
    .min_dimensions(512, 512)
    .build();
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(qr).write_to(&mut bytes, image::ImageFormat::Png)?;
    ctx.m
        .send_media(
            bytes.into_inner(),
            "image/png",
            "qris.png",
            &format!(
                "💳 Deposit QRIS\nID: {id}\nNominal: {nominal}\nTotal: {}\nBatas waktu: {}",
                pay["finalAmount"], pay["expiresAt"]
            ),
        )
        .await?;
    for _ in 0..120 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let result = downloader::json(
            ctx.app
                .http
                .get("https://tools.techwiz.id/api/paymentku/status")
                .header("X-API-Key", key)
                .query(&[("paymentId", id), ("apiKey", key)])
                .send()
                .await?,
        )
        .await?;
        match result.pointer("/payment/status").and_then(|v| v.as_str()) {
            Some("success") => {
                return ctx
                    .reply(format!(
                        "✅ Gateway mengonfirmasi pembayaran {id}. Saldo menunggu pemrosesan."
                    ))
                    .await
            }
            Some("expired" | "failed" | "cancelled") => {
                return ctx
                    .reply(format!(
                        "Pembayaran {id} berakhir: {}",
                        result["payment"]["status"]
                    ))
                    .await
            }
            _ => (),
        }
    }
    ctx.reply(format!(
        "Waktu pemantauan pembayaran {id} habis; cek gateway sebelum membayar ulang."
    ))
    .await
}
