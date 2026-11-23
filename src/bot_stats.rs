//! Statistik pemakaian plugin. Data agregat dipertahankan in-memory untuk
//! rendering cepat, lalu ditulis ke SQLite (`database/plugins.db`) setiap ada
//! eksekusi supaya hit dan "terakhir digunakan" tetap ada setelah restart.

use crate::stats_db::{PersistedStat, PluginDb};
use anyhow::Result;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Hasil eksekusi satu perintah.
#[derive(Clone, Copy, Debug)]
pub enum Outcome {
    Ok,
    Fail,
    Timeout,
}

/// Statistik agregat untuk satu command.
#[derive(Clone, Copy, Debug, Default)]
pub struct CommandStat {
    pub ok: u64,
    pub fail: u64,
    pub timeout: u64,
    pub total_ms: u64,
    /// Unix timestamp (detik) terakhir kali command dipanggil.
    pub last_used: i64,
}

impl CommandStat {
    pub fn total(&self) -> u64 {
        self.ok + self.fail + self.timeout
    }
    pub fn avg_ms(&self) -> u64 {
        let total = self.total();
        if total == 0 {
            0
        } else {
            self.total_ms / total
        }
    }
    pub fn success_pct(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            self.ok as f64 * 100.0 / total as f64
        }
    }
}

#[derive(Clone, Debug)]
pub struct LastError {
    pub command: String,
    pub message: String,
    pub at: chrono::DateTime<chrono::Utc>,
}

#[derive(Default)]
struct Inner {
    per_command: BTreeMap<String, CommandStat>,
    last_errors: Vec<LastError>,
}

/// Kumpulan statistik bot yang aman dipakai secara konkuren.
pub struct Stats {
    started: Instant,
    inner: Mutex<Inner>,
    db: Option<PluginDb>,
    /// Memastikan update memory dan write DB tetap berurutan saat command bersamaan.
    write: tokio::sync::Mutex<()>,
}

impl Stats {
    /// Buka/muat DB statistik. Dipanggil sekali saat aplikasi start.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = PluginDb::open(path).await?;
        let mut inner = Inner::default();
        for (command, stat) in db.load().await? {
            inner.per_command.insert(
                command,
                CommandStat {
                    ok: stat.ok.max(0) as u64,
                    fail: stat.fail.max(0) as u64,
                    timeout: stat.timeout.max(0) as u64,
                    total_ms: stat.total_ms.max(0) as u64,
                    last_used: stat.last_used,
                },
            );
        }
        Ok(Self {
            started: Instant::now(),
            inner: Mutex::new(inner),
            db: Some(db),
            write: tokio::sync::Mutex::new(()),
        })
    }

    /// Varian tanpa DB, dipakai unit test murni.
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            inner: Mutex::new(Inner::default()),
            db: None,
            write: tokio::sync::Mutex::new(()),
        }
    }

    /// Catat satu eksekusi perintah lalu simpan ke DB.
    pub async fn record(
        &self,
        command: &str,
        outcome: Outcome,
        elapsed: Duration,
        error: Option<&str>,
    ) {
        let _write = self.write.lock().await;
        let persisted = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            let stat = inner.per_command.entry(command.to_string()).or_default();
            let millis = elapsed.as_millis().min(u64::MAX as u128) as u64;
            stat.total_ms = stat.total_ms.saturating_add(millis);
            stat.last_used = chrono::Utc::now().timestamp();
            match outcome {
                Outcome::Ok => stat.ok += 1,
                Outcome::Fail => stat.fail += 1,
                Outcome::Timeout => stat.timeout += 1,
            }
            if !matches!(outcome, Outcome::Ok) {
                let message = match outcome {
                    Outcome::Timeout => "timeout".to_string(),
                    _ => error.unwrap_or("error").chars().take(160).collect(),
                };
                inner.last_errors.insert(
                    0,
                    LastError {
                        command: command.to_string(),
                        message,
                        at: chrono::Utc::now(),
                    },
                );
                inner.last_errors.truncate(5);
            }
            PersistedStat {
                ok: stat.ok as i64,
                fail: stat.fail as i64,
                timeout: stat.timeout as i64,
                total_ms: stat.total_ms as i64,
                last_used: stat.last_used,
            }
        };
        if let Some(db) = &self.db {
            if let Err(error) = db.upsert(command, persisted).await {
                tracing::warn!(%error, command = %command, "Gagal menyimpan statistik ke database");
            }
        }
    }

    pub fn uptime(&self) -> Duration {
        self.started.elapsed()
    }

    /// Salinan data siap dirender (tidak menahan lock).
    pub fn snapshot(&self) -> Snapshot {
        let Ok(inner) = self.inner.lock() else {
            return Snapshot {
                uptime: self.uptime(),
                ..Snapshot::default()
            };
        };
        let mut commands: Vec<(String, CommandStat)> = inner
            .per_command
            .iter()
            .map(|(name, stat)| (name.clone(), *stat))
            .collect();
        commands.sort_by(|a, b| b.1.total().cmp(&a.1.total()));
        let mut ok = 0;
        let mut fail = 0;
        let mut timeout = 0;
        let mut total_ms = 0;
        for (_, stat) in &commands {
            ok += stat.ok;
            fail += stat.fail;
            timeout += stat.timeout;
            total_ms += stat.total_ms;
        }
        Snapshot {
            uptime: self.uptime(),
            commands,
            ok,
            fail,
            timeout,
            total_ms,
            last_errors: inner.last_errors.clone(),
        }
    }
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub uptime: Duration,
    /// Diurutkan dari command paling sering dipanggil.
    pub commands: Vec<(String, CommandStat)>,
    pub ok: u64,
    pub fail: u64,
    pub timeout: u64,
    pub total_ms: u64,
    pub last_errors: Vec<LastError>,
}

