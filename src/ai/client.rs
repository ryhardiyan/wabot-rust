//! Klien REST OpenAI-compatible untuk endpoint toolwiz.

use super::tools;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

/// Konfigurasi koneksi yang diambil dari `.env`.
#[derive(Clone)]
pub struct Settings {
    pub url: String,
    pub key: String,
    pub model: String,
    pub root: PathBuf,
    pub max_turns: usize,
    pub history_messages: usize,
    pub allow_secrets: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: Some(text.into()),
            ..Default::default()
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: Some(text.into()),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

/// Kirim percakapan ke API, jalankan tool bila diminta, ulangi sampai jawaban akhir.
/// Riwayat yang dikembalikan sudah termasuk pesan tool, sehingga sesi berikutnya
/// tetap punya konteks penuh.
pub async fn complete(
    settings: &Settings,
    http: &reqwest::Client,
    mut messages: Vec<Message>,
) -> Result<(String, Vec<Message>)> {
    messages = super::session::trim_history(messages, settings.history_messages);
    let endpoint = format!("{}/chat/completions", settings.url.trim_end_matches('/'));
    let tool_defs = tools::definitions();

    for _ in 0..settings.max_turns {
        let body = json!({
            "model": settings.model.clone(),
            "messages": messages.clone(),
            "tools": tool_defs.clone(),
            "tool_choice": "auto",
        });
        let response = http
            .post(&endpoint)
            .bearer_auth(&settings.key)
            .json(&body)
            .send()
            .await
            .context("Gagal menghubungi AI API")?;
        let status = response.status();
        let raw = response
            .text()
            .await
            .context("Gagal membaca respons AI API")?;
        if !status.is_success() {
            let preview: String = raw.chars().take(500).collect();
            bail!("AI API mengembalikan {status}: {preview}");
        }
        let parsed: ApiResponse =
            serde_json::from_str(&raw).context("Respons AI API bukan JSON yang dikenal")?;
        let assistant = parsed
            .choices
            .into_iter()
            .next()
            .context("AI API tidak mengembalikan pilihan jawaban")?
            .message;
        let calls = assistant.tool_calls.clone().unwrap_or_default();
        messages.push(assistant.clone());

        if calls.is_empty() {
            return Ok((assistant.content.unwrap_or_default(), messages));
        }
        for call in calls {
            let result = tools::execute(
                &settings.root,
                settings.allow_secrets,
                &call.function.name,
                &call.function.arguments,
            )
            .await;
            messages.push(Message {
                role: "tool".into(),
                content: Some(result),
                tool_call_id: Some(call.id),
                ..Default::default()
            });
        }
    }
    bail!(
        "AI melebihi {} putaran pemanggilan tool",
        settings.max_turns
    )
}

#[allow(dead_code)] // Pembantu debugging respons mentah saat mengintegrasikan provider baru.
pub fn preview_value(value: &Value) -> String {
    value.to_string().chars().take(300).collect()
}
