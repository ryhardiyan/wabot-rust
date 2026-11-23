//! Tool lokal yang bisa dipanggil model AI. Semua path dibatasi pada `AI_ROOT`
//! (default: folder proyek tempat bot dijalankan), sehingga model tidak bisa
//! membaca/menghapus file sembarangan di VPS. Set `AI_ALLOW_SECRETS=true` untuk
//! mengizinkan akses ke `.env`, database, dan sesi WhatsApp.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

/// Definisi tool dalam format OpenAI-compatible (`tools` pada request body).
pub fn definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Baca isi file teks di dalam AI_ROOT. Nilai kunci rahasia (KEY/TOKEN/PASSWORD/dsb) otomatis disamarkan.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path relatif terhadap AI_ROOT" },
                        "max_chars": { "type": "integer", "description": "Batas karakter yang dikembalikan (default 20000)" }
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Buat atau timpa file teks di dalam AI_ROOT. Folder induk dibuat otomatis.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path relatif terhadap AI_ROOT" },
                        "content": { "type": "string", "description": "Isi file baru" }
                    },
                    "required": ["path", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "delete_file",
                "description": "Hapus satu file di dalam AI_ROOT (bukan folder).",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_dir",
                "description": "Daftar isi folder di dalam AI_ROOT (maksimal 200 entri).",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string", "description": "Path folder, kosong untuk akar" } },
                    "required": []
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "run_command",
                "description": "Jalankan perintah shell di VPS tempat bot berjalan. Tanpa shell interpreter; gunakan argv.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Nama program, mis. git atau ls" },
                        "args": { "type": "array", "items": { "type": "string" }, "description": "Argumen program" },
                        "timeout_secs": { "type": "integer", "description": "Batas waktu (default 30, maksimal 120)" }
                    },
                    "required": ["command"]
                }
            }
        }
    ])
}

/// Terapkan satu tool. Hasilnya selalu berupa teks yang dikirim balik ke model.
pub async fn execute(root: &Path, allow_secrets: bool, name: &str, raw: &str) -> String {
    match run(root, allow_secrets, name, raw).await {
        Ok(text) => text,
        Err(error) => format!("ERROR: {error:#}"),
    }
}

async fn run(root: &Path, allow_secrets: bool, name: &str, raw: &str) -> Result<String> {
    let args: Value = serde_json::from_str(raw).context("Argumen tool bukan JSON")?;
    match name {
        "read_file" => {
            let path = resolve(root, field(&args, "path")?, allow_secrets).await?;
            anyhow::ensure!(path.is_file(), "Bukan file: {}", path.display());
            let bytes = tokio::fs::read(&path).await?;
            anyhow::ensure!(
                bytes.len() <= 4 * 1024 * 1024,
                "File terlalu besar untuk dibaca"
            );
            let text = String::from_utf8_lossy(&bytes);
            let limit = args
                .get("max_chars")
                .and_then(Value::as_u64)
                .unwrap_or(20_000)
                .clamp(1, 50_000) as usize;
            Ok(truncate(&redact_secrets(&text), limit))
        }
        "write_file" => {
            let path = resolve(root, field(&args, "path")?, allow_secrets).await?;
            let content = field(&args, "content")?;
            anyhow::ensure!(content.len() <= 2 * 1024 * 1024, "Isi terlalu besar");
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&path, content).await?;
            Ok(format!(
                "OK: {} ditulis ({} karakter)",
                path.display(),
                content.len()
            ))
        }
        "delete_file" => {
            let path = resolve(root, field(&args, "path")?, allow_secrets).await?;
            anyhow::ensure!(path.is_file(), "Bukan file: {}", path.display());
            tokio::fs::remove_file(&path).await?;
            Ok(format!("OK: {} dihapus", path.display()))
        }
        "list_dir" => {
            let requested = field(&args, "path").ok().unwrap_or("");
            let path = if requested.trim().is_empty() {
                root.to_path_buf()
            } else {
                resolve(root, requested, allow_secrets).await?
            };
            let mut dir = tokio::fs::read_dir(&path).await?;
            let mut names = Vec::new();
            while let Some(entry) = dir.next_entry().await? {
                let kind = if entry.path().is_dir() { "/" } else { "" };
                names.push(format!("{}{}", entry.file_name().to_string_lossy(), kind));
                if names.len() == 200 {
                    break;
                }
            }
            names.sort();
            Ok(if names.is_empty() {
                format!("Folder {} kosong", path.display())
            } else {
                names.join("\n")
            })
        }
        "run_command" => {
            let command = field(&args, "command")?;
            let argv: Vec<String> = args
                .get("args")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            let secs = args
                .get("timeout_secs")
                .and_then(Value::as_u64)
                .unwrap_or(30)
                .clamp(1, 120);
            let child = tokio::process::Command::new(&command)
                .args(&argv)
                .current_dir(root)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .with_context(|| format!("Gagal menjalankan {command}"))?;
            let output = tokio::time::timeout(
                std::time::Duration::from_secs(secs),
                child.wait_with_output(),
            )
            .await
            .with_context(|| format!("{command} timeout setelah {secs} detik"))??;
            let mut text = String::new();
            text.push_str(&String::from_utf8_lossy(&output.stdout));
            if !output.stderr.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str("[stderr] ");
                text.push_str(&String::from_utf8_lossy(&output.stderr));
            }
            Ok(format!(
                "exit={}\n{}",
                output.status.code().unwrap_or(-1),
                truncate(&redact_secrets(&text), 20_000)
            ))
        }
        other => bail!("Tool tidak dikenal: {other}"),
    }
}

