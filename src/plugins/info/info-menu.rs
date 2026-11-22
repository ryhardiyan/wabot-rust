use crate::plugins::Context;
crate::plugins::plugin!(
    &["menu", "help"],
    "info",
    "Daftar command sesuai izin",
    execute
);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let mut rows = std::collections::BTreeMap::<&str, Vec<String>>::new();
    for plugin in &ctx.app.registry {
        let m = plugin.metadata();
        if m.owner_only && !ctx.is_owner {
            continue;
        }
        let trigger = m.custom_prefix.unwrap_or("/");
        rows.entry(m.category).or_default().push(format!(
            "{}{} — {}",
            trigger,
            if m.custom_prefix.is_some() {
                "<kode>"
            } else {
                m.commands[0]
            },
            m.description
        ));
    }
    let mut menu = format!(
        "*{}*\n{}\n\n",
        ctx.app.config.watermark, ctx.app.config.author
    );
    for (category, commands) in rows {
        menu.push_str(&format!("*{category}*\n{}\n\n", commands.join("\n")));
    }
    ctx.reply(menu).await
}
