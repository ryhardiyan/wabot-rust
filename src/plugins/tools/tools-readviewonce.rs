use crate::plugins::Context;
use anyhow::Result;
use whatsapp_rust::prelude::{MessageExt, MessageField};
crate::plugins::plugin!(
    &["read", "readview", "readviewonce", "rvo"],
    "tools",
    "Kirim ulang media sekali lihat dari pesan yang dibalas",
    execute,
    owner_only = true
);
async fn execute(ctx: &Context) -> Result<()> {
    let media = ctx.m.media();
    anyhow::ensure!(media.is_view_once(), "Balas pesan sekali lihat");
    let mut output = media.get_base_message().clone();
    if let Some(m) = output.image_message.as_option() {
        let mut m = m.clone();
        m.view_once = Some(false);
        output.image_message = MessageField::some(m);
    }
    if let Some(m) = output.video_message.as_option() {
        let mut m = m.clone();
        m.view_once = Some(false);
        output.video_message = MessageField::some(m);
    }
    if let Some(m) = output.audio_message.as_option() {
        let mut m = m.clone();
        m.view_once = Some(false);
        output.audio_message = MessageField::some(m);
    }
    ctx.m
        .socket
        .client
        .send_message(ctx.m.chat.parse::<whatsapp_rust::Jid>()?, output)
        .await?;
    Ok(())
}
