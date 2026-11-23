//! Riwayat chat AI per chat, disimpan di SQLite (`database/ai.db`).
//! Satu chat WhatsApp = satu sesi. `--delete` menghapus baris ini.

use super::client::Message;
use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::{path::Path, time::Duration};

#[derive(Clone)]
pub struct AiSession {
    pool: SqlitePool,
}

impl AiSession {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Gagal membuat folder {}", parent.display()))?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .with_context(|| format!("Gagal membuka database {}", path.display()))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ai_sessions (
                chat TEXT PRIMARY KEY,
                messages TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .context("Gagal membuat tabel ai_sessions")?;
        Ok(Self { pool })
    }

    pub async fn load(&self, chat: &str) -> Result<Vec<Message>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT messages FROM ai_sessions WHERE chat = ?")
                .bind(chat)
                .fetch_optional(&self.pool)
                .await
                .context("Gagal memuat sesi AI")?;
        match row {
            Some((raw,)) => serde_json::from_str(&raw).context("Sesi AI rusak"),
            None => Ok(Vec::new()),
        }
    }

    pub async fn save(&self, chat: &str, messages: &[Message]) -> Result<()> {
        let raw = serde_json::to_string(messages).context("Gagal menyusun sesi AI")?;
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO ai_sessions (chat, messages, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(chat) DO UPDATE SET messages = excluded.messages, updated_at = excluded.updated_at",
        )
        .bind(chat)
        .bind(raw)
        .bind(now)
        .execute(&self.pool)
        .await
        .context("Gagal menyimpan sesi AI")?;
        Ok(())
    }

    pub async fn delete(&self, chat: &str) -> Result<()> {
        sqlx::query("DELETE FROM ai_sessions WHERE chat = ?")
            .bind(chat)
            .execute(&self.pool)
            .await
            .context("Gagal menghapus sesi AI")?;
        Ok(())
    }
}

/// Pertahankan pesan system + N giliran terakhir. Pemotongan dilakukan hanya
/// di awal giliran user agar pasangan `tool_calls`/`tool` tidak terpisah.
pub fn trim_history(messages: Vec<Message>, limit_user_turns: usize) -> Vec<Message> {
    let starts: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == "user")
        .map(|(i, _)| i)
        .collect();
    if starts.len() <= limit_user_turns {
        return messages;
    }
    let cut = starts[starts.len() - limit_user_turns];
    let mut out: Vec<Message> = messages
        .iter()
        .filter(|m| m.role == "system")
        .cloned()
        .collect();
    out.extend_from_slice(&messages[cut..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_menyimpan_system_dan_akhir_saja() {
        let mut messages = vec![Message::system("sistem")];
        for i in 0..5 {
            messages.push(Message::user(format!("pertanyaan {i}")));
        }
        let trimmed = trim_history(messages, 2);
        assert_eq!(trimmed.len(), 3);
        assert_eq!(trimmed[0].role, "system");
        assert_eq!(trimmed[1].content.as_deref(), Some("pertanyaan 3"));
    }
}
