use crate::plugins::Context;
use anyhow::{Context as _, Result};
crate::plugins::plugin!(
    &["g", "group"],
    "group",
    "Buka/tutup, anggota, dan link grup",
    execute,
    group_only = true,
    admin_only = true,
    bot_admin = true
);
async fn execute(ctx: &Context) -> Result<()> {
    ctx.app.store.groups.remove(&ctx.m.chat).await;
    let jid = ctx.m.chat.parse::<whatsapp_rust::Jid>()?;
    let client = &ctx.m.socket.client;
    let groups = client.groups();
    match ctx.arg(0) {
        "o" | "open" => groups.set_announce(jid, false).await?,
        "c" | "close" => groups.set_announce(jid, true).await?,
        "l" | "link" | "r" | "revoke" => {
            let link = groups
                .get_invite_link(jid, matches!(ctx.arg(0), "r" | "revoke"))
                .await?;
            return ctx.reply(link).await;
        }
        "a" | "add" | "k" | "kick" | "p" | "promote" | "d" | "demote" => {
            let target = ctx
                .m
                .mentions
                .first()
                .cloned()
                .or_else(|| ctx.m.quoted.as_ref().map(|q| q.sender.clone()))
                .or_else(|| {
                    let number = crate::config::clean_number(ctx.arg(1));
                    (!number.is_empty()).then(|| format!("{number}@s.whatsapp.net"))
                })
                .context("Tentukan nomor, mention, atau balas pesan anggota")?;
            let members = [target.parse::<whatsapp_rust::Jid>()?];
            let result = match ctx.arg(0) {
                "a" | "add" => groups.add_participants(jid, &members).await?,
                "k" | "kick" => groups.remove_participants(jid, &members).await?,
                "p" | "promote" => groups.promote_participants(jid, &members).await?,
                _ => groups.demote_participants(jid, &members).await?,
            };
            return ctx.reply(format!("{result:?}")).await;
        }
        _ => {
            return ctx
                .reply("/group open|close|add|kick|promote|demote|link|revoke [nomor]")
                .await
        }
    }
    ctx.reply("✅ Pengaturan grup diperbarui").await
}
