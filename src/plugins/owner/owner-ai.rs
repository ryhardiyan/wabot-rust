use crate::{
    ai::client::{Progress, ProgressReporter},
    bot_lib::core::socket::WaSocket,
    plugins::Context,
};
use anyhow::Result;
use std::sync::Arc;
use whatsapp_rust::{prelude::MessageBuilderExt, waproto::whatsapp as wa, Jid};

/// Batas karakter untuk edit pesan WhatsApp. Jika jawaban lebih panjang,
/// kirim pesan baru agar edit tidak gagal di sisi server.
const EDIT_LIMIT: usize = 6_000;

/// Mengubah pesan status yang sudah dikirim menjadi progres/jawaban akhir.
/// WhatsApp mengenali edit melalui id pesan aslinya, jadi id disimpan tetap.
struct StatusReporter {
    socket: WaSocket,
    chat: Jid,
    message_id: String,
}

impl StatusReporter {
    async fn edit(&self, text: impl Into<String>) -> Result<()> {
        let text = text.into();
        crate::bot_lib::core::logger::log_outgoing(&self.chat.to_string(), &text);
        self.socket
            .client
            .edit_message(
                self.chat.clone(),
                self.message_id.clone(),
                wa::Message::text(text),
            )
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl ProgressReporter for StatusReporter {
    async fn report(&self, progress: Progress) {
        let text = match progress {
            Progress::Retrying {
                attempt,
                max_attempts,
                status: Some(code),
            } => format!(
                "⚠️ Server AI sedang sibuk (HTTP {code}), mencoba menghubungkan ulang... (Percobaan {attempt}/{max_attempts})"
            ),
            Progress::Retrying {
                attempt,
                max_attempts,
                status: None,
            } => format!(
                "⚠️ Koneksi AI terputus, mencoba menghubungkan ulang... (Percobaan {attempt}/{max_attempts})"
            ),
            Progress::WaitingResponse => "✍️ Menunggu respons lengkap dari server...".into(),
        };
        if let Err(error) = self.edit(text).await {
            tracing::warn!(%error, "Gagal memperbarui status AI; pesan status lama tetap tampil");
        }
    }
}

crate::plugins::plugin!(&["ai"], "owner", execute, owner_only = true);

/// `.ai <prompt> [--delete]` — asisten AI owner-only dengan sesi chat persisten.
/// `--delete` di akhir argumen menghapus sesi chat chat ini sebelum memproses prompt.
async fn execute(ctx: &Context) -> Result<()> {
    let mut args = ctx.args.clone();
    let reset = args.last().is_some_and(|arg| arg == "--delete");
    if reset {
        args.pop();
    }
    let prompt = args.join(" ");
    if !reset && prompt.is_empty() {
        return ctx
            .reply(
                "🤖 `.ai <prompt> [--delete]`\n\
                 Contoh: `.ai baca Cargo.toml lalu ringkas dependensinya`\n\
                 `.ai hapus file tmp/old.txt`\n\
                 `.ai lanjutkan penjelasan sebelumnya --delete` (hapus sesi lama & mulai baru)",
            )
            .await;
    }

    // Kirim status lebih dulu, lalu id-nya dipakai untuk edit progres/results.
    let initial = "⏳ Sedang memproses permintaan...";
    let chat = ctx.m.chat.parse::<Jid>()?;
    let sent = ctx
        .m
        .socket
        .client
        .send_message(&chat, wa::Message::text(initial))
        .await?;
    crate::bot_lib::core::logger::log_outgoing(&ctx.m.chat, initial);
    let reporter = Arc::new(StatusReporter {
        socket: ctx.m.socket.clone(),
        chat,
        message_id: sent.message_id,
    });

    match crate::ai::ask(
        &ctx.app,
        &ctx.m.chat,
        &prompt,
        reset,
        Some(reporter.clone()),
    )
    .await
    {
        Ok(answer) => {
            if answer.chars().count() > EDIT_LIMIT {
                return ctx.reply(answer).await;
            }
            if let Err(error) = reporter.edit(answer.clone()).await {
                tracing::warn!(%error, "Edit jawaban AI gagal; jawaban dikirim sebagai pesan baru");
                return ctx.reply(answer).await;
            }
            Ok(())
        }
        Err(error) => {
            tracing::warn!(command = "ai", error = %error, "AI gagal");
            let text = format!("⚠️ AI gagal: {error}");
            if text.chars().count() <= EDIT_LIMIT && reporter.edit(text.clone()).await.is_ok() {
                return Ok(());
            }
            ctx.reply(text).await
        }
    }
}
