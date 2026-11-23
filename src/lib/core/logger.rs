//! Real-time console logger untuk pesan masuk/keluar dan error fitur.
//!
//! Format pesan (dua baris):
//! ```text
//! 12.00 - +62 823-2782-9267 - Lu Wu Yi - Private Chat
//! Terimakasih admin sayang
//! ```
//!
//! Semua I/O tulis ke terminal terjadi di satu thread khusus. Pemanggil hanya
//! melakukan `try_send` ke channel bounded, jadi logging tidak pernah
//! memblokir pemrosesan pesan. Setiap entry juga dibungkus `catch_unwind`
//! dan parsing JID tidak pernah `unwrap` pada data eksternal: payload korup
//! tidak akan membuat bot crash.

use super::smsg::MessageContext;
use std::{
    io::Write,
    sync::{mpsc, OnceLock},
};
use whatsapp_rust::{prelude::MessageExt, waproto::whatsapp as wa};

const QUEUE: usize = 1024;

struct LogEvent {
    text: String,
}

static SENDER: OnceLock<mpsc::SyncSender<LogEvent>> = OnceLock::new();

/// Inisialisasi lazily (aman dipanggil berulang kali).
pub fn init() {
    let _ = sender();
}

fn sender() -> &'static mpsc::SyncSender<LogEvent> {
    SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<LogEvent>(QUEUE);
        let _ = std::thread::Builder::new()
            .name("wa-logger".into())
            .spawn(move || {
                let stdout = std::io::stdout();
                for event in rx {
                    let mut out = stdout.lock();
                    let _ = writeln!(out, "{}", event.text);
                    let _ = out.flush();
                }
            });
        tx
    })
}

/// Jalankan fungsi dengan pengaman panic tanpa peduli bound `UnwindSafe`
/// (beberapa tipe proto/anyhow memang tidak `RefUnwindSafe`).
fn guard<F: FnOnce()>(f: F) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
}

/// Kirim ke writer thread. Gagal kirim (channel penuh / thread mati) diabaikan.
fn emit(text: String) {
    guard(move || {
        let _ = sender().try_send(LogEvent { text });
    });
}

fn now_wib() -> String {
    // UTC+7 tanpa bergantung pada timezone lokal mesin / chrono-tz.
    (chrono::Utc::now() + chrono::Duration::hours(7))
        .format("%H.%M")
        .to_string()
}

fn is_phone_jid(jid: &str) -> bool {
    jid.ends_with("@s.whatsapp.net") || jid.ends_with("@c.us")
}

/// `6282327829267` -> `+62 823-2782-9267`.
fn format_phone(digits: &str) -> String {
    let mut d = digits.trim().replace(|c| matches!(c, '+' | ' ' | '-'), "");
    if let Some(rest) = d.strip_prefix('0') {
        d = format!("62{rest}");
    } else if d.starts_with('8') {
        d = format!("62{d}");
    }
    if !d.starts_with("62") {
        return if d.is_empty() {
            String::new()
        } else {
            format!("+{d}")
        };
    }
    let rest = &d[2..];
    if rest.is_empty() {
        return "+62".into();
    }
    let mut groups: Vec<&str> = Vec::new();
    let mut end = rest.len();
    while end > 4 {
        groups.insert(0, &rest[end - 4..end]);
        end -= 4;
    }
    if end > 0 {
        groups.insert(0, &rest[..end]);
    }
    format!("+62 {}", groups.join("-"))
}

fn jid_digits(jid: &str) -> String {
    jid.split('@')
        .next()
        .unwrap_or(jid)
        .chars()
        .filter(char::is_ascii_digit)
        .collect()
}

/// Nomor user dari `sender`; LID belum ter-resolve ditampilkan apa adanya.
fn format_sender(sender: &str, sender_alt: Option<&str>, from_me: bool) -> String {
    if from_me {
        return "Bot".into();
    }
    let mut candidates: Vec<&str> = Vec::with_capacity(2);
    candidates.push(sender);
    if let Some(alt) = sender_alt {
        candidates.push(alt);
    }
    if let Some(jid) = candidates.iter().copied().find(|jid| is_phone_jid(jid)) {
        let digits = jid_digits(jid);
        if !digits.is_empty() {
            return format_phone(&digits);
        }
    }
    if let Some(lid) = candidates.iter().copied().find(|jid| jid.ends_with("@lid")) {
        return lid.to_string();
    }
    for jid in candidates {
        let digits = jid_digits(jid);
        if !digits.is_empty() {
            return format_phone(&digits);
        }
    }
    sender.to_string()
}

