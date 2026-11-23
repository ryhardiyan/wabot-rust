//! Penyimpanan SQLite untuk statistik pemakaian plugin.
//!
//! File default-nya `database/plugins.db` dan dibuat otomatis jika belum ada.
//! Tabel ini menyimpan jumlah hit (sukses/gagal/timeout), total waktu proses,
//! dan timestamp terakhir kali sebuah command dipanggil.

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::{path::Path, time::Duration};

/// Baris statistik yang siap ditulis ke SQLite (pakai `i64` sesuai tipe SQLite).
#[derive(Clone, Copy, Default)]
pub struct PersistedStat {
    pub ok: i64,
    pub fail: i64,
    pub timeout: i64,
    pub total_ms: i64,
    pub last_used: i64,
}

#[derive(Clone)]
pub struct PluginDb {
    pool: SqlitePool,
}

impl PluginDb {
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
            "CREATE TABLE IF NOT EXISTS plugin_stats (
                command TEXT PRIMARY KEY,
                ok INTEGER NOT NULL DEFAULT 0,
                fail INTEGER NOT NULL DEFAULT 0,
                timeout INTEGER NOT NULL DEFAULT 0,
                total_ms INTEGER NOT NULL DEFAULT 0,
                last_used INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&pool)
        .await
        .context("Gagal membuat tabel plugin_stats")?;
        Ok(Self { pool })
    }

    /// Muat seluruh baris saat startup agar hit lama tidak hilang setelah restart.
    pub async fn load(&self) -> Result<Vec<(String, PersistedStat)>> {
        let rows: Vec<(String, i64, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT command, ok, fail, timeout, total_ms, last_used FROM plugin_stats",
        )
        .fetch_all(&self.pool)
        .await
        .context("Gagal membaca statistik plugin")?;
        Ok(rows
            .into_iter()
            .map(|(command, ok, fail, timeout, total_ms, last_used)| {
                (
                    command,
                    PersistedStat {
                        ok,
                        fail,
                        timeout,
                        total_ms,
                        last_used,
                    },
                )
            })
            .collect())
    }

    /// Tulis ulang angka terbaru untuk satu command (bukan inkrement SQL),
    /// sehingga nilai di DB selalu identik dengan salinan in-memory.
    pub async fn upsert(&self, command: &str, stat: PersistedStat) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_stats (command, ok, fail, timeout, total_ms, last_used)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(command) DO UPDATE SET
                ok = excluded.ok,
                fail = excluded.fail,
                timeout = excluded.timeout,
                total_ms = excluded.total_ms,
                last_used = excluded.last_used",
        )
        .bind(command)
        .bind(stat.ok)
        .bind(stat.fail)
        .bind(stat.timeout)
        .bind(stat.total_ms)
        .bind(stat.last_used)
        .execute(&self.pool)
        .await
        .with_context(|| format!("Gagal menyimpan statistik /{command}"))?;
        Ok(())
    }
}
