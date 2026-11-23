use crate::{bot_stats::Snapshot, plugins::Context};
use anyhow::Result;
use std::time::Duration;

crate::plugins::plugin!(&["stats", "statbot"], "owner", execute, owner_only = true);

fn human_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let secs = seconds % 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days} hari"));
    }
    if hours > 0 {
        parts.push(format!("{hours} jam"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes} menit"));
    }
    if secs > 0 || parts.is_empty() {
        parts.push(format!("{secs} detik"));
    }
    parts.join(" ")
}

fn pct(part: u64, total: u64) -> String {
    if total == 0 {
        "0%".into()
    } else {
        format!("{:.1}%", part as f64 * 100.0 / total as f64)
    }
}

fn ms(value: u64) -> String {
    if value >= 1000 {
        format!("{:.2}s", value as f64 / 1000.0)
    } else {
        format!("{value}ms")
    }
}

fn render(snapshot: &Snapshot, brand: &str, author: &str) -> String {
    let processed = snapshot.processed();
    let mut text = String::new();
    text.push_str("━━━━━━━━━━━━━━━━━━━━━━━━\n");
    text.push_str(&format!("   *{brand} · BOT STATS*\n"));
    text.push_str(&format!("   © {author}\n"));
    text.push_str("━━━━━━━━━━━━━━━━━━━━━━━━\n\n");

    text.push_str(&format!(
        "⏱️ *Uptime*   : {}\n",
        human_duration(snapshot.uptime)
    ));
    text.push_str(&format!("📨 *Diproses* : *{processed}* command\n\n"));

    text.push_str("📊 *Ringkasan*\n");
    text.push_str(&format!(
        "   ✅ Sukses   : {} ({})\n",
        snapshot.ok,
        pct(snapshot.ok, processed)
    ));
    text.push_str(&format!(
        "   ❌ Gagal    : {} ({})\n",
        snapshot.fail,
        pct(snapshot.fail, processed)
    ));
    text.push_str(&format!(
        "   ⏳ Timeout  : {} ({})\n",
        snapshot.timeout,
        pct(snapshot.timeout, processed)
    ));
    text.push_str(&format!("   ⚡ Rata-rata: {}\n\n", ms(snapshot.avg_ms())));

    if snapshot.commands.is_empty() {
        text.push_str("_Belum ada perintah yang diproses._\n");
    } else {
        text.push_str("📈 *Per Command* · hit & terakhir dipakai\n");
        for (index, (name, stat)) in snapshot.commands.iter().take(10).enumerate() {
            let total = stat.total();
            let last = if stat.last_used == 0 {
                "-".to_string()
            } else {
                chrono::DateTime::from_timestamp(stat.last_used, 0)
                    .map(|date| {
                        date.with_timezone(&chrono::Local)
                            .format("%d/%m %H:%M")
                            .to_string()
                    })
                    .unwrap_or_else(|| "-".into())
            };
            text.push_str(&format!(
                "   {}. /{name:<12} {}x · {} ok · {last} · {}\n",
                index + 1,
                total,
                pct(stat.ok, total),
                ms(stat.avg_ms())
            ));
        }
        text.push('\n');
    }

    if snapshot.last_errors.is_empty() {
        text.push_str("🐞 *Error Terakhir*: bersih ✨\n");
    } else {
        text.push_str("🐞 *Error Terakhir*\n");
        for error in &snapshot.last_errors {
            let time = error.at.format("%d/%m %H:%M:%S");
            text.push_str(&format!(
                "   • /{} · {time} — {}\n",
                error.command, error.message
            ));
        }
    }

    text.push_str("\n━━━━━━━━━━━━━━━━━━━━━━━━\n");
    text.push_str("_Hit & terakhir dipakai tersimpan di database/plugins.db._");
    text
}

async fn execute(ctx: &Context) -> Result<()> {
    let snapshot = ctx.app.stats.snapshot();
    let text = render(&snapshot, ctx.app.config.brand(), &ctx.app.config.author);
    ctx.reply(text).await
}
