//! Klien REST OpenAI-compatible untuk endpoint toolwiz.
//!
//! Permintaan dilindungi retry untuk status 408/425/429/5xx dan error jaringan
//! sementara. Pengirim pesan status (plugin `.ai`) bisa memantau progres lewat
//! `ProgressReporter` tanpa mencampur logika HTTP dengan logika WhatsApp.

use super::tools;
use anyhow::{bail, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONNECTION};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};

/// Konfigurasi koneksi yang diambil dari `.env`.
#[derive(Clone)]
pub struct Settings {
    pub url: String,
    pub key: String,
    pub model: String,
    pub root: PathBuf,
    pub max_turns: usize,
    pub history_messages: usize,
    pub allow_secrets: bool,
    /// Timeout total per percobaan request (detik).
    pub timeout_secs: u64,
    /// Jumlah maksimal percobaan, termasuk percobaan pertama.
    pub max_attempts: u32,
    /// Jeda backoff awal (milidetik); tiap retry digandakan hingga batas 15 detik.
    pub retry_base_ms: u64,
    /// Batas total seluruh permintaan termasuk retry dan putaran tool (detik).
    pub total_timeout_secs: u64,
}

/// Klien HTTP khusus AI: timeout panjang, keep-alive, dan header yang lazim
/// dipakai client supaya tidak ditolak reverse proxy di depan server AI.
pub fn http_client(timeout_secs: u64) -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
    Ok(reqwest::Client::builder()
        .user_agent("Wabot-Rust/11.0 (AI Agent)")
        .default_headers(headers)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(timeout_secs.max(1)))
        .tcp_keepalive(Duration::from_secs(30))
        .pool_max_idle_per_host(4)
        .build()?)
}

/// Status yang dianggap sementara dan layak dicoba lagi.
pub fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

/// Backoff eksponensial: `base`, `2 * base`, `4 * base`, ... (maks. 15 detik).
pub fn retry_delay(settings: &Settings, failed_attempt: u32) -> Duration {
    let factor = 1u64
        .checked_shl(failed_attempt.saturating_sub(1))
        .unwrap_or(u64::MAX);
    let millis = settings.retry_base_ms.saturating_mul(factor).min(15_000);
    Duration::from_millis(millis)
}

/// Progres yang dilaporkan ke plugin pengirim pesan status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Server sibuk / koneksi putus; `attempt` adalah percobaan berikutnya.
    Retrying {
        attempt: u32,
        max_attempts: u32,
        /// `None` berarti kegagalan jaringan, bukan status HTTP.
        status: Option<u16>,
    },
    /// Respons HTTP diterima; body sedang menunggu diisi server.
    WaitingResponse,
}

#[async_trait::async_trait]
pub trait ProgressReporter: Send + Sync {
    async fn report(&self, progress: Progress);
}

