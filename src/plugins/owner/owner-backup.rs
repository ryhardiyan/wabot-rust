use crate::plugins::Context;
use anyhow::{Context as _, Result};
use std::path::{Path, PathBuf};

crate::plugins::plugin!(
    &["backup"],
    "owner",
    "Cadangkan data bot ke direktori backups/",
    execute,
    owner_only = true
);

async fn copy_sources(root: &Path, sources: &[(&str, PathBuf)]) -> Result<Vec<String>> {
    let mut copied = Vec::new();
    for (label, source) in sources {
        if !source.exists() {
            continue;
        }
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .context("Nama file sumber tidak valid")?;
        let target = root.join(format!("{label}-{name}"));
        tokio::fs::copy(source, &target)
            .await
            .with_context(|| format!("Gagal menyalin {}", source.display()))?;
        copied.push(format!("{label}-{name}"));
    }
    Ok(copied)
}

async fn execute(ctx: &Context) -> Result<()> {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let root = PathBuf::from("backups").join(&stamp);
    tokio::fs::create_dir_all(&root)
        .await
        .context("Gagal membuat direktori backup")?;
    let sources = [
        ("users", ctx.app.config.database_path.clone()),
        ("session", ctx.app.config.session_path.clone()),
        ("orkut", PathBuf::from("database/orkut_notif.json")),
        (
            "callback",
            PathBuf::from("database/callback_transaksi.json"),
        ),
    ];
    let copied = copy_sources(&root, &sources).await?;
    anyhow::ensure!(
        !copied.is_empty(),
        "Tidak ada file data yang ditemukan untuk dicadangkan"
    );
    ctx.reply(format!(
        "🗄️ Backup selesai: backups/{stamp}\n{}\n\nCatatan: sesi SQLite sebaiknya dicadangkan saat bot dihentikan.",
        copied.join("\n")
    ))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn copies_only_existing_sources() -> Result<()> {
        let dir = tempfile::tempdir()?;
        tokio::fs::write(dir.path().join("users.json"), "{}").await?;
        let sources = [
            ("users", dir.path().join("users.json")),
            ("missing", dir.path().join("none.json")),
        ];
        let copied = copy_sources(dir.path(), &sources).await?;
        assert_eq!(copied, ["users-users.json"]);
        assert_eq!(
            tokio::fs::read_to_string(dir.path().join("users-users.json")).await?,
            "{}"
        );
        Ok(())
    }
}
