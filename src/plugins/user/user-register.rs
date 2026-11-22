use crate::{
    bot_lib::{core::smsg::MessageContext, google_contacts},
    config::clean_number,
    plugins::{App, Context},
};
use anyhow::{Context as _, Result};
use serde_json::{json, Value};
use std::sync::Arc;

crate::plugins::plugin!(
    &["register", "reg", "daftar", "cekreg"],
    "user",
    "Registrasi dan status akun",
    execute
);
async fn execute(ctx: &Context) -> Result<()> {
    let data = ctx.app.db.snapshot().await;
    let user = data
        .users
        .get(&data.resolve(&ctx.m.sender))
        .context("User belum tercatat")?;
    if ctx.command == "cekreg" {
        return ctx.json(user).await;
    }
    if user["registered"] == true {
        return ctx.reply("✅ Nomor Anda sudah terdaftar.").await;
    }
    let owner = ctx
        .app
        .config
        .env
        .get("OWNER_NUMBER")
        .map(String::as_str)
        .unwrap_or("082323780821");
    ctx.reply(format!("📱 Registrasi bot\nSimpan nomor owner {owner} dengan nama Rahardiyan, lalu kirim kartu kontak owner ke sini. Setelah kartu cocok, nomor Anda disimpan melalui Google Contacts.")).await
}
pub async fn handle_contact(app: &Arc<App>, m: &MessageContext, user: &Value) -> Result<bool> {
    use whatsapp_rust::prelude::MessageExt;
    let base = m.raw.get_base_message();
    let card = base
        .contact_message
        .as_option()
        .and_then(|c| c.vcard.as_deref())
        .or_else(|| {
            base.contacts_array_message
                .as_option()
                .and_then(|c| c.contacts.first())
                .and_then(|c| c.vcard.as_deref())
        });
    let Some(card) = card else { return Ok(false) };
    if user["registered"] == true {
        return Ok(true);
    }
    let (name, phone) = parse_vcard(card);
    let owner = app
        .config
        .env
        .get("OWNER_NUMBER")
        .map(String::as_str)
        .unwrap_or("082323780821");
    if clean_number(&phone) != clean_number(owner) || !name.to_lowercase().contains("rahardiyan") {
        m.reply("Kartu kontak harus berisi nomor owner dengan nama Rahardiyan.")
            .await?;
        return Ok(true);
    }
    let phone = user["phone"]
        .as_str()
        .filter(|s| !s.is_empty())
        .context("Nomor telepon pengirim belum terpetakan dari LID")?;
    google_contacts::verify_and_save(&app.http, &app.config, &m.push_name, phone).await?;
    app.db
        .update(|d| {
            let id = d.resolve(&m.sender);
            let u = d.users.get_mut(&id).context("User hilang")?;
            u["registered"] = json!(true);
            u["registeredAt"] = json!(chrono::Utc::now().to_rfc3339());
            Ok(())
        })
        .await?;
    m.reply("✅ Registrasi berhasil; kontak Anda tersimpan di Google Contacts.")
        .await?;
    Ok(true)
}
fn parse_vcard(card: &str) -> (String, String) {
    let mut name = String::new();
    let mut phone = String::new();
    for line in card.lines() {
        if let Some((key, value)) = line.split_once(':') {
            if key.to_uppercase().starts_with("FN") {
                name = value.trim().into();
            }
            if key.to_uppercase().starts_with("TEL") {
                phone = key
                    .split(';')
                    .find_map(|x| x.strip_prefix("waid="))
                    .unwrap_or(value)
                    .into();
            }
        }
    }
    (name, clean_number(&phone))
}
#[cfg(test)]
mod tests {
    #[test]
    fn vcard() {
        assert_eq!(
            super::parse_vcard("FN:Rahardiyan\nTEL;waid=628123456789:+62 8123456789").1,
            "628123456789"
        );
    }
}
