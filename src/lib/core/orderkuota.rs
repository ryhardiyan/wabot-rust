use super::socket::WaSocket;
use crate::{database::JsonDatabase, plugins::App};
use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};

struct Webhooks {
    app: Arc<App>,
    socket: WaSocket,
    orkut: JsonDatabase<Vec<Value>>,
    callbacks: JsonDatabase<Vec<Value>>,
}
pub async fn serve(app: Arc<App>, socket: WaSocket) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(&app.config.webhook_addr).await?;
    let state = Arc::new(Webhooks {
        app,
        socket,
        orkut: JsonDatabase::open("database/orkut_notif.json").await?,
        callbacks: JsonDatabase::open("database/callback_transaksi.json").await?,
    });
    tracing::info!(address = %listener.local_addr()?, "Webhook aktif dengan autentikasi Bearer");
    let permits = Arc::new(tokio::sync::Semaphore::new(4));
    let token = state.app.config.webhook_token.clone();
    let protected = Router::new()
        .route("/orkut", get(orkut))
        .route("/callback", get(callback))
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let permits = permits.clone();
                let token = token.clone();
                async move {
                    use axum::response::IntoResponse;
                    if !authorized(token.as_deref(), request.headers()) {
                        return StatusCode::UNAUTHORIZED.into_response();
                    }
                    let Ok(_permit) = permits.try_acquire_owned() else {
                        return StatusCode::TOO_MANY_REQUESTS.into_response();
                    };
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(20),
                        next.run(request),
                    )
                    .await
                    {
                        Ok(response) => response,
                        Err(_) => StatusCode::GATEWAY_TIMEOUT.into_response(),
                    }
                }
            },
        ));
    let router = Router::new()
        .route("/health", get(health))
        .merge(protected)
        .with_state(state.clone());
    axum::serve(listener, router).await?;
    Ok(())
}
async fn health(State(state): State<Arc<Webhooks>>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": state.app.started.elapsed().as_secs(),
        "plugins": state.app.registry.len(),
    }))
}
fn authorized(token: Option<&str>, headers: &HeaderMap) -> bool {
    let value = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    value.is_some_and(|v| token == Some(v))
}
async fn process(
    state: Arc<Webhooks>,
    headers: HeaderMap,
    query: HashMap<String, String>,
    is_orkut: bool,
) -> Result<Json<Value>, StatusCode> {
    if !authorized(state.app.config.webhook_token.as_deref(), &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if query.values().any(|s| s.len() > 8192) {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let db = if is_orkut {
        &state.orkut
    } else {
        &state.callbacks
    };
    let has_payload = if is_orkut {
        query.get("data").is_some_and(|s| !s.is_empty())
    } else {
        query.get("refid").is_some_and(|s| !s.is_empty())
            || query.get("message").is_some_and(|s| !s.is_empty())
    };
    if !has_payload {
        return Ok(Json(json!(db.snapshot().await)));
    }
    let timestamp = chrono::Utc::now().to_rfc3339();
    let entry = if is_orkut {
        json!({"timestamp":timestamp,"notification":query.get("data").cloned().unwrap_or_default()})
    } else {
        json!({"timestamp":timestamp,"refid":query.get("refid").cloned().unwrap_or_default(),"message":query.get("message").cloned().unwrap_or_default()})
    };
    let saved = entry.clone();
    db.update(|history| {
        history.insert(0, saved);
        history.truncate(50);
        Ok(())
    })
    .await
    .map_err(|e| {
        tracing::error!(error=%e, "Webhook persistensi gagal");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if let Some(group) = &state.app.config.notification_group {
        // Notifikasi menyebut sumber callback; tidak memalsukan konfirmasi pembayaran server.
        if let Err(error) = state
            .socket
            .send_text(
                group,
                format!(
                    "📩 Notifikasi {}\n{}",
                    if is_orkut { "Order Kuota" } else { "transaksi" },
                    entry
                ),
            )
            .await
        {
            tracing::warn!(%error,"Notifikasi WA tertunda; data tersimpan");
        }
    }
    Ok(Json(json!({"success":true,"data":entry})))
}

async fn orkut(
    State(s): State<Arc<Webhooks>>,
    h: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    process(s, h, q, true).await
}
async fn callback(
    State(s): State<Arc<Webhooks>>,
    h: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    process(s, h, q, false).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn webhook_requires_exact_bearer() {
        let mut h = HeaderMap::new();
        assert!(!authorized(Some("secret"), &h));
        h.insert(
            "authorization",
            "Bearer wrong".parse().expect("header fixture"),
        );
        assert!(!authorized(Some("secret"), &h));
        h.insert(
            "authorization",
            "Bearer secret".parse().expect("header fixture"),
        );
        assert!(authorized(Some("secret"), &h));
        assert!(!authorized(None, &h));
    }
    #[tokio::test]
    async fn callback_persists_and_limits_history_offline() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let config = Arc::new(crate::config::Config::from_env(HashMap::from([(
            "WEBHOOK_TOKEN".into(),
            "fixture-token".into(),
        )]))?);
        let store = super::super::auth::session::open(&dir.path().join("session.db")).await?;
        let bot = whatsapp_rust::prelude::Bot::builder()
            .with_backend(store)
            .build()
            .await?;
        let app = Arc::new(App {
            config,
            http: crate::bot_lib::downloader::client()?,
            db: Arc::new(
                crate::bot_lib::user_db::UserDatabase::open(dir.path().join("users.json")).await?,
            ),
            store: Default::default(),
            registry: crate::plugins::registry(),
            started: std::time::Instant::now(),
            permits: Arc::new(tokio::sync::Semaphore::new(2)),
            rate: crate::bot_lib::core::rate_limit::RateLimiter::new(0),
        });
        let state = Arc::new(Webhooks {
            app,
            socket: WaSocket {
                client: bot.client(),
            },
            orkut: JsonDatabase::open(dir.path().join("orkut.json")).await?,
            callbacks: JsonDatabase::open(dir.path().join("callback.json")).await?,
        });
        let status = health(State(state.clone())).await;
        assert_eq!(status.0["status"], "ok");
        let denied = process(
            state.clone(),
            HeaderMap::new(),
            HashMap::from([("data".into(), "test".into())]),
            true,
        )
        .await;
        assert_eq!(denied.err(), Some(StatusCode::UNAUTHORIZED));
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer fixture-token".parse()?);
        for n in 0..55 {
            let _response = process(
                state.clone(),
                headers.clone(),
                HashMap::from([
                    ("refid".into(), n.to_string()),
                    ("message".into(), "fixture only".into()),
                ]),
                false,
            )
            .await
            .map_err(|s| anyhow::anyhow!("{s}"))?;
        }
        let history = state.callbacks.snapshot().await;
        assert_eq!(history.len(), 50);
        assert_eq!(history[0]["refid"], "54");
        assert!(state.orkut.snapshot().await.is_empty());
        Ok(())
    }
}
