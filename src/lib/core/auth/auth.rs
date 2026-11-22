use anyhow::{Context, Result};
use std::path::Path;
use whatsapp_rust::store::SqliteStore;

pub async fn open(path: &Path) -> Result<SqliteStore> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Buat file baru dengan mode privat sebelum SQLite membuat WAL/SHM.
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    drop(options.open(path).await?);
    let store = SqliteStore::new(path.to_str().context("Path sesi bukan UTF-8")?)
        .await
        .context("Gagal membuka SqliteStore")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    }
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn opens_real_sqlite_session() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("session.db");
        let store = open(&path).await?;
        assert!(path.exists());
        drop(store);
        assert!(tokio::fs::read(path).await?.starts_with(b"SQLite format 3"));
        Ok(())
    }
}
