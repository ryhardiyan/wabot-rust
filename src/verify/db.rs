use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::{path::Path, time::Duration};

/// Status enumerasi untuk setiap JID yang terdaftar dalam sistem verifikasi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Unregistered,
    Pending,
    Verified,
}

impl Status {
    #[allow(dead_code)] // Dipakai test dan berguna untuk logging status.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unregistered => "unregistered",
            Self::Pending => "pending",
            Self::Verified => "verified",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "pending" => Self::Pending,
            "verified" => Self::Verified,
            _ => Self::Unregistered,
        }
    }
}

/// Penyimpanan verifikasi berbasis SQLite. `SqlitePool` aman dipakai lintas task Tokio.
#[derive(Clone)]
pub struct VerifyDb {
    pool: SqlitePool,
}

impl VerifyDb {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Gagal membuat folder database {}", parent.display()))?;
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
            "CREATE TABLE IF NOT EXISTS users (
                jid TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .context("Gagal membuat tabel users")?;
        Ok(Self { pool })
    }

    pub async fn status(&self, jid: &str) -> Result<Status> {
        let row: Option<(String,)> = sqlx::query_as("SELECT status FROM users WHERE jid = ?")
            .bind(jid)
            .fetch_optional(&self.pool)
            .await
            .context("Gagal membaca status verifikasi")?;
        Ok(row
            .map(|value| Status::from_str(&value.0))
            .unwrap_or(Status::Unregistered))
    }

    /// Menyimpan/memperbarui JID menjadi `pending`. `created_at` hanya diisi saat insert pertama.
    pub async fn set_pending(&self, jid: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO users (jid, status, created_at) VALUES (?, 'pending', ?)
             ON CONFLICT(jid) DO UPDATE SET status = 'pending'",
        )
        .bind(jid)
        .bind(now)
        .execute(&self.pool)
        .await
        .context("Gagal menyimpan status pending")?;
        Ok(())
    }

    /// Mengubah status menjadi `verified`. Mengembalikan true jika ada baris yang berubah.
    pub async fn set_verified(&self, jid: &str) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            "INSERT INTO users (jid, status, created_at) VALUES (?, 'verified', ?)
             ON CONFLICT(jid) DO UPDATE SET status = 'verified'",
        )
        .bind(jid)
        .bind(now)
        .execute(&self.pool)
        .await
        .context("Gagal menyimpan status verified")?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn status_lifecycle() -> Result<()> {
        // sqlite::memory: tidak dipakai agar periode koneksi pool tidak menghilangkan skema.
        let dir = tempfile::tempdir()?;
        let db = VerifyDb::open(dir.path().join("verify.db")).await?;
        let jid = "628123456789@s.whatsapp.net";
        assert_eq!(db.status(jid).await?, Status::Unregistered);
        db.set_pending(jid).await?;
        assert_eq!(db.status(jid).await?, Status::Pending);
        assert!(db.set_verified(jid).await?);
        assert_eq!(db.status(jid).await?, Status::Verified);
        assert_eq!(Status::Verified.as_str(), "verified");
        Ok(())
    }
}
