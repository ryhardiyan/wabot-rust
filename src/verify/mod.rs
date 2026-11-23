//! Mutual Contact Verification.
//!
//! Alur: user menyimpan kontak owner dengan nama tertentu -> mengirim vCard ->
//! bot memvalidasi lalu menandai `pending` dan mengirim notifikasi + kartu kontak
//! user ke owner -> owner membalas `.acc` (quote kartu / notifikasi, atau dengan
//! argumen nomor) -> bot menandai `verified`.

pub mod db;
pub mod vcard;

use crate::{
    bot_lib::core::smsg::MessageContext,
    config::clean_number,
    plugins::{App, Context},
};
use anyhow::{Context as _, Result};
use db::Status;
use std::sync::Arc;
use whatsapp_rust::{
    prelude::{wa, MessageExt, MessageField},
    Jid,
};

pub const ACCESS_UNREGISTERED: &str = "Akses ditolak! Anda belum terverifikasi. Silakan simpan nomor Owner terlebih dahulu dengan nama yang mengandung kata 'Rahardiyan', lalu kirimkan kartu kontak (vCard) Owner tersebut ke chat ini.";
pub const ACCESS_PENDING: &str = "Akun Anda sedang dalam antrean verifikasi oleh Owner. Mohon tunggu hingga Owner menyimpan nomor Anda dan menyetujuinya.";
pub const ACCESS_UNAVAILABLE: &str =
    "⏳ Sistem verifikasi sedang tidak tersedia. Silakan coba beberapa saat lagi.";
pub const CONTACT_INVALID: &str = "Verifikasi gagal! Pastikan Anda menyimpan nomor Owner yang benar dengan nama yang mengandung kata 'Rahardiyan', lalu kirim ulang kontaknya.";
pub const CONTACT_ACCEPTED: &str =
    "Kontak valid! Permintaan verifikasi Anda telah diteruskan ke Owner. Silakan tunggu konfirmasi.";
pub const APPROVED_OWNER: &str =
    "✅ Berhasil! User {} telah diverifikasi dan kini bisa menggunakan bot.";
pub const APPROVED_USER: &str = "🎉 *Selamat! Verifikasi Berhasil.*\n\nNomor Anda telah dikonfirmasi oleh Owner. Sekarang Anda sudah bisa menggunakan semua fitur bot!";

/// Nomor telepon kanonik pengirim (dari JID `s.whatsapp.net`, bukan LID).
pub fn sender_phone(m: &MessageContext) -> Option<String> {
    std::iter::once(m.sender.as_str())
        .chain(m.sender_alt.as_deref())
        .find(|s| s.ends_with("@s.whatsapp.net"))
        .map(clean_number)
        .filter(|s| !s.is_empty())
}

/// Owner dikenali dari JID konfigurasi, bukan hanya dari nomor telepon.
/// `OWNER_JID` dapat berupa JID LID, jadi perbandingan string tetap diperlukan.
pub fn is_owner(m: &MessageContext, app: &App) -> bool {
    if m.sender == app.config.owner_jid {
        return true;
    }
    sender_phone(m).is_some_and(|number| {
        app.config.owners.contains(&number) || number == app.config.owner_phone
    })
}

/// Middleware akses: mengembalikan alasan penolakan, atau `None` jika diperbolehkan.
pub async fn access_reason(app: &App, phone: Option<&str>, is_owner: bool) -> Option<String> {
    if is_owner {
        return None;
    }
    let jid = match phone {
        Some(number) => format!("{number}@s.whatsapp.net"),
        None => return Some(ACCESS_UNREGISTERED.into()),
    };
    match app.verify.status(&jid).await {
        Ok(Status::Verified) => None,
        Ok(Status::Pending) => Some(ACCESS_PENDING.into()),
        Ok(Status::Unregistered) => Some(ACCESS_UNREGISTERED.into()),
        Err(error) => {
            tracing::warn!(%error, "Gagal memeriksa status verifikasi");
            Some(ACCESS_UNAVAILABLE.into())
        }
    }
}

/// Mengambil string vCard dari sebuah pesan protobuf (kartu tunggal atau array).
pub fn card_from(message: &wa::Message) -> Option<String> {
    let base = message.get_base_message();
    base.contact_message
        .as_option()
        .and_then(|c| c.vcard.as_deref())
        .or_else(|| {
            base.contacts_array_message
                .as_option()
                .and_then(|c| c.contacts.first())
                .and_then(|c| c.vcard.as_deref())
        })
        .map(str::to_string)
}

/// Pesan yang di-quote oleh pengirim, termasuk bila yang di-quote adalah kartu kontak.
fn quoted_message(m: &MessageContext) -> Option<wa::Message> {
    let base = m.raw.get_base_message();
    let info = base
        .extended_text_message
        .as_option()
        .and_then(|v| v.context_info.as_option())
        .or_else(|| {
            base.contact_message
                .as_option()
                .and_then(|v| v.context_info.as_option())
        })
        .or_else(|| {
            base.contacts_array_message
                .as_option()
                .and_then(|v| v.context_info.as_option())
        })
        .or_else(|| {
            base.image_message
                .as_option()
                .and_then(|v| v.context_info.as_option())
        });
    info.and_then(|c| c.quoted_message.as_option().cloned())
        .or_else(|| m.quoted.as_ref().map(|q| q.message.as_ref().clone()))
}

