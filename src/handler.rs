use crate::{
    bot_lib::core::smsg::MessageContext,
    config::clean_number,
    plugins::{App, Context, PluginMetadata},
};
use anyhow::Result;
use std::sync::Arc;

pub fn parse_command(text: &str) -> Option<(String, String)> {
    let body = text.strip_prefix('/').or_else(|| text.strip_prefix('.'))?;
    let (name, rest) = body
        .trim()
        .split_once(char::is_whitespace)
        .unwrap_or((body.trim(), ""));
    (!name.is_empty()).then(|| (name.to_lowercase(), rest.trim().into()))
}
pub fn permission(
    meta: PluginMetadata,
    owner: bool,
    group: bool,
    admin: bool,
    bot_admin: bool,
) -> Option<&'static str> {
    if meta.owner_only && !owner {
        Some("Perintah ini khusus owner.")
    } else if meta.group_only && !group {
        Some("Perintah ini khusus grup.")
    } else if meta.admin_only && !admin {
        Some("Perintah ini memerlukan admin grup.")
    } else if meta.bot_admin && !bot_admin {
        Some("Bot harus menjadi admin grup.")
    } else {
        None
    }
}

pub async fn handle(app: Arc<App>, inbound: whatsapp_rust::bot::MessageContext) -> Result<()> {
    let m = MessageContext::new(inbound);
    if m.from_me || m.chat == "status@broadcast" {
        return Ok(());
    }
    let key = format!("{}:{}", m.chat, m.id);
    if !app.store.seen.insert_if_absent(key.clone(), ()).await {
        return Ok(());
    }
    app.store.messages.insert(key, m.raw.clone()).await;
    if !m.push_name.is_empty() {
        app.store
            .contacts
            .insert(m.sender.clone(), m.push_name.clone())
            .await;
    }
    let phone = std::iter::once(m.sender.as_str())
        .chain(m.sender_alt.as_deref())
        .find(|s| s.ends_with("@s.whatsapp.net"))
        .map(clean_number);
    let is_owner = phone
        .as_ref()
        .is_some_and(|n| app.config.owners.contains(n));
    if app.config.self_mode && !is_owner {
        return Ok(());
    }
    let user = app
        .db
        .update(|d| d.touch(&m.sender, m.sender_alt.as_deref(), &m.push_name, is_owner))
        .await?;
    match tokio::time::timeout(
        std::time::Duration::from_secs(120),
        crate::plugins::handle_contact(&app, &m, &user),
    )
    .await
    {
        Ok(Ok(true)) => return Ok(()),
        Ok(Ok(false)) => (),
        Ok(Err(error)) => {
            tracing::warn!(%error,"Registrasi kontak gagal");
            return m.reply("Registrasi kontak gagal. Periksa konfigurasi Google Contacts dan mapping nomor pengirim.").await;
        }
        Err(_) => return m.reply("Verifikasi Google Contacts timeout.").await,
    }
    let normal = parse_command(&m.text);
    let selected = app.registry.iter().find_map(|p| {
        let meta = p.metadata();
        if let Some(prefix) = meta.custom_prefix {
            m.text
                .strip_prefix(prefix)
                .or_else(|| {
                    if prefix == "> " {
                        m.text.strip_prefix("=> ")
                    } else {
                        None
                    }
                })
                .map(|text| (p, meta.commands[0].to_string(), text.trim().to_string()))
        } else {
            normal
                .as_ref()
                .filter(|(cmd, _)| meta.commands.contains(&cmd.as_str()))
                .map(|(cmd, text)| (p, cmd.clone(), text.clone()))
        }
    });
    let Some((plugin, command, text)) = selected else {
        return Ok(());
    };
    if !is_owner {
        use crate::bot_lib::core::rate_limit::Decision;
        match app.rate.check(&m.sender).await {
            Decision::Allowed => (),
            Decision::Limited {
                retry_after_secs,
                notify,
            } => {
                if notify {
                    tracing::warn!(sender = %m.sender, "Rate limit perintah tercapai");
                    let _ = m
                        .reply(format!(
                            "⏳ Terlalu banyak perintah. Coba lagi dalam {retry_after_secs} detik."
                        ))
                        .await;
                }
                return Ok(());
            }
        }
    }
    let meta = plugin.metadata();
    if user["registered"] != true
        && !is_owner
        && !["register", "reg", "daftar", "cekreg"].contains(&command.as_str())
    {
        return m
            .reply("Nomor Anda belum terdaftar. Ketik /register untuk petunjuk registrasi.")
            .await;
    }
    // Periksa owner/group sebelum permintaan metadata apa pun.
    if let Some(error) = permission(
        PluginMetadata {
            admin_only: false,
            bot_admin: false,
            ..meta
        },
        is_owner,
        m.is_group,
        false,
        false,
    ) {
        return m.reply(error).await;
    }
    let mut is_admin = false;
    let mut is_bot_admin = false;
    if m.is_group && (meta.admin_only || meta.bot_admin) {
        let jid = m.chat.parse::<whatsapp_rust::Jid>()?;
        let group = m.socket.client.groups().get_metadata(&jid).await?;
        is_admin = group.participants.iter().any(|p| {
            p.is_admin()
                && (p.jid.to_string() == m.sender
                    || p.phone_number
                        .as_ref()
                        .is_some_and(|n| Some(clean_number(&n.to_string())) == phone)
                    || p.lid.as_ref().is_some_and(|n| n.to_string() == m.sender))
        });
        let device = m.socket.client.persistence_manager().get_device_arc().await;
        let device = device.read().await;
        is_bot_admin = group.participants.iter().any(|p| {
            p.is_admin()
                && (device.pn.as_ref().is_some_and(|j| {
                    p.jid.user == j.user
                        || p.phone_number.as_ref().is_some_and(|n| n.user == j.user)
                }) || device.lid.as_ref().is_some_and(|j| {
                    p.jid.user == j.user || p.lid.as_ref().is_some_and(|n| n.user == j.user)
                }))
        });
    }
    if let Some(error) = permission(meta, is_owner, m.is_group, is_admin, is_bot_admin) {
        return m.reply(error).await;
    }
    let ctx = Context {
        app: app.clone(),
        m,
        command,
        args: text.split_whitespace().map(String::from).collect(),
        text,
        is_owner,
        is_admin,
        is_bot_admin,
    };
    let _ = ctx.m.react("⏳").await;
    let result =
        tokio::time::timeout(std::time::Duration::from_secs(660), plugin.execute(&ctx)).await;
    let _ = ctx.m.react("").await;
    match result {
        Ok(Ok(())) => (),
        Ok(Err(error)) => {
            tracing::warn!(command = %ctx.command, error = %error, "Plugin gagal");
            ctx.reply("⚠️ Proses gagal. Periksa input, konfigurasi layanan, atau log bot.")
                .await?;
        }
        Err(_) => ctx.reply("Batas waktu pemrosesan tercapai.").await?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_only_leading_prefix() {
        assert_eq!(
            parse_command("/PING a b"),
            Some(("ping".into(), "a b".into()))
        );
        assert!(parse_command("hello /ping").is_none());
    }
    #[test]
    fn owner_gate_is_enforced() {
        let m = PluginMetadata {
            owner_only: true,
            ..PluginMetadata::new(&["exec"], "owner", "")
        };
        assert!(permission(m, false, true, true, true).is_some());
        assert!(permission(m, true, false, false, false).is_none());
    }
    #[test]
    fn group_admin_gates_are_independent() {
        let m = PluginMetadata {
            group_only: true,
            admin_only: true,
            bot_admin: true,
            ..PluginMetadata::new(&["group"], "group", "")
        };
        assert_eq!(
            permission(m, true, false, true, true),
            Some("Perintah ini khusus grup.")
        );
        assert_eq!(
            permission(m, true, true, false, true),
            Some("Perintah ini memerlukan admin grup.")
        );
        assert_eq!(
            permission(m, true, true, true, false),
            Some("Bot harus menjadi admin grup.")
        );
        assert_eq!(permission(m, false, true, true, true), None);
    }
}
