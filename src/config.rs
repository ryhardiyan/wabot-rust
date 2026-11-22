use anyhow::{bail, Context, Result};
use std::{collections::HashMap, path::PathBuf};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct Config {
    pub owners: Vec<String>,
    pub pairing_number: String,
    pub pairing_code: Option<String>,
    pub watermark: String,
    pub author: String,
    pub stickpack: String,
    pub stickauth: String,
    #[allow(dead_code)] // Preview kaya dipertahankan sebagai konfigurasi, bukan renderer default.
    pub thumbnail_url: Option<String>,
    pub self_mode: bool,
    pub session_path: PathBuf,
    pub database_path: PathBuf,
    pub max_media_bytes: usize,
    pub max_concurrent: usize,
    pub rate_limit: u32,
    pub webhook_addr: String,
    pub webhook_token: Option<String>,
    pub notification_group: Option<String>,
    pub env: HashMap<String, String>,
}

pub fn clean_number(value: &str) -> String {
    let raw = value
        .split('@')
        .next()
        .unwrap_or(value)
        .split(':')
        .next()
        .unwrap_or(value);
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.len() >= 8 && digits.starts_with('0') {
        format!("62{}", &digits[1..])
    } else {
        digits
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let mut env = std::env::vars().collect::<HashMap<String, String>>();
        match std::fs::read_to_string(".env") {
            Ok(source) => {
                for (key, value) in parse_dotenv(&source)? {
                    env.entry(key).or_insert(value);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error).context("Gagal membaca .env"),
        }
        Self::from_env(env)
    }

    pub fn from_env(env: HashMap<String, String>) -> Result<Self> {
        let get =
            |key: &str, default: &str| env.get(key).cloned().unwrap_or_else(|| default.into());
        let raw = get("OWNERS", "");
        let owners: Vec<String> = if raw.trim().starts_with('[') {
            serde_json::from_str(&raw).context("OWNERS harus array JSON string atau daftar koma")?
        } else {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        };
        let code = get("PAIRING_CODE", "").trim().to_uppercase();
        if !code.is_empty()
            && (code.len() != 8
                || !code
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
        {
            bail!("PAIRING_CODE harus 8 karakter A-Z/0-9");
        }
        let max_concurrent: usize = get("MAX_CONCURRENT_COMMANDS", "4").parse()?;
        let max_media_bytes: usize = get("MAX_MEDIA_MB", "25")
            .parse::<usize>()?
            .checked_mul(1024 * 1024)
            .context("MAX_MEDIA_MB terlalu besar")?;
        if max_concurrent == 0 || max_media_bytes == 0 {
            bail!("Batas konkurensi/media harus positif");
        }
        let rate_limit: u32 = get("RATE_LIMIT_PER_MINUTE", "30").parse()?;
        let session_path = PathBuf::from(get("SESSION_DB", "database/session.db"));
        anyhow::ensure!(
            session_path
                .extension()
                .is_some_and(|s| s == "db" || s == "sqlite" || s == "sqlite3"),
            "SESSION_DB harus menunjuk file SQLite (.db/.sqlite/.sqlite3)"
        );
        let pairing_number = clean_number(&get("PAIRING_NUMBER", ""));
        anyhow::ensure!(
            pairing_number.is_empty() || (8..=15).contains(&pairing_number.len()),
            "PAIRING_NUMBER harus nomor internasional 8–15 digit"
        );
        let webhook_addr = get(
            "WEBHOOK_ADDR",
            &format!("127.0.0.1:{}", get("SERVER_PORT", "6104")),
        );
        webhook_addr
            .parse::<std::net::SocketAddr>()
            .context("WEBHOOK_ADDR harus IP:port")?;
        Ok(Self {
            owners: owners
                .iter()
                .map(|s| clean_number(s))
                .filter(|s| !s.is_empty())
                .collect(),
            pairing_number,
            pairing_code: (!code.is_empty()).then_some(code),
            watermark: get("WATERMARK", "Liora"),
            author: get("AUTHOR", "Naruya Izumi"),
            stickpack: get("STICKPACK", "Liora"),
            stickauth: get("STICKAUTH", "© Naruya Izumi"),
            thumbnail_url: env
                .get("THUMBNAIL_URL")
                .filter(|s| reqwest::Url::parse(s).is_ok_and(|u| u.scheme() == "https"))
                .cloned(),
            self_mode: get("SELF", "false").eq_ignore_ascii_case("true"),
            session_path,
            database_path: PathBuf::from(get("USER_DB", "src/database/users.json")),
            max_media_bytes,
            max_concurrent,
            rate_limit,
            webhook_addr,
            webhook_token: env.get("WEBHOOK_TOKEN").filter(|s| !s.is_empty()).cloned(),
            notification_group: env
                .get("NOTIFICATION_GROUP")
                .filter(|s| !s.is_empty())
                .cloned(),
            env,
        })
    }

    pub fn required(&self, key: &str) -> Result<&str> {
        self.env
            .get(key)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .with_context(|| format!("Lengkapi {key} di .env"))
    }
}

fn parse_dotenv(source: &str) -> Result<HashMap<String, String>> {
    match dotenvy::from_read_iter(source.as_bytes())
        .collect::<std::result::Result<HashMap<_, _>, _>>()
    {
        Ok(values) => Ok(values),
        Err(_) => {
            // Loader JS lama menerima value ber-spasi tanpa quote. Parse ulang
            // tanpa mencetak isi baris/secret dan tanpa menulis ulang .env pengguna.
            let mut values = HashMap::new();
            for (index, line) in source.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let line = line.strip_prefix("export ").unwrap_or(line);
                let (key, value) = line
                    .split_once('=')
                    .with_context(|| format!("Format .env tidak valid pada baris {}", index + 1))?;
                let key = key.trim();
                anyhow::ensure!(
                    !key.is_empty()
                        && key.bytes().enumerate().all(|(i, b)| b == b'_'
                            || b.is_ascii_alphabetic()
                            || (i > 0 && b.is_ascii_digit())),
                    "Nama variabel .env tidak valid pada baris {}",
                    index + 1
                );
                let value = value.trim();
                let value = {
                    value
                        .strip_prefix('"')
                        .and_then(|v| v.strip_suffix('"'))
                        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                        .unwrap_or(value)
                        .to_string()
                };
                values.insert(key.into(), value);
            }
            Ok(values)
        }
    }
}

pub fn init_logging(config: &Config) -> Result<()> {
    let level = config
        .env
        .get("RUST_LOG")
        .or_else(|| config.env.get("LOG_LEVEL"))
        .cloned()
        .unwrap_or_else(|| "info".into());
    let level = match level.as_str() {
        "silent" => "off",
        "fatal" => "error",
        _ => &level,
    };
    let filter = EnvFilter::try_new(level).context("Level log tidak valid")?;
    if config.env.get("LOG_PRETTY").is_some_and(|s| s == "false") {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .try_init()
            .map_err(anyhow::Error::msg)?;
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .try_init()
            .map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_json_owners_before_commas() -> Result<()> {
        let c = Config::from_env(HashMap::from([(
            "OWNERS".into(),
            r#"["08123456789","62899999999"]"#.into(),
        )]))?;
        assert_eq!(c.owners, ["628123456789", "62899999999"]);
        Ok(())
    }
    #[test]
    fn rejects_bad_pairing() {
        assert!(
            Config::from_env(HashMap::from([("PAIRING_CODE".into(), "short".into())])).is_err()
        );
    }
    #[test]
    fn rejects_zero_capacity_and_non_sqlite_path() {
        assert!(Config::from_env(HashMap::from([(
            "MAX_CONCURRENT_COMMANDS".into(),
            "0".into()
        )]))
        .is_err());
        assert!(Config::from_env(HashMap::from([(
            "SESSION_DB".into(),
            "src/auth.json".into()
        )]))
        .is_err());
    }
    #[test]
    fn supports_legacy_unquoted_spaces() -> Result<()> {
        let values = parse_dotenv("WATERMARK=© Example Bot\nSELF=false\nQUOTED=\"Hello World\"\n")?;
        assert_eq!(values["WATERMARK"], "© Example Bot");
        assert_eq!(values["QUOTED"], "Hello World");
        Ok(())
    }
    #[test]
    fn invalid_env_error_does_not_echo_secret() {
        let e = parse_dotenv("GOOD=value with spaces\nINVALID SECRET CONTENT")
            .expect_err("invalid fixture");
        assert!(!e.to_string().contains("SECRET"));
    }
}