/// Kategori chat sesuai label yang diminta.
pub fn chat_category(chat: &str) -> &'static str {
    if chat == "status@broadcast" {
        "Status / SW"
    } else if chat.ends_with("@g.us") {
        "Group Chat"
    } else if chat.ends_with("@newsletter") {
        "Channel / Newsletter"
    } else if chat.ends_with("@s.whatsapp.net") || chat.ends_with("@lid") || chat.ends_with("@c.us")
    {
        "Private Chat"
    } else {
        "Lainnya"
    }
}

/// Teks dari respon button/list bila pesan tidak punya teks/caption biasa.
fn response_text(base: &wa::Message) -> Option<String> {
    if let Some(text) = base
        .template_button_reply_message
        .as_option()
        .and_then(|reply| reply.selected_display_text.as_deref())
    {
        return Some(text.to_string());
    }
    if let Some(reply) = base.buttons_response_message.as_option() {
        use wa::message::buttons_response_message::Response;
        if let Some(Response::SelectedDisplayText(text)) = reply.response.as_ref() {
            return Some(text.clone());
        }
        if let Some(id) = reply.selected_button_id.as_deref() {
            return Some(id.to_string());
        }
    }
    if let Some(reply) = base.list_response_message.as_option() {
        if let Some(title) = reply.title.as_deref() {
            return Some(title.to_string());
        }
        if let Some(desc) = reply.description.as_deref() {
            return Some(desc.to_string());
        }
    }
    if let Some(text) = base
        .interactive_response_message
        .as_option()
        .and_then(|reply| reply.body.as_option())
        .and_then(|body| body.text.as_deref())
    {
        return Some(text.to_string());
    }
    None
}

/// Indikator tipe saat pesan tidak membawa teks sama sekali.
fn media_indicator(base: &wa::Message) -> Option<&'static str> {
    if base.sticker_message.is_set() {
        Some("[Sticker]")
    } else if base.image_message.is_set() {
        Some("[Image]")
    } else if base.ptv_message.is_set() || base.video_message.is_set() {
        Some("[Video]")
    } else if base.audio_message.is_set() {
        Some("[Audio]")
    } else if base.document_message.is_set() {
        Some("[Document]")
    } else if base.location_message.is_set() || base.live_location_message.is_set() {
        Some("[Location]")
    } else if base.contact_message.is_set() || base.contacts_array_message.is_set() {
        Some("[Contact]")
    } else if base.protocol_message.is_set() {
        Some("[System Message]")
    } else if base.reaction_message.is_set() {
        Some("[Reaction]")
    } else if base.buttons_response_message.is_set()
        || base.list_response_message.is_set()
        || base.interactive_response_message.is_set()
        || base.template_button_reply_message.is_set()
    {
        Some("[Response]")
    } else if base.event_message.is_set() || base.group_invite_message.is_set() {
        Some("[Other]")
    } else {
        None
    }
}

fn message_line(number: &str, name: &str, chat: &str, content: &str) -> String {
    format!(
        "{} - {} - {} - {}\n{}",
        now_wib(),
        number,
        name,
        chat,
        content
    )
}

/// Log satu pesan masuk. Dipanggil handler sebelum filter command.
pub fn log_message(m: &MessageContext) {
    guard(|| {
        let base = m.raw.get_base_message();
        let content = if m.text.trim().is_empty() {
            response_text(base)
                .or_else(|| media_indicator(base).map(str::to_string))
                .unwrap_or_else(|| "[Pesan Kosong]".into())
        } else {
            m.text.clone()
        };
        let name = if m.push_name.trim().is_empty() {
            "Tanpa Nama"
        } else {
            m.push_name.trim()
        };
        let line = message_line(
            &format_sender(&m.sender, m.sender_alt.as_deref(), false),
            name,
            chat_category(&m.chat),
            &content,
        );
        emit(line);
    });
}

/// Log pesan keluar dari bot. `content` berisi teks balasan atau indikator
/// media seperti `[Image]`, `[Sticker]`, dst.
pub fn log_outgoing(chat: &str, content: &str) {
    guard(|| {
        emit(message_line("Bot", "Bot", chat_category(chat), content));
    });
}

