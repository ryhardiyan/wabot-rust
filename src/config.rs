use anyhow::{bail, Context, Result};
use std::{collections::HashMap, path::PathBuf};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct Config {
    pub owners: Vec<String>,
    /// Nomor/LID owner dari `OWNER_NUMBER` (bisa lebih dari satu akun).
    pub owner_numbers: Vec<String>,
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
    /// JID owner yang berhak menyetujui verifikasi (OWNER_JID).
    pub owner_jid: String,
    /// Nomor owner yang harus tertulis di vCard (OWNER_PHONE).
    pub owner_phone: String,
    /// Kata wajib pada field FN vCard (REQUIRED_NAME).
    pub required_name: String,
    /// File SQLite untuk tabel users verifikasi (VERIFY_DB).
    pub verify_db_path: PathBuf,
    /// File SQLite untuk statistik pemakaian plugin (PLUGIN_DB).
    pub plugin_db_path: PathBuf,
    /// Endpoint REST OpenAI-compatible untuk fitur AI owner-only (AI_API_URL).
    pub ai_api_url: String,
    /// Kunci API toolwiz (AI_API_KEY).
    pub ai_api_key: String,
    /// Model yang dipakai (AI_MODEL).
    pub ai_model: String,
    /// File SQLite riwayat chat AI (AI_DB).
    pub ai_db_path: PathBuf,
    /// Akar path yang boleh diakses tool AI (AI_ROOT).
    pub ai_root: PathBuf,
    /// Maksimal putaran pemanggilan tool per permintaan (AI_MAX_TURNS).
    pub ai_max_turns: usize,
    /// Jumlah giliran user yang disimpan dalam riwayat (AI_HISTORY_MESSAGES).
    pub ai_history_messages: usize,
    /// Izinkan tool AI mengakses .env/database/sesi (AI_ALLOW_SECRETS).
    pub ai_allow_secrets: bool,
    /// Timeout total satu request AI dalam detik (AI_TIMEOUT_SECS).
    pub ai_timeout_secs: u64,
    /// Maksimal percobaan request AI, termasuk percobaan pertama (AI_MAX_ATTEMPTS).
    pub ai_max_attempts: u32,
    /// Jeda awal backoff retry AI dalam milidetik (AI_RETRY_BASE_MS).
    pub ai_retry_base_ms: u64,
    /// Batas total satu permintaan AI termasuk retry (AI_TOTAL_TIMEOUT_SECS).
    pub ai_total_timeout_secs: u64,
    pub max_media_bytes: usize,
    pub max_concurrent: usize,
    pub rate_limit: u32,
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
    if digits.len() >= 8 {
        // `08...` -> `628...`, dan `6208...` (prefix 62 dengan nol) -> `628...`.
        // Selaras dengan `verify::vcard::normalize_phone` agar nomor owner yang
        // cocok di vCard tidak meleset hanya karena beda format awalan.
        if let Some(rest) = digits.strip_prefix('0') {
            return format!("62{rest}");
        }
        if let Some(rest) = digits.strip_prefix("620") {
            return format!("62{rest}");
        }
    }
    digits
}

