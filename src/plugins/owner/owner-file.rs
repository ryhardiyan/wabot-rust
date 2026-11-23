use crate::plugins::Context;
use anyhow::{Context as _, Result};
use std::path::{Path, PathBuf};
crate::plugins::plugin!(&["file", "f"], "owner", execute, owner_only = true);
async fn resolve(parts: &[String]) -> Result<PathBuf> {
    let root = tokio::fs::canonicalize(".").await?;
    let relative = parts.iter().collect::<PathBuf>();
    anyhow::ensure!(
        !relative.is_absolute()
            && !relative
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
        "Path harus relatif dalam proyek"
    );
    let target = root.join(relative);
    let existing = if target.exists() {
        target.as_path()
    } else {
        target.parent().unwrap_or(Path::new("."))
    };
    let canonical = tokio::fs::canonicalize(existing).await?;
    anyhow::ensure!(canonical.starts_with(&root), "Path keluar dari proyek");
    if target.exists() {
        Ok(canonical)
    } else {
        Ok(target)
    }
}
async fn execute(ctx: &Context) -> Result<()> {
    if ctx.arg(0) == "r" {
        return ctx.reply("Plugin Rust terdaftar saat kompilasi. Build ulang lalu restart untuk memuat perubahan.").await;
    }
    if ctx.args.len() < 2 {
        return ctx
            .reply("/file g|s|d <path relatif> — s tanpa quoted media menampilkan isi folder")
            .await;
    }
    let path = resolve(&ctx.args[1..]).await?;
    match ctx.arg(0) {
        "g" => {
            let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
            anyhow::ensure!(
                path.file_name()
                    .is_none_or(|n| !n.to_string_lossy().starts_with(".env"))
                    && !matches!(ext, "db" | "sqlite" | "sqlite3")
                    && !path.to_string_lossy().contains("session"),
                "File kredensial/sesi tidak dikirim melalui command file"
            );
            let meta = tokio::fs::metadata(&path).await?;
            anyhow::ensure!(
                meta.len() <= ctx.app.config.max_media_bytes as u64,
                "File terlalu besar"
            );
            let bytes = tokio::fs::read(&path).await?;
            ctx.m
                .send_media(
                    bytes,
                    "application/octet-stream",
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .context("Nama file kosong")?,
                    "",
                )
                .await
        }
        "d" => {
            tokio::fs::remove_file(path).await?;
            ctx.reply("File dihapus").await
        }
        "s" => {
            if ctx.m.quoted.is_some() {
                let bytes = ctx.m.download(ctx.app.config.max_media_bytes).await?;
                anyhow::ensure!(!path.is_dir(), "Tentukan path lengkap file tujuan");
                tokio::fs::write(path, bytes).await?;
                ctx.reply("File tersimpan").await
            } else {
                let mut items = tokio::fs::read_dir(path).await?;
                let mut names = Vec::new();
                while let Some(item) = items.next_entry().await? {
                    names.push(item.file_name().to_string_lossy().into_owned());
                    if names.len() >= 200 {
                        break;
                    }
                }
                names.sort();
                ctx.reply(names.join("\n")).await
            }
        }
        _ => ctx.reply("/file g|s|d|r").await,
    }
}
