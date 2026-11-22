use whatsapp_rust::{prelude::MessageExt, waproto::whatsapp as wa};

pub fn text(message: &wa::Message) -> String {
    message
        .text_content()
        .or_else(|| message.get_caption())
        .unwrap_or_default()
        .into()
}
pub fn context(message: &wa::Message) -> Option<&wa::ContextInfo> {
    let b = message.get_base_message();
    b.extended_text_message
        .as_option()
        .and_then(|m| m.context_info.as_option())
        .or_else(|| {
            b.image_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| {
            b.video_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| {
            b.audio_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| {
            b.document_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| {
            b.sticker_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
}
pub fn mime(message: &wa::Message) -> &str {
    let b = message.get_base_message();
    b.image_message
        .as_option()
        .and_then(|m| m.mimetype.as_deref())
        .or_else(|| {
            b.video_message
                .as_option()
                .and_then(|m| m.mimetype.as_deref())
        })
        .or_else(|| {
            b.audio_message
                .as_option()
                .and_then(|m| m.mimetype.as_deref())
        })
        .or_else(|| {
            b.document_message
                .as_option()
                .and_then(|m| m.mimetype.as_deref())
        })
        .or_else(|| {
            b.sticker_message
                .as_option()
                .and_then(|m| m.mimetype.as_deref())
        })
        .unwrap_or("")
}
pub fn media_length(message: &wa::Message) -> u64 {
    let b = message.get_base_message();
    b.image_message
        .as_option()
        .and_then(|m| m.file_length)
        .or_else(|| b.video_message.as_option().and_then(|m| m.file_length))
        .or_else(|| b.audio_message.as_option().and_then(|m| m.file_length))
        .or_else(|| b.document_message.as_option().and_then(|m| m.file_length))
        .or_else(|| b.sticker_message.as_option().and_then(|m| m.file_length))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use whatsapp_rust::prelude::{MessageBuilderExt, MessageField};
    #[test]
    fn parses_caption_and_quote() {
        let m = wa::Message {
            image_message: MessageField::some(wa::message::ImageMessage {
                caption: Some("/s".into()),
                mimetype: Some("image/png".into()),
                context_info: MessageField::some(wa::ContextInfo {
                    quoted_message: MessageField::some(wa::Message::text("hello")),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(text(&m), "/s");
        assert_eq!(mime(&m), "image/png");
        assert_eq!(
            context(&m)
                .and_then(|c| c.quoted_message.as_option())
                .map(text),
            Some("hello".into())
        );
    }
}