impl Snapshot {
    pub fn processed(&self) -> u64 {
        self.ok + self.fail + self.timeout
    }
    pub fn avg_ms(&self) -> u64 {
        let processed = self.processed();
        if processed == 0 {
            0
        } else {
            self.total_ms / processed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn records_success_failure_and_timeout() {
        let stats = Stats::new();
        stats
            .record("ping", Outcome::Ok, Duration::from_millis(40), None)
            .await;
        stats
            .record("ping", Outcome::Ok, Duration::from_millis(60), None)
            .await;
        stats
            .record(
                "stalk",
                Outcome::Fail,
                Duration::from_millis(100),
                Some("boom"),
            )
            .await;
        stats
            .record("fetch", Outcome::Timeout, Duration::from_millis(200), None)
            .await;

        let snap = stats.snapshot();
        assert_eq!(snap.processed(), 4);
        assert_eq!(snap.ok, 2);
        assert_eq!(snap.fail, 1);
        assert_eq!(snap.timeout, 1);
        assert_eq!(snap.avg_ms(), 100);
        assert_eq!(snap.commands[0].0, "ping");
        assert_eq!(snap.commands[0].1.avg_ms(), 50);
        assert!(snap.commands.iter().all(|(_, s)| s.last_used > 0));
    }

    #[tokio::test]
    async fn keeps_only_recent_errors_and_redacts_length() {
        let stats = Stats::new();
        for _ in 0..8 {
            stats
                .record(
                    "cmd",
                    Outcome::Fail,
                    Duration::from_millis(1),
                    Some(&"x".repeat(500)),
                )
                .await;
        }
        let snap = stats.snapshot();
        assert_eq!(snap.last_errors.len(), 5);
        assert!(snap
            .last_errors
            .iter()
            .all(|e| e.message.chars().count() == 160));
    }

    #[test]
    fn empty_snapshot_is_safe() {
        let snap = Stats::new().snapshot();
        assert_eq!(snap.processed(), 0);
        assert_eq!(snap.avg_ms(), 0);
        assert!(snap.commands.is_empty());
        assert!(snap.last_errors.is_empty());
    }
}
