use crate::plugins::Context;
use std::collections::BTreeMap;

crate::plugins::plugin!(&["menu", "help"], "info", execute);

async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let filter = ctx.arg(0).trim().to_ascii_lowercase();
    let mut rows = BTreeMap::<&str, Vec<String>>::new();
    for plugin in &ctx.app.registry {
        let meta = plugin.metadata();
        if meta.owner_only && !ctx.is_owner {
            continue;
        }
        if !filter.is_empty() && !meta.category.to_ascii_lowercase().contains(&filter) {
            continue;
        }
        let trigger = meta.custom_prefix.unwrap_or("/");
        let command = match meta.custom_prefix {
            Some(_) => format!("{trigger}<kode>"),
            None => format!("{trigger}{}", meta.commands[0]),
        };
        rows.entry(meta.category).or_default().push(command);
    }

    let total = rows.values().map(Vec::len).sum::<usize>();
    let categories = rows.len();

    let mut text = String::new();
    text.push_str("━━━━━━━━━━━━━━━━━━━━━━━━\n");
    text.push_str(&format!(
        "   *{} MENU*\n",
        ctx.app.config.brand().to_ascii_uppercase()
    ));
    text.push_str(&format!("   © {}\n", ctx.app.config.author));
    text.push_str("━━━━━━━━━━━━━━━━━━━━━━━━\n\n");

    if rows.is_empty() {
        text.push_str("Tidak ada command untuk kategori tersebut.\n");
    } else {
        for (category, commands) in rows {
            text.push_str(&format!(
                "▸ *{}*  `{} command`\n",
                category.to_ascii_uppercase(),
                commands.len()
            ));
            for command in commands {
                text.push_str(&format!("   • {command}\n"));
            }
            text.push('\n');
        }
    }

    text.push_str("━━━━━━━━━━━━━━━━━━━━━━━━\n");
    text.push_str(&format!("*{total}* command · *{categories}* kategori\n"));
    text.push_str("_Ketik /menu <kategori> untuk memfilter._");

    ctx.reply(text).await
}
