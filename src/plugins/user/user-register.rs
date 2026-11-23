use crate::plugins::Context;
use anyhow::{Context as _, Result};

crate::plugins::plugin!(&["register", "reg", "daftar", "cekreg"], "user", execute);
async fn execute(ctx: &Context) -> Result<()> {
    let data = ctx.app.db.snapshot().await;
    let user = data
        .users
        .get(&data.resolve(&ctx.m.sender))
        .context("User belum tercatat")?;
    if ctx.command == "cekreg" {
        let number = user["number"]
            .as_str()
            .or_else(|| user["phone"].as_str())
            .or_else(|| user["jid"].as_str())
            .unwrap_or(&ctx.m.sender);
        let registered = user["registered"].as_bool() == Some(true);
        let since = user["registeredAt"].as_str().unwrap_or("-");
        return ctx
            .reply(format!(
                "━━━━━━━━━━━━━━━━━━━━━━━━\n\
                 *STATUS PENDAFTARAN*\n\
                 ━━━━━━━━━━━━━━━━━━━━━━━━\n\
                 📱 Nomor  : `{number}`\n\
                 ✅ Status : *{}*\n\
                 📅 Sejak  : {since}",
                if registered {
                    "Terdaftar"
                } else {
                    "Belum terdaftar"
                }
            ))
            .await;
    }
    if user["registered"] == true {
        return ctx.reply("✅ Nomor Anda sudah terdaftar.").await;
    }
    let owner = &ctx.app.config.owner_phone;
    let required = &ctx.app.config.required_name;
    ctx.reply(format!(
        "📱 *Verifikasi bot*\n\n1. Simpan nomor owner {owner} di HP Anda dengan nama yang mengandung kata *{required}*.\n2. Kirim kartu kontak (vCard) owner tersebut ke chat ini.\n3. Setelah cocok, owner akan menyetujui permintaan Anda dan Anda mendapat notifikasi."
    ))
    .await
}
