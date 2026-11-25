#![recursion_limit = "512"]
mod ai;
#[path = "lib/mod.rs"]
mod bot_lib;
mod bot_stats;
mod config;
mod handler;
mod plugins;
mod stats_db;
mod verify;

use anyhow::Result;
use std::{sync::Arc, time::Instant};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Arc::new(config::Config::load()?);
    config::init_logging(&config)?;
    bot_lib::core::logger::init();
    let rate_limit = config.rate_limit;
    let registry = plugins::registry();
    if std::env::args().any(|a| a == "--check") {
        let mut commands = std::collections::BTreeSet::new();
        for plugin in &registry {
            let m = plugin.metadata();
            for c in m.commands {
                anyhow::ensure!(commands.insert(*c), "Command ganda: {c}");
            }
        }
        println!(
            "CONFIG_OK plugins={} commands={} edition=2021",
            registry.len(),
            commands.len()
        );
        return Ok(());
    }
    let db = Arc::new(bot_lib::user_db::UserDatabase::open(&config.database_path).await?);
    let verify = Arc::new(verify::db::VerifyDb::open(&config.verify_db_path).await?);
    if !config.owner_jid.is_empty() {
        verify.set_verified(&config.owner_jid).await?;
    }
    let stats = Arc::new(bot_stats::Stats::open(&config.plugin_db_path).await?);
    let ai = Arc::new(ai::session::AiSession::open(&config.ai_db_path).await?);
    let ai_http = crate::ai::client::http_client(config.ai_timeout_secs)?;
    let app = Arc::new(plugins::App {
        permits: Arc::new(tokio::sync::Semaphore::new(config.max_concurrent)),
        config,
        http: bot_lib::downloader::client()?,
        ai_http,
        db,
        verify,
        ai,
        store: Default::default(),
        registry,
        rate: bot_lib::core::rate_limit::RateLimiter::new(rate_limit),
        stats,
        started: Instant::now(),
    });
    bot_lib::core::connection::run(app).await
}
