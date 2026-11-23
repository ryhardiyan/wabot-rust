use crate::plugins::Context;
use anyhow::{Context as _, Result};
use whatsapp_rust::NewsletterVerification;
crate::plugins::plugin!(&["stalk"], "stalk", execute);

async fn execute(ctx: &Context) -> Result<()> {
    let client = &ctx.m.socket.client;
    match ctx.arg(0) {
        "-tt" => super::stalk_stalktiktok::lookup(ctx, ctx.arg(1)).await,
        "-g" => {
            let groups = client.groups();
            let group = if ctx.arg(1).is_empty() {
                anyhow::ensure!(
                    ctx.m.is_group,
                    "Gunakan di grup atau sertakan link undangan"
                );
                groups.get_metadata(&ctx.m.chat.parse()?).await?
            } else {
                let url = reqwest::Url::parse(ctx.arg(1))?;
                anyhow::ensure!(
                    url.host_str() == Some("chat.whatsapp.com"),
                    "Link grup tidak valid"
                );
                groups
                    .get_invite_info(url.path().trim_start_matches('/'))
                    .await?
            };
            let admins = group.participants.iter().filter(|p| p.is_admin()).count();
            let text = format!(
                "━━━━━━━━━━━━━━━━━━━━━━━━\n\
                 *{}\n\
                 ━━━━━━━━━━━━━━━━━━━━━━━━\n\
                 🆔 ID      : `{}`\n\
                 👥 Anggota : *{}*\n\
                 🛡️ Admin   : *{}*",
                group.subject,
                group.id,
                group.participants.len(),
                admins
            );
            ctx.reply(text).await
        }
        "-c" => {
            let url = reqwest::Url::parse(ctx.arg(1))?;
            anyhow::ensure!(
                url.host_str() == Some("whatsapp.com")
                    || url.host_str() == Some("www.whatsapp.com"),
                "URL channel tidak valid"
            );
            let code = url
                .path()
                .strip_prefix("/channel/")
                .context("URL harus /channel/<code>")?;
            let meta = client.newsletter().get_metadata_by_invite(code).await?;
            let verified = matches!(meta.verification, NewsletterVerification::Verified);
            let text = format!(
                "━━━━━━━━━━━━━━━━━━━━━━━━\n\
                 *CHANNEL WHATSAPP*\n\
                 ━━━━━━━━━━━━━━━━━━━━━━━━\n\
                 📢 Nama     : *{}*\n\
                 📝 Deskripsi: {}\n\
                 👥 Subscriber: *{}*\n\
                 ✅ Terverifikasi: {}\n\
                 🔗 Undangan : {}\n\
                 🆔 JID      : `{}`",
                meta.name,
                meta.description.as_deref().unwrap_or("-"),
                meta.subscriber_count,
                if verified { "Ya" } else { "Tidak" },
                meta.invite_code.as_deref().unwrap_or("-"),
                meta.jid
            );
            ctx.reply(text).await
        }
        _ => {
            let target = ctx
                .m
                .mentions
                .first()
                .cloned()
                .or_else(|| ctx.m.quoted.as_ref().map(|q| q.sender.clone()))
                .unwrap_or_else(|| {
                    format!("{}@s.whatsapp.net", crate::config::clean_number(&ctx.text))
                });
            let jid = target.parse::<whatsapp_rust::Jid>()?;
            let info = client
                .contacts()
                .get_user_info(std::slice::from_ref(&jid))
                .await?;
            let business = client.get_business_profile(&jid).await?;
            let detail = info.get(&jid);
            let name = jid.user.clone();
            let status = detail.and_then(|d| d.status.as_deref()).unwrap_or("-");
            let devices = detail.map(|d| d.devices.clone()).unwrap_or_default();
            let mut text = format!(
                "━━━━━━━━━━━━━━━━━━━━━━━━\n\
                 *{}\n\
                 ━━━━━━━━━━━━━━━━━━━━━━━━\n\
                 🆔 JID      : `{}`\n\
                 💬 Status   : {}\n\
                 🤖 Bisnis   : {}\n\
                 📱 Perangkat: {}",
                name,
                jid,
                status,
                if detail.map(|d| d.is_business).unwrap_or(false) {
                    "Ya"
                } else {
                    "Tidak"
                },
                if devices.is_empty() {
                    "-".into()
                } else {
                    devices
                        .iter()
                        .map(|d| d.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            );
            if let Some(profile) = business.as_ref() {
                if !profile.description.is_empty() {
                    text.push_str(&format!("\n📄 Deskripsi: {}", profile.description));
                }
                if let Some(email) = profile.email.as_deref() {
                    text.push_str(&format!("\n📧 Email    : {email}"));
                }
                if !profile.website.is_empty() {
                    text.push_str(&format!("\n🌐 Web      : {}", profile.website.join(", ")));
                }
            }
            ctx.reply(text).await
        }
    }
}
