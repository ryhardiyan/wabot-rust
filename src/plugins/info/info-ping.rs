use crate::plugins::Context;
crate::plugins::plugin!(&["ping"], "info", "Status runtime dan uptime", execute);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    ctx.reply(format!(
        "🏓 Pong!\nUptime: {} detik\nPlugin: {}\nAdmin: {} · Bot admin: {}",
        ctx.app.started.elapsed().as_secs(),
        ctx.app.registry.len(),
        ctx.is_admin,
        ctx.is_bot_admin
    ))
    .await
}
