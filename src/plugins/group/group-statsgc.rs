use crate::plugins::Context;
crate::plugins::plugin!(
    &["statsgc"],
    "group",
    "Statistik metadata grup",
    execute,
    group_only = true
);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let g = if let Some(g) = ctx.app.store.groups.get(&ctx.m.chat).await {
        g
    } else {
        let g = ctx
            .m
            .socket
            .client
            .groups()
            .get_metadata(&ctx.m.chat.parse()?)
            .await?;
        ctx.app
            .store
            .groups
            .insert(ctx.m.chat.clone(), g.clone())
            .await;
        g
    };
    ctx.reply(format!(
        "*{}*\nAnggota: {}\nAdmin: {}\nID: {}",
        g.subject,
        g.participants.len(),
        g.participants.iter().filter(|p| p.is_admin()).count(),
        g.id
    ))
    .await
}
