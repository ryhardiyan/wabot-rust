mod downloader;
mod group;
mod info;
mod internet;
mod maker;
mod owner;
mod stalk;
mod tools;
mod user;

use crate::{
    bot_lib::{
        core::{smsg::MessageContext, store::cache::Store},
        user_db::UserDatabase,
    },
    config::Config,
    verify::db::VerifyDb,
};
use anyhow::Result;
use async_trait::async_trait;
use std::{sync::Arc, time::Instant};

pub struct App {
    pub config: Arc<Config>,
    pub http: reqwest::Client,
    /// Klien khusus AI: timeout panjang + header standar agar proxy tidak menolak.
    pub ai_http: reqwest::Client,
    pub db: Arc<UserDatabase>,
    pub verify: Arc<VerifyDb>,
    pub ai: Arc<crate::ai::session::AiSession>,
    pub store: Store,
    pub registry: Registry,
    pub started: Instant,
    pub permits: Arc<tokio::sync::Semaphore>,
    pub rate: crate::bot_lib::core::rate_limit::RateLimiter,
    pub stats: Arc<crate::bot_stats::Stats>,
}
pub struct Context {
    pub app: Arc<App>,
    pub m: MessageContext,
    pub command: String,
    pub text: String,
    pub args: Vec<String>,
    pub is_owner: bool,
    pub is_admin: bool,
    pub is_bot_admin: bool,
}
impl Context {
    pub async fn reply(&self, text: impl Into<String>) -> Result<()> {
        self.m.reply(text).await
    }
    pub fn arg(&self, index: usize) -> &str {
        self.args.get(index).map(String::as_str).unwrap_or("")
    }
    /// Kirim data hasil olahan API sebagai teks yang mudah dibaca, bukan dump JSON mentah.
    pub async fn json(&self, value: &serde_json::Value) -> Result<()> {
        let text = json_text(value);
        if text.len() > 8000 {
            self.m
                .send_media(text.into_bytes(), "text/plain", "hasil.txt", "Hasil")
                .await
        } else {
            self.reply(text).await
        }
    }
    pub async fn media_url(&self, url: &str, mime: &str, caption: &str) -> Result<()> {
        let parsed = reqwest::Url::parse(url)?;
        anyhow::ensure!(
            matches!(parsed.scheme(), "http" | "https"),
            "Protokol media tidak valid"
        );
        let response = self.app.http.get(parsed).send().await?;
        let reported = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.split(';').next())
            .unwrap_or(mime)
            .to_string();
        let actual = if reported.starts_with("image/")
            || reported.starts_with("video/")
            || reported.starts_with("audio/")
        {
            reported.as_str()
        } else {
            mime
        };
        let bytes =
            crate::bot_lib::downloader::bounded(response, self.app.config.max_media_bytes).await?;
        self.m.send_media(bytes, actual, "media", caption).await
    }
}

fn redact(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let key = key.to_ascii_lowercase();
                if [
                    "apikey",
                    "api_key",
                    "access_key",
                    "access_secret",
                    "access_token",
                    "refresh_token",
                    "password",
                    "token",
                    "secret",
                    "authorization",
                ]
                .contains(&key.as_str())
                {
                    *value = serde_json::Value::String("[redacted]".into());
                } else {
                    redact(value);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact(item);
            }
        }
        _ => (),
    }
}

pub(crate) fn json_text(value: &serde_json::Value) -> String {
    let mut value = value.clone();
    redact(&mut value);
    let mut body = String::new();
    render(&value, 0, &mut body);
    format!("*HASIL*\n\n{body}")
}

fn leaf_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "—".into(),
        serde_json::Value::Bool(v) => v.to_string(),
        serde_json::Value::Number(v) => v.to_string(),
        serde_json::Value::String(v) => v.clone(),
        _ => String::new(),
    }
}

fn is_leaf(value: &serde_json::Value) -> bool {
    !value.is_object() && !value.is_array()
}

