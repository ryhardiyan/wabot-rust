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

/// Nomor telepon kanonik (JID `@s.whatsapp.net`) dari sebuah JID/identifier.
/// LID tidak bisa dikonversi ke nomor hanya dengan membuang `@lid`-nya.
pub fn canonical_pn(value: &str) -> Option<String> {
    let raw = value.trim();
    let user = raw.split('@').next().unwrap_or(raw);
    let phone = crate::config::clean_number(user);
    ((8..=15).contains(&phone.len()) && phone.starts_with("62"))
        .then(|| format!("{phone}@s.whatsapp.net"))
}

/// JID LID kanonik dari sebuah identifier (mis. `123456789@lid`).
pub fn canonical_lid(value: &str) -> Option<String> {
    let raw = value.trim();
    let (user, server) = raw.split_once('@')?;
    (!user.is_empty() && server.eq_ignore_ascii_case("lid")).then(|| format!("{user}@lid"))
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
        // Relasi LID <-> nomor telepon dipelajari dari setiap pesan masuk.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS identity_links (
                lid TEXT PRIMARY KEY,
                jid TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .context("Gagal membuat tabel identity_links")?;
        sqlx::query("CREATE INDEX IF NOT EXISTS identity_links_jid ON identity_links(jid)")
            .execute(&pool)
            .await
            .context("Gagal membuat indeks identity_links")?;
        Ok(Self { pool })
    }

    /// Simpan pasangan LID dan JID telepon. Dipanggil setiap ada pesan masuk
    /// yang membawa keduanya (header `sender`/`sender_pn` atau hasil lookup store).
    pub async fn link_identity(&self, lid: &str, jid: &str) -> Result<()> {
        let Some(lid) = canonical_lid(lid) else {
            return Ok(());
        };
        let Some(jid) = canonical_pn(jid) else {
            return Ok(());
        };
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO identity_links (lid, jid, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(lid) DO UPDATE SET jid = excluded.jid, updated_at = excluded.updated_at",
        )
        .bind(&lid)
        .bind(&jid)
        .bind(now)
        .execute(&self.pool)
        .await
        .context("Gagal menyimpan relasi LID/JID")?;
        Ok(())
    }

    /// Nomor telepon (JID PN) yang terhubung dengan sebuah LID.
    pub async fn jid_for_lid(&self, lid: &str) -> Result<Option<String>> {
        let Some(lid) = canonical_lid(lid) else {
            return Ok(None);
        };
        let row: Option<(String,)> = sqlx::query_as("SELECT jid FROM identity_links WHERE lid = ?")
            .bind(&lid)
            .fetch_optional(&self.pool)
            .await
            .context("Gagal membaca relasi LID/JID")?;
        Ok(row.map(|value| value.0))
    }

    /// LID terakhir yang terhubung dengan sebuah nomor telepon.
    pub async fn lid_for_jid(&self, jid: &str) -> Result<Option<String>> {
        let Some(jid) = canonical_pn(jid) else {
            return Ok(None);
        };
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT lid FROM identity_links WHERE jid = ? ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(&jid)
        .fetch_optional(&self.pool)
        .await
        .context("Gagal membaca relasi JID/LID")?;
        Ok(row.map(|value| value.0))
    }

    /// Identitas alternatif dari sebuah JID/LID, jika sudah dipelajari.
    async fn alias(&self, value: &str) -> Result<Option<String>> {
        if canonical_lid(value).is_some() {
            self.jid_for_lid(value).await
        } else {
            self.lid_for_jid(value).await
        }
    }

    /// Semua kunci status yang mungkin: bentuk kanonik, identifier asli, dan aliasnya.
    async fn keys(&self, value: &str) -> Result<Vec<String>> {
        let canonical = canonical_lid(value)
            .or_else(|| canonical_pn(value))
            .unwrap_or_else(|| value.to_string());
        let mut keys = vec![canonical.clone()];
        if value != canonical {
            keys.push(value.to_string());
        }
        if let Some(alias) = self.alias(&canonical).await? {
            if !keys.contains(&alias) {
                keys.push(alias);
            }
        }
        Ok(keys)
    }

    /// Kunci kanonik untuk tulis: nomor telepon bila LID sudah dipetakan,
    /// sehingga status tidak terduplikasi saat LID berubah.
    async fn write_key(&self, value: &str) -> Result<String> {
        if canonical_lid(value).is_some() {
            if let Some(jid) = self.jid_for_lid(value).await? {
                return Ok(jid);
            }
        }
        Ok(canonical_pn(value).unwrap_or_else(|| value.to_string()))
    }

    /// Status untuk JID mana pun, termasuk LID yang sudah dipetakan ke nomor.
    /// Jika salah satu identitas terverifikasi, maka statusnya terverifikasi.
    pub async fn status(&self, jid: &str) -> Result<Status> {
        let mut pending = false;
        for key in self.keys(jid).await? {
            let row: Option<(String,)> = sqlx::query_as("SELECT status FROM users WHERE jid = ?")
                .bind(&key)
                .fetch_optional(&self.pool)
                .await
                .context("Gagal membaca status verifikasi")?;
            match row.map(|value| Status::from_str(&value.0)) {
                Some(Status::Verified) => return Ok(Status::Verified),
                Some(Status::Pending) => pending = true,
                _ => (),
            }
        }
        Ok(if pending {
            Status::Pending
        } else {
            Status::Unregistered
        })
    }

    /// Menyimpan/memperbarui JID menjadi `pending`. `created_at` hanya diisi saat insert pertama.
    pub async fn set_pending(&self, jid: &str) -> Result<()> {
        let key = self.write_key(jid).await?;
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO users (jid, status, created_at) VALUES (?, 'pending', ?)
             ON CONFLICT(jid) DO UPDATE SET status = 'pending'",
        )
        .bind(&key)
        .bind(now)
        .execute(&self.pool)
        .await
        .context("Gagal menyimpan status pending")?;
        Ok(())
    }

    /// Mengubah status menjadi `verified`. Mengembalikan true jika ada baris yang berubah.
    pub async fn set_verified(&self, jid: &str) -> Result<bool> {
        let key = self.write_key(jid).await?;
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            "INSERT INTO users (jid, status, created_at) VALUES (?, 'verified', ?)
             ON CONFLICT(jid) DO UPDATE SET status = 'verified'",
        )
        .bind(&key)
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

    #[tokio::test]
    async fn status_follows_lid_alias() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = VerifyDb::open(dir.path().join("verify.db")).await?;
        let jid = "08123456789@s.whatsapp.net";
        let lid = "123456789@lid";
        db.link_identity(lid, jid).await?;
        assert_eq!(
            db.jid_for_lid(lid).await?.as_deref(),
            Some("628123456789@s.whatsapp.net")
        );
        assert_eq!(db.lid_for_jid(jid).await?.as_deref(), Some(lid));
        db.set_verified(jid).await?;
        assert_eq!(db.status(lid).await?, Status::Verified);
        assert_eq!(db.status(jid).await?, Status::Verified);
        db.set_pending(lid).await?;
        assert_eq!(
            db.status("628123456789@s.whatsapp.net").await?,
            Status::Pending
        );
        Ok(())
    }

    #[test]
    fn canonical_forms_ignore_invalid_identifiers() {
        assert_eq!(canonical_lid("123@lid"), Some("123@lid".into()));
        assert_eq!(canonical_lid("123@s.whatsapp.net"), None);
        assert_eq!(
            canonical_pn("0812-3456 789@s.whatsapp.net"),
            Some("628123456789@s.whatsapp.net".into())
        );
        assert_eq!(canonical_pn("123@lid"), None);
    }
}
