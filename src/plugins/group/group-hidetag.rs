use crate::plugins::Context;
use whatsapp_rust::prelude::MessageBuilderExt;
crate::plugins::plugin!(
    &["hidetag", "ht", "h"],
    "group",
    execute,
    group_only = true,
    admin_only = true
);

async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let text = if ctx.text.is_empty() {
        ctx.m.quoted.as_ref().map(|q| q.text.as_str()).unwrap_or("")
    } else {
        &ctx.text
    };
    anyhow::ensure!(!text.is_empty(), "/hidetag <pesan> atau balas pesan");

    let jid = ctx.m.chat.parse::<whatsapp_rust::Jid>()?;
    let group = ctx.m.socket.client.groups().get_metadata(&jid).await?;
    let mentions = group
        .participants
        .iter()
        .map(|p| p.jid.to_string())
        .collect();
    let msg = whatsapp_rust::waproto::whatsapp::Message::text_with_context(
        text,
        whatsapp_rust::waproto::whatsapp::ContextInfo {
            mentioned_jid: mentions,
            ..Default::default()
        },
    );
    ctx.m.socket.client.send_message(jid, msg).await?;
    crate::bot_lib::core::logger::log_outgoing(&ctx.m.chat, text);
    Ok(())
}