impl Config {
    /// Nama brand tanpa simbol © bawaan `.env`, siap dipakai di header/caption.
    pub fn brand(&self) -> &str {
        self.watermark.trim().trim_start_matches('©').trim()
    }

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
        let owner_numbers: Vec<String> = get("OWNER_NUMBER", "")
            .split(',')
            .map(str::trim)
            .map(clean_number)
            .filter(|s| !s.is_empty())
            .collect();
        let fallback_owner = owner_numbers.first().cloned().unwrap_or_default();
        let owner_phone = clean_number(&get("OWNER_PHONE", &fallback_owner));
        anyhow::ensure!(
            owner_phone.is_empty() || (8..=15).contains(&owner_phone.len()),
            "OWNER_PHONE harus nomor internasional 8–15 digit"
        );
        let raw_owner_jid = get("OWNER_JID", "");
        let owner_jid = if raw_owner_jid.is_empty() {
            if owner_phone.is_empty() {
                String::new()
            } else {
                format!("{owner_phone}@s.whatsapp.net")
            }
        } else if raw_owner_jid.contains('@') {
            raw_owner_jid
        } else {
            format!("{}@s.whatsapp.net", clean_number(&raw_owner_jid))
        };
        let required_name = get("REQUIRED_NAME", "Rahardiyan");
        let verify_db_path = PathBuf::from(get("VERIFY_DB", "database/users.db"));
        let plugin_db_path = PathBuf::from(get("PLUGIN_DB", "database/plugins.db"));
        let ai_api_url = get("AI_API_URL", "https://ai.toolwiz.id/v1");
        if !ai_api_url.trim().is_empty() {
            let parsed = reqwest::Url::parse(&ai_api_url).context("AI_API_URL tidak valid")?;
            anyhow::ensure!(
                matches!(parsed.scheme(), "http" | "https"),
                "AI_API_URL harus http/https"
            );
        }
        let ai_api_key = get("AI_API_KEY", "");
        let ai_model = get("AI_MODEL", "rmod/hy4-flash-free");
        let ai_db_path = PathBuf::from(get("AI_DB", "database/ai.db"));
        let ai_root = PathBuf::from(get("AI_ROOT", "."));
        let ai_max_turns: usize = get("AI_MAX_TURNS", "8").parse()?;
        let ai_history_messages: usize = get("AI_HISTORY_MESSAGES", "40").parse()?;
        anyhow::ensure!(
            ai_max_turns > 0 && ai_history_messages > 0,
            "AI_MAX_TURNS dan AI_HISTORY_MESSAGES harus positif"
        );
        let ai_allow_secrets = get("AI_ALLOW_SECRETS", "false").eq_ignore_ascii_case("true");
        let ai_timeout_secs: u64 = get("AI_TIMEOUT_SECS", "180").parse()?;
        let ai_max_attempts: u32 = get("AI_MAX_ATTEMPTS", "4").parse()?;
        let ai_retry_base_ms: u64 = get("AI_RETRY_BASE_MS", "2000").parse()?;
        // Batas total ±5 menit, masih di bawah timeout plugin/handler (660–720 dtk).
        let ai_total_timeout_secs: u64 = get("AI_TOTAL_TIMEOUT_SECS", "300").parse()?;
        anyhow::ensure!(
            (30..=600).contains(&ai_timeout_secs),
            "AI_TIMEOUT_SECS harus 30–600 detik"
        );
        anyhow::ensure!(
            (30..=600).contains(&ai_total_timeout_secs),
            "AI_TOTAL_TIMEOUT_SECS harus 30–600 detik"
        );
        anyhow::ensure!(
            (1..=10).contains(&ai_max_attempts),
            "AI_MAX_ATTEMPTS harus 1–10"
        );
        anyhow::ensure!(
            (100..=30_000).contains(&ai_retry_base_ms),
            "AI_RETRY_BASE_MS harus 100–30000 milidetik"
        );
        Ok(Self {
            owners: owners
                .iter()
                .map(|s| clean_number(s))
                .filter(|s| !s.is_empty())
                .collect(),
            owner_numbers,
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
            database_path: PathBuf::from(get("USER_DB", "database/users.json")),
            owner_jid,
            owner_phone,
            required_name,
            verify_db_path,
            plugin_db_path,
            ai_api_url,
            ai_api_key,
            ai_model,
            ai_db_path,
            ai_root,
            ai_max_turns,
            ai_history_messages,
            ai_allow_secrets,
            ai_timeout_secs,
            ai_max_attempts,
            ai_retry_base_ms,
            ai_total_timeout_secs,
            max_media_bytes,
            max_concurrent,
            rate_limit,
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

    #[test]
    fn ai_retry_settings_have_safe_defaults_and_limits() -> Result<()> {
        let c = Config::from_env(HashMap::new())?;
        assert_eq!(c.ai_timeout_secs, 180);
        assert_eq!(c.ai_max_attempts, 4);
        assert_eq!(c.ai_retry_base_ms, 2_000);
        assert_eq!(c.ai_total_timeout_secs, 300);
        assert!(Config::from_env(HashMap::from([("AI_MAX_ATTEMPTS".into(), "0".into())])).is_err());
        assert!(Config::from_env(HashMap::from([(
            "AI_TOTAL_TIMEOUT_SECS".into(),
            "10".into()
        )]))
        .is_err());
        assert!(Config::from_env(HashMap::from([("AI_TIMEOUT_SECS".into(), "5".into())])).is_err());
        Ok(())
    }

    #[test]
    fn clean_number_matches_vcard_normalization() {
        assert_eq!(clean_number("0812-3456 789"), "628123456789");
        assert_eq!(
            clean_number("6282323780821@s.whatsapp.net"),
            "6282323780821"
        );
        assert_eq!(clean_number("6208123456789"), "628123456789");
    }
}