async fn notify(reporter: Option<&dyn ProgressReporter>, progress: Progress) {
    if let Some(reporter) = reporter {
        reporter.report(progress).await;
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: Some(text.into()),
            ..Default::default()
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: Some(text.into()),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

/// Kirim satu request dengan retry otomatis. Mengembalikan status akhir dan body.
/// Error jaringan yang terus gagal diubah menjadi `anyhow::Error` di sini,
/// sehingga pemanggil hanya perlu menangani satu jenis kegagalan.
async fn request_with_retry(
    settings: &Settings,
    http: &reqwest::Client,
    endpoint: &str,
    body: &Value,
    reporter: Option<&dyn ProgressReporter>,
) -> Result<(reqwest::StatusCode, String)> {
    for attempt in 1..=settings.max_attempts.max(1) {
        let response = match http
            .post(endpoint)
            .bearer_auth(&settings.key)
            .json(body)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                let retryable = error.is_timeout() || error.is_connect() || error.is_request();
                let message = format!("Gagal menghubungi AI API: {error}");
                if retryable && attempt < settings.max_attempts {
                    notify(
                        reporter,
                        Progress::Retrying {
                            attempt: attempt + 1,
                            max_attempts: settings.max_attempts,
                            status: None,
                        },
                    )
                    .await;
                    tokio::time::sleep(retry_delay(settings, attempt)).await;
                    continue;
                }
                bail!(message);
            }
        };

        let status = response.status();
        if is_retryable_status(status.as_u16()) && attempt < settings.max_attempts {
            notify(
                reporter,
                Progress::Retrying {
                    attempt: attempt + 1,
                    max_attempts: settings.max_attempts,
                    status: Some(status.as_u16()),
                },
            )
            .await;
            tokio::time::sleep(retry_delay(settings, attempt)).await;
            continue;
        }

        if status.is_success() {
            notify(reporter, Progress::WaitingResponse).await;
        }
        let raw = match response.text().await {
            Ok(raw) => raw,
            Err(error) => {
                let retryable = error.is_timeout()
                    || error.is_connect()
                    || error.is_request()
                    || error.is_body()
                    || error.is_decode();
                let message = format!("Gagal membaca respons AI API: {error}");
                if retryable && attempt < settings.max_attempts {
                    notify(
                        reporter,
                        Progress::Retrying {
                            attempt: attempt + 1,
                            max_attempts: settings.max_attempts,
                            status: Some(status.as_u16()),
                        },
                    )
                    .await;
                    tokio::time::sleep(retry_delay(settings, attempt)).await;
                    continue;
                }
                bail!(message);
            }
        };
        return Ok((status, raw));
    }
    unreachable!("minimal satu percobaan dijamin oleh max_attempts")
}

/// Kirim percakapan ke API, jalankan tool bila diminta, ulangi sampai jawaban akhir.
/// Riwayat yang dikembalikan sudah termasuk pesan tool, sehingga sesi berikutnya
/// tetap punya konteks penuh.
pub async fn complete(
    settings: &Settings,
    http: &reqwest::Client,
    messages: Vec<Message>,
    reporter: Option<Arc<dyn ProgressReporter>>,
) -> Result<(String, Vec<Message>)> {
    // Batas total menghindari retry 4x180 detik melewati timeout plugin (660 dtk).
    let deadline = Duration::from_secs(settings.total_timeout_secs.max(1));
    tokio::time::timeout(deadline, complete_inner(settings, http, messages, reporter))
        .await
        .with_context(|| format!("AI melebihi batas waktu total {} detik", deadline.as_secs()))?
}

async fn complete_inner(
    settings: &Settings,
    http: &reqwest::Client,
    mut messages: Vec<Message>,
    reporter: Option<Arc<dyn ProgressReporter>>,
) -> Result<(String, Vec<Message>)> {
    messages = super::session::trim_history(messages, settings.history_messages);
    let endpoint = format!("{}/chat/completions", settings.url.trim_end_matches('/'));
    let tool_defs = tools::definitions();
    let reporter = reporter.as_deref();

    for _ in 0..settings.max_turns {
        let body = json!({
            "model": settings.model.clone(),
            "messages": messages.clone(),
            "tools": tool_defs.clone(),
            "tool_choice": "auto",
        });
        let (status, raw) = request_with_retry(settings, http, &endpoint, &body, reporter).await?;
        if !status.is_success() {
            let preview: String = raw.chars().take(500).collect();
            bail!("AI API mengembalikan {status}: {preview}");
        }
        let parsed: ApiResponse =
            serde_json::from_str(&raw).context("Respons AI API bukan JSON yang dikenal")?;
        let assistant = parsed
            .choices
            .into_iter()
            .next()
            .context("AI API tidak mengembalikan pilihan jawaban")?
            .message;
        let calls = assistant.tool_calls.clone().unwrap_or_default();
        messages.push(assistant.clone());

        if calls.is_empty() {
            return Ok((assistant.content.unwrap_or_default(), messages));
        }
        for call in calls {
            let result = tools::execute(
                &settings.root,
                settings.allow_secrets,
                &call.function.name,
                &call.function.arguments,
            )
            .await;
            messages.push(Message {
                role: "tool".into(),
                content: Some(result),
                tool_call_id: Some(call.id),
                ..Default::default()
            });
        }
    }
    bail!(
        "AI melebihi {} putaran pemanggilan tool",
        settings.max_turns
    )
}

