use crate::plugins::App;
use anyhow::Result;
use std::sync::Arc;
use whatsapp_rust::prelude::Bot;

pub async fn run(app: Arc<App>) -> Result<()> {
    let store = super::auth::session::open(&app.config.session_path).await?;
    let auth = Arc::new(super::auth::core::AuthState::default());
    let logged_out = auth.clone();
    let messages = app.clone();
    let admission = app.permits.clone();
    let tasks = Arc::new(tokio::sync::Mutex::new(tokio::task::JoinSet::new()));
    let message_tasks = tasks.clone();
    let mut builder = Bot::builder()
        .with_backend(store)
        .skip_history_sync()
        .on_qr_code(|code, timeout| async move {
            match qrcode::QrCode::new(code.as_bytes()) {
                Ok(qr) => println!(
                    "QR berlaku {} detik:\n{}",
                    timeout.as_secs(),
                    qr.render::<qrcode::render::unicode::Dense1x2>().build()
                ),
                Err(error) => tracing::error!(%error, "QR gagal dirender"),
            }
        })
        .on_pair_code(|code, timeout| async move {
            println!("PAIRING CODE: {code} ({} detik)", timeout.as_secs());
        })
        .on_connected(|_| async {
            tracing::info!("WhatsApp tersambung; reconnect ditangani runtime upstream");
        })
        .on_pair_code_error(|error, _client| async move {
            tracing::error!(?error, "Pairing kode gagal");
        })
        .on_logged_out(move |_| {
            let auth = logged_out.clone();
            async move {
                auth.mark_logged_out();
                tracing::error!("Sesi logout; lakukan pairing ulang pada restart berikutnya");
            }
        })
        .on_message(move |ctx| {
            let app = messages.clone();
            let permit = admission.clone().try_acquire_owned();
            let tasks = message_tasks.clone();
            async move {
                let Ok(permit) = permit else {
                    return;
                };
                let mut tasks = tasks.lock().await;
                while let Some(result) = tasks.try_join_next() {
                    if let Err(error) = result {
                        tracing::warn!(%error,"Task pesan berhenti");
                    }
                }
                tasks.spawn(async move {
                    let _permit = permit;
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(720),
                        crate::handler::handle(app, ctx),
                    )
                    .await
                    {
                        Ok(Ok(())) => (),
                        Ok(Err(error)) => tracing::warn!(%error,"Gagal menangani pesan"),
                        Err(_) => tracing::warn!("Batas waktu handler tercapai"),
                    }
                });
            }
        });
    if let Some(pairing) = super::auth::config::pairing(&app.config) {
        builder = builder.with_pair_code(pairing);
    }
    let bot = builder.build().await?;
    let client = bot.client();
    let mut handle = bot.spawn();
    let webhook = if app.config.webhook_token.is_some() {
        let app = app.clone();
        Some(tokio::spawn(async move {
            if let Err(error) =
                super::orderkuota::serve(app, super::socket::WaSocket { client }).await
            {
                tracing::error!(%error,"Server webhook berhenti");
            }
        }))
    } else {
        None
    };
    tokio::select! {
        _ = &mut handle => { tracing::info!(logged_out = auth.logged_out(), "Runtime WhatsApp selesai"); }
        _ = shutdown() => { tracing::info!("Menghentikan bot dan menyimpan sesi"); handle.shutdown().await; }
    }
    if let Some(task) = webhook {
        task.abort();
        let _ = task.await;
    }
    let mut tasks = tasks.lock().await;
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    Ok(())
}
async fn shutdown() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                tokio::select! { _ = signal.recv() => (), _ = tokio::signal::ctrl_c() => () }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