fn field<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("Argumen `{key}` wajib berupa teks"))
}

/// Path relatif terhadap root. Menolak `..` yang keluar dari root dan file
/// sensitif kecuali `allow_secrets` aktif.
async fn resolve(root: &Path, value: &str, allow_secrets: bool) -> Result<PathBuf> {
    let value = value.trim();
    anyhow::ensure!(!value.is_empty(), "Path kosong");
    let canonical_root = tokio::fs::canonicalize(root).await?;
    let relative = Path::new(value);
    let target = if relative.is_absolute() {
        relative.to_path_buf()
    } else {
        canonical_root.join(relative)
    };
    let existing = if target.exists() {
        target.clone()
    } else {
        target
            .parent()
            .filter(|p| p.exists())
            .unwrap_or(root)
            .to_path_buf()
    };
    let canonical = tokio::fs::canonicalize(&existing).await?;
    anyhow::ensure!(
        canonical == canonical_root || canonical.starts_with(&canonical_root),
        "Path keluar dari AI_ROOT"
    );
    let final_path = if target.exists() {
        canonical
    } else {
        normalize(&target)
    };
    if final_path != canonical_root && !final_path.starts_with(&canonical_root) {
        bail!("Path keluar dari AI_ROOT");
    }
    if !allow_secrets && is_sensitive(&final_path, &canonical_root) {
        bail!("File sensitif dilindungi; aktifkan AI_ALLOW_SECRETS=true bila diperlukan");
    }
    Ok(final_path)
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => (),
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn is_sensitive(path: &Path, root: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return true;
    };
    let text = rel.to_string_lossy().to_ascii_lowercase();
    text == ".env"
        || text.starts_with(".env.")
        || text.starts_with(".git/")
        || text.contains("session")
        || text.ends_with(".db")
        || text.ends_with(".sqlite")
        || text.ends_with(".sqlite3")
}

fn redact_secrets(text: &str) -> String {
    text.lines()
        .map(|line| {
            let (key, _) = line
                .split_once('=')
                .or_else(|| line.split_once(':'))
                .unwrap_or((line, ""));
            let key = key.trim().to_ascii_uppercase();
            if ["KEY", "TOKEN", "SECRET", "PASSWORD", "CREDENTIAL", "AUTH"]
                .iter()
                .any(|s| key.contains(s))
            {
                format!("{key}=[redacted]")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let head: String = text.chars().take(limit).collect();
    format!("{head}\n…[dipotong]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_menolak_path_keluar_dari_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        assert!(resolve(root, "../luar.txt", false).await.is_err());
        assert!(resolve(root, "/etc/passwd", false).await.is_err());
    }

    #[tokio::test]
    async fn resolve_menerima_file_di_dalam_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("catatan.txt"), "halo")
            .await
            .expect("tulis file");
        let path = resolve(dir.path(), "catatan.txt", false)
            .await
            .expect("path valid");
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("catatan.txt")
        );
    }

    #[tokio::test]
    async fn file_sensitif_dilindungi() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(resolve(dir.path(), ".env", false).await.is_err());
        assert!(resolve(dir.path(), "database/ai.db", true).await.is_ok());
    }

    #[test]
    fn redaksi_menutup_nilai_rahasia() {
        let text = "AI_API_KEY=rahasia\nAI_MODEL=rmod/hy4-flash-free\n";
        let redacted = redact_secrets(text);
        assert!(redacted.contains("[redacted]"));
        assert!(!redacted.contains("rahasia"));
        assert!(redacted.contains("rmod/hy4-flash-free"));
    }
}
