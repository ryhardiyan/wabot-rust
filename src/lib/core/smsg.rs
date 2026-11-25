use super::{message, socket::WaSocket};
use anyhow::{bail, Result};
use std::sync::Arc;
use whatsapp_rust::{bot::MessageContext as Inbound, prelude::MessageExt, waproto::whatsapp as wa};

#[derive(Clone)]
pub struct QuotedMessage {
    #[allow(dead_code)] // Kontrak kompatibilitas smsg untuk plugin berikutnya.
    pub id: String,
    pub sender: String,
    pub text: String,
    pub message: Arc<wa::Message>,
}
#[derive(Clone)]
pub struct MessageContext {
    pub id: String,
    pub sender: String,
    pub sender_alt: Option<String>,
    pub chat: String,
    pub text: String,
    pub push_name: String,
    pub is_group: bool,
    pub from_me: bool,
    pub quoted: Option<QuotedMessage>,
    pub mentions: Vec<String>,
    pub raw: Arc<wa::Message>,
    pub socket: WaSocket,
    inbound: Inbound,
}
#[allow(dead_code)] // Nama publik kompatibel migrasi Baileys.
pub type SerializedMessage = MessageContext;
impl MessageContext {
    pub fn new(inbound: Inbound) -> Self {
        let ctx = message::context(&inbound.message);
        let quoted = ctx.and_then(|c| {
            c.quoted_message.as_option().map(|m| QuotedMessage {
                id: c.stanza_id.clone().unwrap_or_default(),
                sender: c.participant.clone().unwrap_or_default(),
                text: message::text(m),
                message: Arc::new(m.clone()),
            })
        });
        Self {
            id: inbound.info.id.clone(),
            sender: inbound.info.source.sender.to_string(),
            sender_alt: inbound
                .info
                .source
                .sender_alt
                .as_ref()
                .map(ToString::to_string),
            chat: inbound.info.source.chat.to_string(),
            text: message::text(&inbound.message),
            push_name: inbound.info.push_name.clone(),
            is_group: inbound.info.source.is_group,
            from_me: inbound.info.source.is_from_me,
            quoted,
            mentions: ctx.map(|c| c.mentioned_jid.clone()).unwrap_or_default(),
            raw: inbound.message.clone(),
            socket: WaSocket {
                client: inbound.client.clone(),
            },
            inbound,
        }
    }
    pub async fn reply(&self, text: impl Into<String>) -> Result<()> {
        let text = text.into();
        if text.chars().count() > 12000 {
            return self
                .send_media(text.into_bytes(), "text/plain", "reply.txt", "Output teks")
                .await;
        }
        super::logger::log_outgoing(&self.chat, &text);
        self.inbound.reply_quoting(text).await?;
        Ok(())
    }
    pub async fn react(&self, emoji: &str) -> Result<()> {
        if !emoji.is_empty() {
            super::logger::log_outgoing(&self.chat, "[Reaction]");
        }
        self.inbound.react(emoji).await?;
        Ok(())
    }
    pub fn media(&self) -> &wa::Message {
        self.quoted
            .as_ref()
            .map(|q| q.message.as_ref())
            .unwrap_or(&self.raw)
    }
    pub async fn download(&self, limit: usize) -> Result<Vec<u8>> {
        let media = self.media();
        if message::media_length(media) > limit as u64 {
            bail!("Media melebihi batas {} MiB", limit / 1024 / 1024);
        }
        let b = media.get_base_message();
        let writer = LimitedWriter {
            cursor: std::io::Cursor::new(Vec::new()),
            limit,
        };
        let writer = if let Some(m) = b.image_message.as_option() {
            self.socket.client.download_to_writer(m, writer).await?
        } else if let Some(m) = b.video_message.as_option() {
            self.socket.client.download_to_writer(m, writer).await?
        } else if let Some(m) = b.audio_message.as_option() {
            self.socket.client.download_to_writer(m, writer).await?
        } else if let Some(m) = b.document_message.as_option() {
            self.socket.client.download_to_writer(m, writer).await?
        } else if let Some(m) = b.sticker_message.as_option() {
            self.socket.client.download_to_writer(m, writer).await?
        } else {
            bail!("Kirim atau balas media terlebih dahulu");
        };
        let bytes = writer.cursor.into_inner();
        if bytes.len() > limit {
            bail!("Ukuran media melampaui batas");
        }
        Ok(bytes)
    }
    pub async fn send_media(
        &self,
        data: Vec<u8>,
        mime: &str,
        name: &str,
        caption: &str,
    ) -> Result<()> {
        self.socket
            .send_media(
                &self.chat,
                data,
                mime,
                name,
                caption,
                Some(self.inbound.build_quote_context()),
            )
            .await
    }
    /// Kirim stiker audio dengan context quote pesan yang dibalas.
    pub async fn send_audio_sticker(
        &self,
        sticker: Vec<u8>,
        audio: Vec<u8>,
        is_animated: bool,
    ) -> Result<()> {
        self.socket
            .send_audio_sticker(
                &self.chat,
                sticker,
                audio,
                is_animated,
                Some(self.inbound.build_quote_context()),
            )
            .await
    }
}

struct LimitedWriter {
    cursor: std::io::Cursor<Vec<u8>>,
    limit: usize,
}
impl std::io::Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.cursor.position().saturating_add(bytes.len() as u64) > self.limit as u64 {
            return Err(std::io::Error::other("Media melampaui batas"));
        }
        std::io::Write::write(&mut self.cursor, bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl std::io::Seek for LimitedWriter {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        std::io::Seek::seek(&mut self.cursor, position)
    }
}
impl whatsapp_rust::wacore::download::DownloadWriter for LimitedWriter {
    fn truncate(&mut self, len: u64) -> std::io::Result<()> {
        self.cursor.get_mut().truncate(len as usize);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enforces_limit_before_allocation() -> std::io::Result<()> {
        use std::io::Write;
        let mut w = LimitedWriter {
            cursor: std::io::Cursor::new(Vec::new()),
            limit: 3,
        };
        w.write_all(b"abc")?;
        assert!(w.write_all(b"d").is_err());
        assert_eq!(w.cursor.into_inner(), b"abc");
        Ok(())
    }
    #[tokio::test]
    async fn serializes_real_upstream_context_without_network() -> Result<()> {
        use whatsapp_rust::prelude::{Bot, MessageBuilderExt, MessageField};
        let dir = tempfile::tempdir()?;
        let store = super::super::auth::session::open(&dir.path().join("session.db")).await?;
        let bot = Bot::builder().with_backend(store).build().await?;
        let message = wa::Message::text_with_context(
            "/ping",
            wa::ContextInfo {
                stanza_id: Some("quoted-id".into()),
                participant: Some("628111111111@s.whatsapp.net".into()),
                quoted_message: MessageField::some(wa::Message::text("quoted text")),
                ..Default::default()
            },
        );
        let mut info = whatsapp_rust::types::message::MessageInfo {
            id: "incoming-id".into(),
            ..Default::default()
        };
        info.source.sender = "123456789@lid".parse()?;
        info.source.sender_alt = Some("628123456789@s.whatsapp.net".parse()?);
        info.source.chat = "123456789-123456@g.us".parse()?;
        info.source.is_group = true;
        let m = MessageContext::new(Inbound::from_parts(&message, &info, bot.client()));
        assert_eq!(m.text, "/ping");
        assert!(m.is_group);
        assert_eq!(m.sender, "123456789@lid");
        assert_eq!(
            m.quoted.as_ref().map(|q| q.text.as_str()),
            Some("quoted text")
        );
        Ok(())
    }
}