/// Dipanggil lebih awal oleh handler untuk setiap pesan masuk.
/// Mengembalikan `true` jika pesan adalah kartu kontak dan sudah ditangani.
pub async fn handle_contact(app: &Arc<App>, m: &MessageContext) -> Result<bool> {
    let Some(card) = card_from(&m.raw) else {
        return Ok(false);
    };
    let parsed = vcard::parse(&card);
    let owner_phone = vcard::normalize_phone(&app.config.owner_phone);
    let required = app.config.required_name.to_ascii_lowercase();
    let name_ok = !required.is_empty() && parsed.name.to_ascii_lowercase().contains(&required);
    let phone_ok = !parsed.phone.is_empty() && parsed.phone == owner_phone;
    if !name_ok || !phone_ok {
        m.reply(CONTACT_INVALID).await?;
        return Ok(true);
    }
    let number = match sender_phone(m) {
        Some(value) => value,
        None => {
            m.reply(CONTACT_INVALID).await?;
            return Ok(true);
        }
    };
    let applicant = vcard::phone_jid(&number)?;
    if app.verify.status(&applicant).await? == Status::Verified {
        m.reply("✅ Nomor Anda sudah terverifikasi.").await?;
        return Ok(true);
    }
    app.verify.set_pending(&applicant).await?;

    let display = if m.push_name.is_empty() {
        number.clone()
    } else {
        m.push_name.clone()
    };
    let notice = format!(
        "🔔 *PERMINTAAN VERIFIKASI BARU*\n\nAda user yang ingin mendaftar. Silakan simpan nomor kontak di bawah ini secara manual di HP Anda, lalu reply pesan kontak ini dengan perintah *.acc* untuk menyetujui.\n\nNama: {display}\nNomor: {number}"
    );
    let owner = &app.config.owner_jid;
    m.socket.send_text(owner, &notice).await?;

    let mut contact = wa::Message {
        contact_message: MessageField::some(wa::message::ContactMessage {
            display_name: Some(display.clone()),
            vcard: Some(vcard::build(&display, &number)),
            ..Default::default()
        }),
        ..Default::default()
    };
    let quote = whatsapp_rust::wacore::proto_helpers::build_quote_context_with_info(
        &m.id,
        &m.sender.parse::<Jid>()?,
        &m.chat.parse::<Jid>()?,
        &owner.parse::<Jid>()?,
        &m.raw,
    );
    if !contact.set_context_info(quote) {
        tracing::debug!("ContactMessage tidak menerima context info; dikirim tanpa quote");
    }
    m.socket
        .client
        .send_message(owner.parse::<Jid>()?, contact)
        .await?;
    // Konfirmasi ke user dikirim paling akhir: jika notifikasi owner gagal,
    // user tidak menerima pesan "sudah diteruskan" yang palsu.
    m.reply(CONTACT_ACCEPTED).await?;
    Ok(true)
}

/// Menentukan target persetujuan dari argumen teks, kartu kontak yang di-reply,
/// teks notifikasi yang di-reply, mention, atau pengirim pesan yang di-quote.
fn resolve_target(ctx: &Context) -> Option<String> {
    if !ctx.arg(0).is_empty() {
        return Some(ctx.arg(0).to_string());
    }
    if let Some(quoted) = quoted_message(&ctx.m) {
        if let Some(card) = card_from(&quoted) {
            let phone = vcard::parse(&card).phone;
            if !phone.is_empty() {
                return Some(phone);
            }
        }
        let text = crate::bot_lib::core::message::text(&quoted);
        if let Some(phone) = vcard::phone_from_notice(&text) {
            return Some(phone);
        }
    }
    if let Some(jid) = ctx.m.mentions.first() {
        let phone = clean_number(jid);
        if !phone.is_empty() {
            return Some(phone);
        }
    }
    let quoted_sender = ctx
        .m
        .quoted
        .as_ref()
        .map(|q| q.sender.as_str())
        .filter(|s| !s.is_empty())?;
    let phone = clean_number(quoted_sender);
    let owner_phone = vcard::normalize_phone(&ctx.app.config.owner_phone);
    (!phone.is_empty() && phone != owner_phone).then_some(phone)
}

/// Eksekusi perintah `.acc` (hanya owner).
pub async fn approve(ctx: &Context) -> Result<()> {
    anyhow::ensure!(ctx.is_owner, "Perintah ini khusus owner.");
    let target = resolve_target(ctx).context(
        "Tentukan nomor user (contoh: .acc 08123456789) atau balas kartu kontak/notifikasi bot",
    )?;
    let jid = vcard::phone_jid(&target)?;
    // Spesifikasi memperbolehkan persetujuan langsung dari argumen nomor atau
    // kartu yang di-reply, jadi status `pending` tidak dijadikan syarat.
    let updated = ctx.app.verify.set_verified(&jid).await?;
    anyhow::ensure!(updated, "Gagal menyimpan status terverifikasi untuk {jid}");

    // Menjaga konsistensi database JSON lama yang dipakai plugin lain.
    let jid_for_json = jid.clone();
    if let Err(error) = ctx
        .app
        .db
        .update(move |d| {
            let id = d.resolve(&jid_for_json);
            let user = d
                .users
                .get_mut(&id)
                .context("Data user JSON tidak ditemukan")?;
            user["registered"] = serde_json::json!(true);
            user["registeredAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            Ok(())
        })
        .await
    {
        tracing::warn!(%error, jid = %jid, "Sinkronisasi status JSON gagal");
    }

    ctx.reply(APPROVED_OWNER.replace("{}", &jid)).await?;
    ctx.m.socket.send_text(&jid, APPROVED_USER).await?;
    Ok(())
}
