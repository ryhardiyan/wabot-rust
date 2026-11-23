use crate::plugins::Context;
use anyhow::{Context as _, Result};
crate::plugins::plugin!(
    &["userlist", "listuser", "unreguser", "accuser"],
    "owner",
    execute,
    owner_only = true
);
async fn execute(ctx: &Context) -> Result<()> {
    if matches!(ctx.command.as_str(), "userlist" | "listuser") {
        return ctx
            .json(&serde_json::json!(ctx.app.db.snapshot().await.users))
            .await;
    }
    let target = ctx
        .m
        .mentions
        .first()
        .map(String::as_str)
        .or_else(|| ctx.m.quoted.as_ref().map(|q| q.sender.as_str()))
        .unwrap_or(ctx.arg(0));
    anyhow::ensure!(
        !target.is_empty(),
        "Tentukan nomor, mention, atau balas pesan user"
    );
    ctx.app
        .db
        .update(|d| {
            let mut id = d.resolve(target);
            if !d.users.contains_key(&id) {
                let number = crate::config::clean_number(target);
                if !number.is_empty() {
                    id = d.resolve(&format!("{number}@s.whatsapp.net"));
                }
            }
            let user = d.users.get_mut(&id).context("User tidak ditemukan")?;
            user["registered"] = serde_json::json!(ctx.command == "accuser");
            Ok(())
        })
        .await?;
    ctx.reply("✅ Status registrasi diperbarui").await
}
