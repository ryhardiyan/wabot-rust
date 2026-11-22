use crate::plugins::Context;
use anyhow::{Context as _, Result};
crate::plugins::plugin!(
    &["stalk"],
    "stalk",
    "Info publik WhatsApp/grup/channel/TikTok",
    execute
);
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
            ctx.reply(format!(
                "*{}*\nID: {}\nAnggota: {}\nAdmin: {}",
                group.subject,
                group.id,
                group.participants.len(),
                group.participants.iter().filter(|p| p.is_admin()).count()
            ))
            .await
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
            ctx.reply(format!("{meta:#?}")).await
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
            let text =
                format!("WhatsApp: {jid}\nProfil: {info:#?}\nBusiness profile: {business:#?}");
            ctx.reply(text).await
        }
    }
}
