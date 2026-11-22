use crate::plugins::Context;
use whatsapp_rust::prelude::{MessageBuilderExt, MessageExt};
crate::plugins::plugin!(
    &["statusgc", "swgc"],
    "owner",
    "Kirim status grup dengan context protokol",
    execute,
    owner_only = true,
    group_only = true
);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let mut msg = if ctx.text.is_empty() {
        ctx.m.media().get_base_message().clone()
    } else {
        whatsapp_rust::waproto::whatsapp::Message::text(&ctx.text)
    };
    let context = whatsapp_rust::waproto::whatsapp::ContextInfo {
        is_group_status: Some(true),
        ..Default::default()
    };
    anyhow::ensure!(
        msg.set_context_info(context),
        "Balas media atau berikan teks"
    );
    ctx.m
        .socket
        .client
        .send_message(ctx.m.chat.parse::<whatsapp_rust::Jid>()?, msg)
        .await?;
    Ok(())
}