#[allow(dead_code)] // Pembantu debugging respons mentah saat mengintegrasikan provider baru.
pub fn preview_value(value: &Value) -> String {
    value.to_string().chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn settings(url: &str) -> Settings {
        Settings {
            url: url.into(),
            key: "test-key".into(),
            model: "test-model".into(),
            root: PathBuf::from("."),
            max_turns: 2,
            history_messages: 10,
            allow_secrets: false,
            timeout_secs: 5,
            max_attempts: 4,
            retry_base_ms: 10,
            total_timeout_secs: 30,
        }
    }

    #[test]
    fn only_transient_statuses_are_retried() {
        for status in [408, 425, 429, 500, 502, 503, 504] {
            assert!(is_retryable_status(status), "{status} harus diretry");
        }
        for status in [200, 400, 401, 403, 404, 418] {
            assert!(!is_retryable_status(status), "{status} tidak boleh diretry");
        }
        let s = settings("http://localhost");
        assert_eq!(retry_delay(&s, 1), Duration::from_millis(10));
        assert_eq!(retry_delay(&s, 2), Duration::from_millis(20));
    }

    struct Counter {
        retries: AtomicUsize,
        waiting: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl ProgressReporter for Counter {
        async fn report(&self, progress: Progress) {
            match progress {
                Progress::Retrying { .. } => {
                    self.retries.fetch_add(1, Ordering::SeqCst);
                }
                Progress::WaitingResponse => {
                    self.waiting.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
    }

    #[tokio::test]
    async fn retries_bad_gateway_then_returns_answer() -> Result<()> {
        let hits = Arc::new(AtomicUsize::new(0));
        let counter_hits = hits.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let app = axum::Router::new().route(
            "/chat/completions",
            axum::routing::post(move || {
                let hits = counter_hits.clone();
                async move {
                    let count = hits.fetch_add(1, Ordering::SeqCst) + 1;
                    if count < 3 {
                        return (
                            axum::http::StatusCode::BAD_GATEWAY,
                            "server sibuk".to_string(),
                        );
                    }
                    let body = json!({
                        "choices": [{"message": {"role": "assistant", "content": "oke"}}]
                    });
                    (
                        axum::http::StatusCode::OK,
                        serde_json::to_string(&body).expect("valid json"),
                    )
                }
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let http = http_client(5)?;
        let counter = Arc::new(Counter {
            retries: AtomicUsize::new(0),
            waiting: AtomicUsize::new(0),
        });
        let (answer, _) = complete(
            &settings(&format!("http://{address}")),
            &http,
            vec![Message::user("halo")],
            Some(counter.clone()),
        )
        .await?;
        assert_eq!(answer, "oke");
        assert_eq!(hits.load(Ordering::SeqCst), 3);
        assert_eq!(counter.retries.load(Ordering::SeqCst), 2);
        assert_eq!(counter.waiting.load(Ordering::SeqCst), 1);
        server.abort();
        let _ = server.await;
        Ok(())
    }

    #[tokio::test]
    async fn client_error_is_not_retried() -> Result<()> {
        let hits = Arc::new(AtomicUsize::new(0));
        let counter_hits = hits.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let app = axum::Router::new().route(
            "/chat/completions",
            axum::routing::post(move || {
                let hits = counter_hits.clone();
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    (
                        axum::http::StatusCode::BAD_REQUEST,
                        "prompt salah".to_string(),
                    )
                }
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let http = http_client(5)?;
        let result = complete(
            &settings(&format!("http://{address}")),
            &http,
            vec![Message::user("halo")],
            None,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        server.abort();
        let _ = server.await;
        Ok(())
    }
}
