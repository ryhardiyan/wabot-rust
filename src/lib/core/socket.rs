use anyhow::Result;
use std::sync::Arc;
use whatsapp_rust::{
    prelude::{MessageBuilderExt, MessageExt, MessageField},
    wacore::download::MediaType,
    waproto::whatsapp as wa,
    Client, UploadOptions,
};

#[derive(Clone)]
pub struct WaSocket {
    pub client: Arc<Client>,
}
impl WaSocket {
    pub async fn send_text(&self, chat: &str, text: impl Into<String>) -> Result<()> {
        self.client
            .send_message(chat.parse::<whatsapp_rust::Jid>()?, wa::Message::text(text))
            .await?;
        Ok(())
    }
    #[allow(dead_code)] // Helper API untuk plugin; send_media dipakai saat ini.
    pub async fn send_image(&self, chat: &str, bytes: Vec<u8>, caption: &str) -> Result<()> {
        self.send_media(chat, bytes, "image/jpeg", "image.jpg", caption, None)
            .await
    }
    pub async fn send_media(
        &self,
        chat: &str,
        bytes: Vec<u8>,
        mime: &str,
        name: &str,
        caption: &str,
        quote: Option<wa::ContextInfo>,
    ) -> Result<()> {
        anyhow::ensure!(
            bytes.len() <= 25 * 1024 * 1024,
            "Pengiriman media maksimum 25 MiB"
        );
        let animated = mime == "image/webp" && bytes.windows(4).any(|chunk| chunk == b"ANIM");
        let dimensions = if mime.starts_with("image/") {
            image::ImageReader::new(std::io::Cursor::new(&bytes))
                .with_guessed_format()?
                .into_dimensions()
                .ok()
        } else {
            None
        };
        let kind = if mime == "image/webp" {
            "sticker"
        } else {
            mime.split('/').next().unwrap_or("document")
        };
        let media_type = match kind {
            "image" | "sticker" => MediaType::Image,
            "video" => MediaType::Video,
            "audio" => MediaType::Audio,
            _ => MediaType::Document,
        };
        let up = self
            .client
            .upload(bytes, media_type, UploadOptions::default())
            .await?;
        macro_rules! fill {
            ($t:path) => {{
                let mut value: $t = Default::default();
                value.url = Some(up.url.clone());
                value.direct_path = Some(up.direct_path.clone());
                value.media_key = Some(up.media_key.to_vec());
                value.file_sha256 = Some(up.file_sha256.to_vec());
                value.file_enc_sha256 = Some(up.file_enc_sha256.to_vec());
                value.file_length = Some(up.file_length);
                value.mimetype = Some(mime.into());
                value
            }};
        }
        let mut msg = wa::Message::default();
        match kind {
            "sticker" => {
                let mut m = fill!(wa::message::StickerMessage);
                if let Some((width, height)) = dimensions {
                    m.width = Some(width);
                    m.height = Some(height);
                }
                m.is_animated = Some(animated);
                msg.sticker_message = MessageField::some(m)
            }
            "image" => {
                let mut m = fill!(wa::message::ImageMessage);
                m.caption = Some(caption.into());
                if let Some((width, height)) = dimensions {
                    m.width = Some(width);
                    m.height = Some(height);
                }
                msg.image_message = MessageField::some(m);
            }
            "video" => {
                let mut m = fill!(wa::message::VideoMessage);
                m.caption = Some(caption.into());
                msg.video_message = MessageField::some(m);
            }
            "audio" => msg.audio_message = MessageField::some(fill!(wa::message::AudioMessage)),
            _ => {
                let mut m = fill!(wa::message::DocumentMessage);
                m.caption = Some(caption.into());
                m.file_name = Some(name.into());
                msg.document_message = MessageField::some(m);
            }
        }
        if let Some(ctx) = quote {
            msg.set_context_info(ctx);
        }
        self.client
            .send_message(chat.parse::<whatsapp_rust::Jid>()?, msg)
            .await?;
        Ok(())
    }
}
