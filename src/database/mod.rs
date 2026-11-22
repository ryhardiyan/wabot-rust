use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::RwLock;

/// Semua mutasi diserialisasi sampai rename selesai; pembaca tidak melihat state setengah tertulis.
pub struct JsonDatabase<T> {
    path: PathBuf,
    data: Arc<RwLock<T>>,
}

impl<T: Serialize + DeserializeOwned + Default + Clone + Send + Sync + 'static> JsonDatabase<T> {
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let data = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("JSON rusak: {}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => T::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            path,
            data: Arc::new(RwLock::new(data)),
        })
    }
    pub async fn snapshot(&self) -> T {
        self.data.read().await.clone()
    }
    pub async fn update<R>(&self, edit: impl FnOnce(&mut T) -> Result<R> + Send) -> Result<R> {
        let mut guard = self.data.clone().write_owned().await;
        let mut next = guard.clone();
        let result = edit(&mut next)?;
        let bytes = serde_json::to_vec_pretty(&next)?;
        let path = self.path.clone();
        if bytes == serde_json::to_vec_pretty(&*guard)? {
            return Ok(result);
        }
        // Guard dan commit tetap dimiliki worker jika future pemanggil dibatalkan.
        // Mutasi berikutnya menunggu disk dan in-memory state kembali konsisten.
        tokio::task::spawn_blocking(move || -> Result<()> {
            atomic_write(&path, &bytes)?;
            *guard = next;
            Ok(())
        })
        .await??;
        Ok(result)
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn concurrent_updates_are_durable() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("data.json");
        let db = Arc::new(JsonDatabase::<u64>::open(&path).await?);
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..20 {
            let db = db.clone();
            tasks.spawn(async move {
                db.update(|n| {
                    *n += 1;
                    Ok(())
                })
                .await
            });
        }
        while let Some(r) = tasks.join_next().await {
            r??;
        }
        assert_eq!(db.snapshot().await, 20);
        assert_eq!(JsonDatabase::<u64>::open(path).await?.snapshot().await, 20);
        Ok(())
    }
    #[tokio::test]
    async fn corrupt_json_is_not_overwritten() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("bad.json");
        tokio::fs::write(&path, b"broken").await?;
        assert!(JsonDatabase::<u64>::open(&path).await.is_err());
        assert_eq!(tokio::fs::read(path).await?, b"broken");
        Ok(())
    }
    #[tokio::test]
    async fn failing_edit_does_not_commit() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("data.json");
        let db = JsonDatabase::<u64>::open(&path).await?;
        let result: Result<()> = db
            .update(|v| {
                *v = 99;
                anyhow::bail!("fixture edit failed")
            })
            .await;
        assert!(result.is_err());
        assert_eq!(db.snapshot().await, 0);
        assert!(!path.exists());
        Ok(())
    }
}
