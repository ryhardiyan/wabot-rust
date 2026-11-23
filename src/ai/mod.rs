//! Agent AI owner-only berbasis endpoint OpenAI-compatible (toolwiz).
//!
//! Alur: `.ai <prompt> [--delete]` -> sesi chat dimuat dari SQLite -> pesan
//! dikirim ke REST API -> bila model memanggil tool, tool dijalankan lokal di
//! VPS -> hasilnya dikembalikan ke model -> jawaban akhir dibalas ke owner.
//!
//! `--delete` di akhir argumen menghapus riwayat sesi chat untuk chat tersebut,
//! lalu (bila ada prompt) langsung memulai sesi baru dengan prompt itu.

pub mod client;
pub mod session;
pub mod tools;

use crate::plugins::App;
use anyhow::{bail, Result};

const SYSTEM_PROMPT: &str = "Kamu adalah asisten pribadi owner bot WhatsApp yang berjalan di VPS. \
Jawab dengan ringkas dan jelas dalam bahasa Indonesia. Kamu memiliki akses ke file-system VPS \
melalui tool yang tersedia: read_file, write_file, delete_file, list_dir, dan run_command. \
Gunakan tool bila diminta membaca, mengubah, menghapus file, atau menjalankan perintah. \
Selalu laporkan hasil akhir secara singkat setelah memakai tool. Jangan mengarang isi file \
bila belum membacanya.";

/// Jalankan satu permintaan AI. `reset` menghapus sesi lama lebih dulu.
pub async fn ask(app: &App, chat: &str, prompt: &str, reset: bool) -> Result<String> {
    if app.config.ai_api_key.trim().is_empty() {
        bail!("AI_API_KEY belum diisi di .env");
    }
    if app.config.ai_api_url.trim().is_empty() {
        bail!("AI_API_URL belum diisi di .env");
    }
    if reset {
        app.ai.delete(chat).await?;
    }
    if prompt.trim().is_empty() {
        return Ok("🗑️ Sesi chat dihapus. Kirim `.ai <prompt>` untuk memulai sesi baru.".into());
    }

    let settings = client::Settings {
        url: app.config.ai_api_url.clone(),
        key: app.config.ai_api_key.clone(),
        model: app.config.ai_model.clone(),
        root: app.config.ai_root.clone(),
        max_turns: app.config.ai_max_turns,
        history_messages: app.config.ai_history_messages,
        allow_secrets: app.config.ai_allow_secrets,
    };
    let mut messages = app.ai.load(chat).await?;
    if messages.is_empty() {
        messages.push(client::Message::system(SYSTEM_PROMPT));
    }
    messages.push(client::Message::user(prompt));

    let (answer, history) = client::complete(&settings, &app.http, messages).await?;
    app.ai.save(chat, &history).await?;
    Ok(answer)
}
