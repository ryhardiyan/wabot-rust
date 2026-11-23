use crate::plugins::Context;
use anyhow::{Context as _, Result};
crate::plugins::plugin!(
    &["g", "group"],
    "group",
    execute,
    group_only = true,
    admin_only = true,
    bot_admin = true
);

const USAGE: &str = "/group open|close|add|kick|promote|demote|link|revoke [nomor]";

async fn execute(ctx: &Context) -> Result<()> {
    ctx.app.store.groups.remove(&ctx.m.chat).await;
    let jid = ctx.m.chat.parse::<whatsapp_rust::Jid>()?;
    let client = &ctx.m.socket.client;
    let groups = client.groups();
    match ctx.arg(0) {
        "o" | "open" => groups.set_announce(jid, false).await?,
        "c" | "close" => groups.set_announce(jid, true).await?,
        "l" | "link" | "r" | "revoke" => {
            let reset = matches!(ctx.arg(0), "r" | "revoke");
            let link = groups.get_invite_link(jid, reset).await?;
            let label = if reset {
                "Link grup diperbarui"
            } else {
                "Link grup"
            };
            return ctx.reply(format!("*{label}*\n{link}")).await;
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
            let action = ctx.arg(0);
            let result = match action {
                "a" | "add" => groups.add_participants(jid, &members).await?,
                "k" | "kick" => groups.remove_participants(jid, &members).await?,
                "p" | "promote" => groups.promote_participants(jid, &members).await?,
                _ => groups.demote_participants(jid, &members).await?,
            };
            let label = match action {
                "a" | "add" => "Hasil Tambah Anggota",
                "k" | "kick" => "Hasil Keluarkan Anggota",
                "p" | "promote" => "Hasil Promosi Admin",
                _ => "Hasil Copot Admin",
            };
            let lines = result
                .iter()
                .map(|item| {
                    let mark = if item.is_ok() { "✅" } else { "❌" };
                    let note = item.error.as_deref().unwrap_or("berhasil");
                    format!("{mark} @{} — {note}", item.jid.user)
                })
                .collect::<Vec<_>>()
                .join("\n");
            return ctx.reply(format!("*{label}*\n{lines}")).await;
        }
        _ => return ctx.reply(format!("*{USAGE}*")).await,
    }
    ctx.reply("✅ Pengaturan grup diperbarui").await
}
