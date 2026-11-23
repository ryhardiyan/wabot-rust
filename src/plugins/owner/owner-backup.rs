use crate::plugins::Context;
use anyhow::{Context as _, Result};
use std::{
    collections::HashSet,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

crate::plugins::plugin!(&["backup"], "owner", execute, owner_only = true);

const BACKUP_DIR: &str = "backups";
const RETENTION_DAYS: i64 = 7;

/// Direktori sumber diambil dari konfigurasi (bukan path hard-coded) supaya
/// override lewat .env tetap tercakup. File tanpa parent dibackup satu-satu.
fn backup_roots(ctx: &Context) -> Vec<PathBuf> {
    let candidates = [
        ctx.app.config.database_path.clone(),
        ctx.app.config.session_path.clone(),
        ctx.app.config.verify_db_path.clone(),
        ctx.app.config.plugin_db_path.clone(),
    ];
    let mut roots = Vec::new();
    for path in candidates {
        let root = match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => path.clone(),
        };
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

/// Telusuri rekursif. Symlink dan special file dilewat demi keamanan.
async fn files_in(root: &Path) -> Result<Vec<PathBuf>> {
    if root.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e).with_context(|| format!("Gagal membaca {}", dir.display())),
        };
        while let Some(entry) = entries.next_entry().await? {
            let kind = entry.file_type().await?;
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                found.push(entry.path());
            }
        }
    }
    found.sort();
    Ok(found)
}

/// Nama entri di dalam zip, relatif terhadap root-nya.
fn entry_name(root: &Path, file: &Path, multiple_roots: bool) -> Result<String> {
    let relative = if root.is_file() {
        PathBuf::from(root.file_name().context("Nama file tidak valid")?)
    } else {
        file.strip_prefix(root)
            .with_context(|| format!("{} di luar {}", file.display(), root.display()))?
            .to_path_buf()
    };
    let named = if multiple_roots {
        let prefix = root
            .file_name()
            .map(|name| name.to_os_string())
            .unwrap_or_else(|| "root".into());
        PathBuf::from(prefix).join(relative)
    } else {
        relative
    };
    Ok(named.to_string_lossy().replace('\\', "/"))
}

/// Arsip dibuat di memori. Stored dipilih karena SQLite/JSON sudah terkompresi
/// dan crate zip dipakai tanpa default features (tanpa backend deflate).
fn make_zip(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o600);
    for (name, bytes) in entries {
        archive
            .start_file(name, options)
            .with_context(|| format!("Gagal menambahkan {name} ke arsip"))?;
        archive
            .write_all(bytes)
            .with_context(|| format!("Gagal menulis {name} ke arsip"))?;
    }
    let cursor = archive.finish().context("Gagal menutup arsip zip")?;
    Ok(cursor.into_inner())
}

/// Sisa backup lama (termasuk folder staging plugin versi sebelumnya) dibersihkan
/// supaya kegagalan kirim berulang tidak menumpuk di disk.
async fn prune_old_backups() -> Result<usize> {
    let mut entries = match tokio::fs::read_dir(BACKUP_DIR).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e).context("Gagal membaca folder backups"),
    };
    let cutoff = chrono::Utc::now() - chrono::Duration::days(RETENTION_DAYS);
    let mut removed = 0;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let modified = match entry.metadata().await.and_then(|meta| meta.modified()) {
            Ok(time) => time,
            Err(_) => continue,
        };
        if chrono::DateTime::<chrono::Utc>::from(modified) >= cutoff {
            continue;
        }
        let result = if path.is_dir() {
            tokio::fs::remove_dir_all(&path).await
        } else {
            tokio::fs::remove_file(&path).await
        };
        match result {
            Ok(()) => removed += 1,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Gagal menghapus backup lama")
            }
        }
    }
    Ok(removed)
}

