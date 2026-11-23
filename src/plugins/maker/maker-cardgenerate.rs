use crate::{bot_lib::downloader, plugins::Context};
use serde_json::Value;
crate::plugins::plugin!(&["ccgen", "cardgen"], "maker", execute);

fn pick(v: &Value, keys: &[&str]) -> String {
    for key in keys {
        let value = &v[*key];
        if let Some(s) = value.as_str() {
            if !s.is_empty() && s != "-" {
                return s.into();
            }
        }
        if let Some(n) = value.as_u64() {
            return n.to_string();
        }
    }
    "-".into()
}

fn cards_text(v: &Value, typ: &str) -> Option<String> {
    let items = v.as_array().or_else(|| v["data"].as_array())?;
    if items.is_empty() {
        return None;
    }
    let mut out = String::from(
        "━━━━━━━━━━━━━━━━━━━━━━━━\n*HASIL CARD GENERATOR*\n━━━━━━━━━━━━━━━━━━━━━━━━\n",
    );
    for (index, card) in items.iter().take(10).enumerate() {
        out.push_str(&format!(
            "{}. *{}* · `{}` · exp {}/{} · cvv `{}`\n",
            index + 1,
            pick(card, &["type", "brand", "card_type"]),
            pick(
                card,
                &["card_number", "cardNumber", "number", "credit_card"]
            ),
            pick(
                card,
                &["expiry_month", "exp_month", "expMonth", "expire_month"]
            ),
            pick(card, &["expiry_year", "exp_year", "expYear", "expire_year"]),
            pick(card, &["cvv", "cvc", "cvv2"]),
        ));
    }
    out.push_str(&format!("\n_Tipe: {typ}_"));
    Some(out)
}

async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let amount = ctx
        .arg(1)
        .parse::<u32>()
        .unwrap_or(1)
        .clamp(1, 10)
        .to_string();
    let typ = if ctx.arg(0).is_empty() {
        "visa"
    } else {
        ctx.arg(0)
    };
    let v = downloader::get_json(
        &ctx.app.http,
        "https://backend.lambdatest.com/api/dev-tools/credit-card-generator",
        &[("type", typ), ("no-of-cards", &amount)],
    )
    .await?;
    match cards_text(&v, typ) {
        Some(text) => ctx.reply(text).await,
        None => ctx.json(&v).await,
    }
}