fn error_line(
    feature: &str,
    reason: &str,
    user: Option<&str>,
    chat: Option<&str>,
    location: Option<&str>,
) -> String {
    const RED: &str = "\x1b[31m";
    const RESET: &str = "\x1b[0m";
    let user = user.filter(|s| !s.is_empty()).unwrap_or("-");
    let chat = chat.filter(|s| !s.is_empty()).unwrap_or("-");
    let mut line = format!(
        "{} - {}[ERROR]{} Fitur: {} | User: {} ({})\nReason: {}",
        now_wib(),
        RED,
        RESET,
        feature,
        user,
        chat,
        reason
    );
    if let Some(loc) = location.filter(|s| !s.is_empty()) {
        line.push_str("\nAt: ");
        line.push_str(loc);
    }
    line
}

/// Fungsi error logger umum, bisa dipanggil dari blok `catch` tiap plugin.
/// `chat` sebaiknya sudah berupa label kategori (`chat_category`).
pub fn log_error(feature: &str, error: &str, user: Option<&str>, chat: Option<&str>) {
    guard(|| {
        emit(error_line(feature, error, user, chat, None));
    });
}

/// Varian `anyhow::Error`: menyertakan rantai penyebab singkat dan lokasi
/// sumber pertama yang bisa ditemukan dari backtrace (bila tersedia).
pub fn log_anyhow(feature: &str, error: &anyhow::Error, user: Option<&str>, chat: Option<&str>) {
    guard(|| {
        let mut reason = error.to_string();
        for cause in error.chain().skip(1).take(2) {
            reason.push_str(" <- ");
            reason.push_str(&cause.to_string());
        }
        let debug = format!("{error:?}");
        // Ambil frame sumber pertama yang relevan, bukan frame internal
        // std/registry agar baris "At:" tetap pendek dan berguna.
        let location = debug
            .lines()
            .map(str::trim)
            .find(|line| {
                line.contains(".rs:")
                    && !line.contains("/rustc/")
                    && !line.contains("/registry/")
                    && !line.contains("/.cargo/")
            })
            .map(str::to_string);
        emit(error_line(
            feature,
            &reason,
            user,
            chat,
            location.as_deref(),
        ));
    });
}

fn user_label(m: &MessageContext) -> String {
    format_sender(&m.sender, m.sender_alt.as_deref(), false)
}

/// Helper plugin: ambil user & kategori chat otomatis dari konteks pesan.
pub fn log_from(m: &MessageContext, feature: &str, error: &str) {
    log_error(
        feature,
        error,
        Some(&user_label(m)),
        Some(chat_category(&m.chat)),
    );
}

/// Helper plugin untuk `anyhow::Error` dengan konteks pesan.
pub fn log_anyhow_from(m: &MessageContext, feature: &str, error: &anyhow::Error) {
    log_anyhow(
        feature,
        error,
        Some(&user_label(m)),
        Some(chat_category(&m.chat)),
    );
}

/// Info ringan untuk keperluan debug console.
pub fn log_info(scope: &str, message: &str) {
    emit(format!("{} - [INFO] {scope}: {message}", now_wib()));
}

/// Debug ringan untuk keperluan debug console.
pub fn log_debug(scope: &str, message: &str) {
    emit(format!("{} - [DEBUG] {scope}: {message}", now_wib()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_phone_numbers() {
        assert_eq!(format_phone("6282327829267"), "+62 823-2782-9267");
        assert_eq!(format_phone("082327829267"), "+62 823-2782-9267");
        assert_eq!(format_phone("+62 823-2782-9267"), "+62 823-2782-9267");
    }

    #[test]
    fn categorizes_chats() {
        assert_eq!(chat_category("62812@s.whatsapp.net"), "Private Chat");
        assert_eq!(chat_category("123@lid"), "Private Chat");
        assert_eq!(chat_category("abc@g.us"), "Group Chat");
        assert_eq!(chat_category("status@broadcast"), "Status / SW");
        assert_eq!(chat_category("x@newsletter"), "Channel / Newsletter");
        assert_eq!(chat_category("misterius@xxx"), "Lainnya");
    }

    #[test]
    fn prefer_phone_over_lid() {
        assert_eq!(
            format_sender("123456@lid", Some("6282327829267@s.whatsapp.net"), false),
            "+62 823-2782-9267"
        );
        assert_eq!(format_sender("123456@lid", None, false), "123456@lid");
        assert_eq!(format_sender("62812@s.whatsapp.net", None, true), "Bot");
    }

    #[test]
    fn detects_media_without_panicking() {
        let mut msg = wa::Message::default();
        assert_eq!(media_indicator(msg.get_base_message()), None);
        msg.sticker_message = whatsapp_rust::prelude::MessageField::some(Default::default());
        assert_eq!(media_indicator(msg.get_base_message()), Some("[Sticker]"));
    }
}