async fn execute(ctx: &Context) -> Result<()> {
    let owner = ctx.app.config.owner_jid.as_str();
    anyhow::ensure!(!owner.is_empty(), "OWNER_JID / OWNER_PHONE belum diisi");

    let roots = backup_roots(ctx);
    anyhow::ensure!(!roots.is_empty(), "Lokasi database belum dikonfigurasi");

    // Kumpulkan semua file dari semua root, dedup kalau ada path yang tumpang tindih.
    let mut seen = HashSet::new();
    let mut sources: Vec<(PathBuf, String)> = Vec::new();
    for root in &roots {
        for file in files_in(root).await? {
            if !seen.insert(file.clone()) {
                continue;
            }
            sources.push((file.clone(), entry_name(root, &file, roots.len() > 1)?));
        }
    }
    anyhow::ensure!(!sources.is_empty(), "Tidak ada file untuk dicadangkan");

    let mut entries = Vec::with_capacity(sources.len());
    for (file, name) in &sources {
        let bytes = tokio::fs::read(file)
            .await
            .with_context(|| format!("Gagal membaca {}", file.display()))?;
        entries.push((name.clone(), bytes));
    }
    let zip_bytes = make_zip(&entries)?;

    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let name = format!("wabot-backup-{stamp}.zip");
    let path = Path::new(BACKUP_DIR).join(&name);
    tokio::fs::create_dir_all(BACKUP_DIR)
        .await
        .context("Gagal menyiapkan folder backups")?;
    tokio::fs::write(&path, &zip_bytes)
        .await
        .with_context(|| format!("Gagal menyimpan {}", path.display()))?;

    let caption = format!(
        "🗄️ Backup database bot\n📦 {name}\n📄 {} file • {} KiB\n🕐 {stamp} UTC",
        sources.len(),
        zip_bytes.len() / 1024
    );
    let sent = ctx
        .m
        .socket
        .send_media(owner, zip_bytes, "application/zip", &name, &caption, None)
        .await;

    match sent {
        Ok(()) => {
            if let Err(e) = tokio::fs::remove_file(&path).await {
                tracing::warn!(path = %path.display(), error = %e, "Gagal menghapus backup setelah kirim");
            }
            let pruned = prune_old_backups().await.unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Gagal membersihkan backup lama");
                0
            });
            let cleaned = if pruned > 0 {
                format!(" • {pruned} backup lama dibersihkan")
            } else {
                String::new()
            };
            ctx.reply(format!(
                "✅ Backup terkirim ke chat privat owner.\n📦 {name} • {} file\n🧹 File lokal dihapus{cleaned}",
                sources.len()
            ))
            .await
        }
        Err(error) => {
            ctx.reply(format!(
                "⚠️ Backup gagal dikirim: {error}\nArsip disimpan di {} untuk recovery manual.",
                path.display()
            ))
            .await?;
            Err(error).context("Gagal mengirim backup ke owner")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_keeps_every_entry() -> Result<()> {
        let bytes = make_zip(&[
            ("users.json".into(), b"{}".to_vec()),
            ("nested/session.db".into(), vec![0u8, 1, 2, 3, 4]),
        ])?;
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        assert_eq!(archive.len(), 2);
        let mut users = archive.by_name("users.json")?;
        let mut content = String::new();
        std::io::Read::read_to_string(&mut users, &mut content)?;
        assert_eq!(content, "{}");
        drop(users);
        assert_eq!(archive.by_name("nested/session.db")?.size(), 5);
        Ok(())
    }

    #[tokio::test]
    async fn files_in_walks_recursively() -> Result<()> {
        let dir = tempfile::tempdir()?;
        tokio::fs::write(dir.path().join("a.json"), "a").await?;
        tokio::fs::create_dir(dir.path().join("sub")).await?;
        tokio::fs::write(dir.path().join("sub/b.db"), "b").await?;
        let files = files_in(dir.path()).await?;
        assert_eq!(files.len(), 2);
        assert_eq!(entry_name(dir.path(), &files[0], false)?, "a.json");
        assert_eq!(entry_name(dir.path(), &files[1], false)?, "sub/b.db");
        Ok(())
    }
}