/// Render JSON sebagai baris ringkas: kunci ditebalkan, array skalar digabung, objek bersarang di-indent.
fn render(value: &serde_json::Value, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    match value {
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                if is_leaf(item) {
                    out.push_str(&format!("{pad}*{key}* : {}\n", leaf_text(item)));
                } else if item
                    .as_array()
                    .is_some_and(|items| items.iter().all(is_leaf))
                {
                    let list = item
                        .as_array()
                        .expect("array sudah diverifikasi")
                        .iter()
                        .map(leaf_text)
                        .collect::<Vec<_>>()
                        .join(", ");
                    let list = if list.is_empty() { "—" } else { &list };
                    out.push_str(&format!("{pad}*{key}* : {list}\n"));
                } else {
                    out.push_str(&format!("{pad}*{key}*:\n"));
                    render(item, indent + 1, out);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                if is_leaf(item) {
                    out.push_str(&format!("{pad}• {}\n", leaf_text(item)));
                } else {
                    out.push_str(&format!("{pad}• [{}]\n", index + 1));
                    render(item, indent + 1, out);
                }
            }
        }
        leaf => out.push_str(&format!("{pad}{}\n", leaf_text(leaf))),
    }
}

#[derive(Clone, Copy)]
pub struct PluginMetadata {
    pub commands: &'static [&'static str],
    pub category: &'static str,
    pub owner_only: bool,
    pub group_only: bool,
    pub admin_only: bool,
    pub bot_admin: bool,
    pub custom_prefix: Option<&'static str>,
}
impl PluginMetadata {
    pub const fn new(commands: &'static [&'static str], category: &'static str) -> Self {
        Self {
            commands,
            category,
            owner_only: false,
            group_only: false,
            admin_only: false,
            bot_admin: false,
            custom_prefix: None,
        }
    }
}
#[async_trait]
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> PluginMetadata;
    async fn execute(&self, ctx: &Context) -> Result<()>;
}
pub type Registry = Vec<Box<dyn Plugin>>;

/// Implementasi per file dengan metadata statis dan dispatch async bertipe.
macro_rules! plugin {
    ($commands:expr, $category:expr, $execute:path $(, $field:ident = $value:expr)* $(,)?) => {
        pub(super) struct Command;
        #[async_trait::async_trait]
        impl crate::plugins::Plugin for Command {
            fn metadata(&self) -> crate::plugins::PluginMetadata {
                let base = crate::plugins::PluginMetadata::new($commands, $category);
                crate::plugins::PluginMetadata { $($field: $value,)* ..base }
            }
            async fn execute(&self, ctx: &crate::plugins::Context) -> anyhow::Result<()> { $execute(ctx).await }
        }
    }
}
pub(crate) use plugin;

pub async fn handle_contact(app: &Arc<App>, m: &MessageContext) -> Result<bool> {
    crate::verify::handle_contact(app, m).await
}

pub fn registry() -> Registry {
    let mut plugins = Registry::new();
    for register in [
        downloader::register as fn(&mut Registry),
        group::register,
        info::register,
        internet::register,
        maker::register,
        owner::register,
        stalk::register,
        tools::register,
        user::register,
    ] {
        register(&mut plugins);
    }
    plugins
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_results_redact_nested_secrets() {
        let mut v = serde_json::json!({"user":{"apiKey":"secret","name":"visible"},"data":[{"token":"hidden"}]});
        super::redact(&mut v);
        assert_eq!(v["user"]["apiKey"], "[redacted]");
        assert_eq!(v["user"]["name"], "visible");
        assert_eq!(v["data"][0]["token"], "[redacted]");
    }
    #[test]
    fn registry_has_all_enabled_plugins_and_unique_commands() {
        let plugins = super::registry();
        assert_eq!(plugins.len(), 50);
        let mut seen = std::collections::HashSet::new();
        for plugin in plugins {
            let meta = plugin.metadata();
            assert!(!meta.commands.is_empty());
            for c in meta.commands {
                assert!(seen.insert(c), "duplicate: {c}");
            }
        }
    }
    #[test]
    fn sensitive_commands_require_owner() {
        let registry = super::registry();
        for name in [
            "stats",
            "exec",
            "eval",
            "file",
            "backup",
            "acc",
            "accuser",
            "unreguser",
            "cf",
            "convert",
            "wink",
            "hdvideo",
            "tourlgh",
            "rvo",
        ] {
            let meta = registry
                .iter()
                .map(|p| p.metadata())
                .find(|m| m.commands.contains(&name))
                .expect("registered fixture command");
            assert!(meta.owner_only, "owner gate missing: {name}");
        }
    }
    #[test]
    fn sqlite_store_is_available() {
        assert!(std::mem::size_of::<whatsapp_rust::store::SqliteStore>() > 0);
    }
}
