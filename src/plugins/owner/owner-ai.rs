use crate::plugins::Context;
use anyhow::Result;
crate::plugins::plugin!(&["ai"], "owner", execute, owner_only = true);

/// `.ai <prompt> [--delete]` — asisten AI owner-only dengan sesi chat persisten.
/// `--delete` di akhir argumen menghapus sesi chat chat ini sebelum memproses prompt.
async fn execute(ctx: &Context) -> Result<()> {
    let mut args = ctx.args.clone();
    let reset = args.last().is_some_and(|arg| arg == "--delete");
    if reset {
        args.pop();
    }
    let prompt = args.join(" ");
    if !reset && prompt.is_empty() {
        return ctx
            .reply(
                "🤖 `.ai <prompt> [--delete]`\n\
                 Contoh: `.ai baca Cargo.toml lalu ringkas dependensinya`\n\
                 `.ai hapus file tmp/old.txt`\n\
                 `.ai lanjutkan penjelasan sebelumnya --delete` (hapus sesi lama & mulai baru)",
            )
            .await;
    }

    match crate::ai::ask(&ctx.app, &ctx.m.chat, &prompt, reset).await {
        Ok(answer) => {
            if answer.len() > 12_000 {
                ctx.m
                    .send_media(answer.into_bytes(), "text/plain", "ai.txt", "Jawaban AI")
                    .await
            } else {
                ctx.reply(answer).await
            }
        }
        Err(error) => {
            tracing::warn!(command = "ai", error = %error, "AI gagal");
            ctx.reply(format!("⚠️ AI gagal: {error}")).await
        }
    }
}
