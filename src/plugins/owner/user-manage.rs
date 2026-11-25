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

    let approve = ctx.command == "accuser";
    // Database verifikasi SQLite adalah sumber kebenaran untuk akses. Dulu
    // `.accuser`/`.unreguser` hanya mengubah JSON sehingga user tetap ditolak
    // (atau tetap punya akses setelah dicabut).
    let verify_jid = match target.parse::<whatsapp_rust::Jid>() {
        Ok(jid) => jid.to_string(),
        Err(_) => crate::verify::vcard::phone_jid(target)?,
    };
    if approve {
        ctx.app.verify.set_verified(&verify_jid).await?;
    } else {
        ctx.app.verify.set_unregistered(&verify_jid).await?;
    }

    // Sinkronkan database JSON lama agar tampilan `.cekreg` tetap konsisten.
    let jid_for_json = verify_jid.clone();
    if let Err(error) = ctx
        .app
        .db
        .update(move |d| {
            let mut id = d.resolve(&jid_for_json);
            if !d.users.contains_key(&id) {
                let number = crate::config::clean_number(&jid_for_json);
                if !number.is_empty() {
                    id = d.resolve(&format!("{number}@s.whatsapp.net"));
                }
            }
            let user = d.users.get_mut(&id).context("User tidak ditemukan")?;
            user["registered"] = serde_json::json!(approve);
            user["registeredAt"] = if approve {
                serde_json::json!(chrono::Utc::now().to_rfc3339())
            } else {
                serde_json::Value::Null
            };
            Ok(())
        })
        .await
    {
        tracing::warn!(%error, jid = %verify_jid, "Sinkronisasi status JSON gagal");
    }

    ctx.reply(if approve {
        format!("✅ User {verify_jid} diverifikasi dan kini bisa menggunakan bot.")
    } else {
        format!("✅ Akses user {verify_jid} dicabut.")
    })
    .await
}
