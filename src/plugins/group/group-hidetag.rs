use crate::plugins::Context;
use whatsapp_rust::prelude::MessageBuilderExt;
crate::plugins::plugin!(
    &["hidetag", "ht", "h"],
    "group",
    "Mention semua anggota grup",
    execute,
    group_only = true,
    admin_only = true
);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let jid = ctx.m.chat.parse::<whatsapp_rust::Jid>()?;
    let group = ctx.m.socket.client.groups().get_metadata(&jid).await?;
    let mentions = group
        .participants
        .iter()
        .map(|p| p.jid.to_string())
        .collect();
    let text = if ctx.text.is_empty() {
        ctx.m.quoted.as_ref().map(|q| q.text.as_str()).unwrap_or("")
    } else {
        &ctx.text
    };
    let msg = whatsapp_rust::waproto::whatsapp::Message::text_with_context(
        text,
        whatsapp_rust::waproto::whatsapp::ContextInfo {
            mentioned_jid: mentions,
            ..Default::default()
        },
    );
    ctx.m.socket.client.send_message(jid, msg).await?;
    Ok(())
}
