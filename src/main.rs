#![recursion_limit = "512"]
#[path = "lib/mod.rs"]
mod bot_lib;
mod config;
mod database;
mod handler;
mod plugins;

use anyhow::Result;
use std::{sync::Arc, time::Instant};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Arc::new(config::Config::load()?);
    config::init_logging(&config)?;
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
    let app = Arc::new(plugins::App {
        permits: Arc::new(tokio::sync::Semaphore::new(config.max_concurrent)),
        config,
        http: bot_lib::downloader::client()?,
        db,
        store: Default::default(),
        registry,
        rate: bot_lib::core::rate_limit::RateLimiter::new(rate_limit),
        started: Instant::now(),
    });
    bot_lib::core::connection::run(app).await
}
