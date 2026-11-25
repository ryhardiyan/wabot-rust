use anyhow::Result;
use std::sync::Arc;
use whatsapp_rust::{
    prelude::{MessageBuilderExt, MessageExt, MessageField},
    wacore::download::MediaType,
    waproto::whatsapp as wa,
    Client, SendOptions, StanzaType, UploadOptions,
};

#[derive(Clone)]
pub struct WaSocket {
    pub client: Arc<Client>,
}
impl WaSocket {
    #[allow(dead_code)] // Helper API untuk plugin.
    pub async fn send_text(&self, chat: &str, text: impl Into<String>) -> Result<()> {
        let text = text.into();
        super::logger::log_outgoing(chat, &text);
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
        let indicator = match kind {
            "sticker" => "[Sticker]",
            "image" => "[Image]",
            "video" => "[Video]",
            "audio" => "[Audio]",
            _ => "[Document]",
        };
        let content = if caption.trim().is_empty() {
            indicator.to_string()
        } else {
            format!("{indicator} {}", caption.trim())
        };
        super::logger::log_outgoing(chat, &content);
        Ok(())
    }

    /// Kirim stiker animasi dengan audio Opus yang disisipkan (field 134).
    ///
    /// `context` dipakai untuk balasan/quote; `experienceIds = [13007]` selalu
    /// ditambahkan agar WhatsApp mengenali pesan sebagai stiker audio.
    #[allow(dead_code)] // Helper API untuk plugin.
    pub async fn send_audio_sticker(
        &self,
        chat: &str,
        sticker: Vec<u8>,
        audio: Vec<u8>,
        is_animated: bool,
        context: Option<wa::ContextInfo>,
    ) -> Result<()> {
        let sticker_len = sticker.len() as u64;
        let audio_len = audio.len() as u64;
        anyhow::ensure!(
            sticker_len <= 25 * 1024 * 1024 && audio_len <= 25 * 1024 * 1024,
            "Stiker/audio melebihi batas 25 MiB"
        );
        let sticker_up = self
            .client
            .upload(sticker, MediaType::Image, UploadOptions::default())
            .await?;
        let audio_up = self
            .client
            .upload(audio, MediaType::Audio, UploadOptions::default())
            .await?;

        let sticker_media = MediaFields::from(&sticker_up, sticker_len);
        let audio_media = MediaFields::from(&audio_up, audio_len);
        let message = build_audio_sticker_message(
            &sticker_media,
            &audio_media,
            is_animated,
            chrono::Utc::now().timestamp(),
            context,
        );
        // whatsmeow mengirim AudioStickerMessage sebagai `type="text"` karena
        // classifier generiknya tidak mengenali wrapper ini sebagai media. Tanpa
        // override, whatsapp-rust memakai `type="media"` tanpa atribut
        // `mediatype`, yang bisa membuat server men-drop stanza.
        self.client
            .send_message_with_options(
                chat.parse::<whatsapp_rust::Jid>()?,
                message,
                SendOptions::default().with_stanza_type_override(StanzaType::Text),
            )
            .await?;
        super::logger::log_outgoing(chat, "[Stiker Audio]");
        Ok(())
    }
}

struct MediaFields<'a> {
    url: &'a str,
    direct_path: &'a str,
    media_key: &'a [u8],
    file_enc_sha256: &'a [u8],
    file_sha256: &'a [u8],
    file_length: u64,
}
impl<'a> MediaFields<'a> {
    fn from(up: &'a whatsapp_rust::upload::UploadResponse, file_length: u64) -> Self {
        Self {
            url: &up.url,
            direct_path: &up.direct_path,
            media_key: &up.media_key,
            file_enc_sha256: &up.file_enc_sha256,
            file_sha256: &up.file_sha256,
            file_length,
        }
    }
}

/// Rakit pesan stiker audio: StickerMessage dengan AudioMessage pada field 26,
/// dibungkus FutureProofMessage pada Message field 134.
fn build_audio_sticker_message(
    sticker: &MediaFields<'_>,
    audio: &MediaFields<'_>,
    is_animated: bool,
    sent_at: i64,
    context: Option<wa::ContextInfo>,
) -> wa::Message {
    let audio_message = wa::message::AudioMessage {
        url: Some(audio.url.into()),
        direct_path: Some(audio.direct_path.into()),
        media_key: Some(audio.media_key.to_vec()),
        file_enc_sha256: Some(audio.file_enc_sha256.to_vec()),
        file_sha256: Some(audio.file_sha256.to_vec()),
        file_length: Some(audio.file_length),
        mimetype: Some("audio/ogg; codecs=opus".into()),
        ..Default::default()
    };
    let mut ctx = context.unwrap_or_default();
    ctx.experience_ids = vec![13007];
    let sticker_message = wa::message::StickerMessage {
        url: Some(sticker.url.into()),
        direct_path: Some(sticker.direct_path.into()),
        media_key: Some(sticker.media_key.to_vec()),
        file_enc_sha256: Some(sticker.file_enc_sha256.to_vec()),
        file_sha256: Some(sticker.file_sha256.to_vec()),
        file_length: Some(sticker.file_length),
        mimetype: Some("image/webp".into()),
        is_animated: Some(is_animated),
        sticker_sent_ts: Some(sent_at),
        context_info: MessageField::some(ctx),
        audio: Some(wa::message::sticker_message::Audio::AudioMessage(Box::new(
            audio_message,
        ))),
        ..Default::default()
    };
    wa::Message {
        audio_sticker_message: MessageField::some(wa::message::FutureProofMessage {
            message: MessageField::some(wa::Message {
                sticker_message: MessageField::some(sticker_message),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whatsapp_rust::waproto::codec::message_to_vec;

    fn upload(len: u64) -> MediaFields<'static> {
        static KEY: [u8; 32] = [1u8; 32];
        static ENC: [u8; 32] = [2u8; 32];
        static SHA: [u8; 32] = [3u8; 32];
        MediaFields {
            url: "https://example.invalid/u",
            direct_path: "/path/u",
            media_key: &KEY,
            file_enc_sha256: &ENC,
            file_sha256: &SHA,
            file_length: len,
        }
    }

    #[test]
    fn audio_sticker_uses_the_wire_fields_expected_by_whatsapp() {
        let message = build_audio_sticker_message(
            &upload(11),
            &upload(12),
            true,
            42,
            Some(wa::ContextInfo {
                stanza_id: Some("quoted".into()),
                ..Default::default()
            }),
        );
        let encoded = message_to_vec(&message);
        // Message.audioStickerMessage = 134, wire type 2 => tag b2 08.
        assert!(encoded.windows(2).any(|w| w == [0xb2, 0x08]));
        // ContextInfo.experienceIds = 82, packed => tag 92 05, len 2, value 13007.
        assert!(encoded
            .windows(5)
            .any(|w| w == [0x92, 0x05, 0x02, 0xcf, 0x65]));
        // StickerMessage.audioMessage = 26, wire type 2 => tag d2 01.
        assert!(encoded.windows(2).any(|w| w == [0xd2, 0x01]));
        // isAnimated field 13 + stickerSentTs field 18 survive the nesting.
        assert!(encoded.windows(2).any(|w| w == [0x68, 0x01]));
    }
}
