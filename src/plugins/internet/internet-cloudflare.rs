use crate::{bot_lib::downloader, database::JsonDatabase, plugins::Context};
use anyhow::{Context as _, Result};
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use tokio::sync::OnceCell;
crate::plugins::plugin!(
    &["cf", "cloudflare"],
    "internet",
    "Cloudflare DNS dan akses bersama kedaluwarsa",
    execute,
    owner_only = true
);
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
struct State {
    users: std::collections::HashMap<String, Value>,
    shares: std::collections::HashMap<String, Value>,
}
static DB: OnceLock<OnceCell<Arc<JsonDatabase<State>>>> = OnceLock::new();
async fn api(
    ctx: &Context,
    user: &Value,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value> {
    let mut request = ctx
        .app
        .http
        .request(
            method,
            format!("https://api.cloudflare.com/client/v4/{path}"),
        )
        .header(
            "X-Auth-Email",
            user["email"].as_str().context("Email CF kosong")?,
        )
        .header(
            "X-Auth-Key",
            user["apiKey"].as_str().context("API key CF kosong")?,
        );
    if let Some(body) = body {
        request = request.json(&body);
    }
    let value = downloader::json(request.send().await?).await?;
    anyhow::ensure!(value["success"] == true, "Cloudflare API menolak operasi");
    Ok(value["result"].clone())
}
async fn execute(ctx: &Context) -> Result<()> {
    let db = DB
        .get_or_init(OnceCell::new)
        .get_or_try_init(|| async {
            Ok::<_, anyhow::Error>(Arc::new(
                JsonDatabase::<State>::open("database/cloudflare.json").await?,
            ))
        })
        .await?;
    let snapshot = db.snapshot().await;
    let key = &ctx.m.sender;
    match ctx.arg(0) {
        "addkey" => {
            anyhow::ensure!(
                !ctx.arg(1).is_empty() && !ctx.arg(2).is_empty(),
                "/cf addkey <email> <global_api_key>"
            );
            let mut user = json!({"email":ctx.arg(1),"apiKey":ctx.arg(2)});
            user["zones"] = api(ctx, &user, reqwest::Method::GET, "zones", None).await?;
            user["activeZoneId"] = user.pointer("/zones/0/id").cloned().unwrap_or(Value::Null);
            db.update(|s| {
                s.users.insert(key.clone(), user);
                Ok(())
            })
            .await?;
            ctx.reply("✅ Akun Cloudflare terhubung").await
        }
        "sharecf" => {
            anyhow::ensure!(
                snapshot.users.contains_key(key),
                "Jalankan /cf addkey terlebih dahulu"
            );
            let duration = ctx.arg(2);
            let unit = duration.chars().last().context("Durasi harus 30m/2h/1d")?;
            let amount = duration
                .get(..duration.len().saturating_sub(1))
                .context("Durasi tidak valid")?
                .parse::<i64>()?;
            let seconds = amount
                .checked_mul(match unit {
                    'm' => 60,
                    'h' => 3600,
                    'd' => 86400,
                    _ => anyhow::bail!("Durasi harus m/h/d"),
                })
                .context("Durasi terlalu besar")?;
            anyhow::ensure!(
                seconds > 0 && seconds <= 30 * 86400 && !ctx.arg(1).is_empty(),
                "Durasi harus positif maksimal 30 hari"
            );
            let expires = chrono::Utc::now().timestamp() + seconds;
            db.update(|s| {
                anyhow::ensure!(
                    !s.shares.contains_key(ctx.arg(1)),
                    "Password share sudah ada"
                );
                s.shares
                    .insert(ctx.arg(1).into(), json!({"owner":key,"expiresAt":expires}));
                Ok(())
            })
            .await?;
            ctx.reply(format!("✅ Share aktif sampai timestamp {expires}"))
                .await
        }
        "addshare" => {
            let ticket = snapshot
                .shares
                .get(ctx.arg(1))
                .context("Tiket tidak ditemukan")?;
            anyhow::ensure!(
                ticket["expiresAt"].as_i64().unwrap_or(0) > chrono::Utc::now().timestamp(),
                "Tiket kedaluwarsa"
            );
            anyhow::ensure!(
                ticket["owner"].as_str() != Some(key.as_str()),
                "Tiket share milik akun yang sama tidak perlu dipasang"
            );
            // Simpan referensi tiket, bukan salin permanen kredensial yang melewati masa berlaku.
            db.update(|s| {
                s.users.insert(key.clone(), json!({"share":ctx.arg(1)}));
                Ok(())
            })
            .await?;
            ctx.reply("✅ Akses bersama terpasang").await
        }
        "setzone" | "listdns" | "adddns" | "randomdns" | "deldns" => {
            let mut user = snapshot
                .users
                .get(key)
                .context("Hubungkan akun melalui /cf addkey")?
                .clone();
            let shared = snapshot
                .users
                .get(key)
                .and_then(|u| u.get("share"))
                .cloned();
            if let Some(ticket) = user["share"].as_str() {
                let t = snapshot
                    .shares
                    .get(ticket)
                    .context("Tiket tidak ditemukan")?;
                anyhow::ensure!(
                    t["expiresAt"].as_i64().unwrap_or(0) > chrono::Utc::now().timestamp(),
                    "Tiket kedaluwarsa"
                );
                user = snapshot
                    .users
                    .get(t["owner"].as_str().context("Pemilik tiket kosong")?)
                    .context("Akun pemilik hilang")?
                    .clone();
            }
            if ctx.arg(0) == "setzone" {
                if ctx.arg(1).is_empty() {
                    return ctx.json(&user["zones"]).await;
                }
                let index = ctx
                    .arg(1)
                    .parse::<usize>()?
                    .checked_sub(1)
                    .context("Indeks mulai 1")?;
                user["activeZoneId"] = user["zones"]
                    .get(index)
                    .and_then(|z| z.get("id"))
                    .context("Zone tidak ditemukan")?
                    .clone();
                let selected = user["activeZoneId"].clone();
                db.update(|s| {
                    if let Some(ticket) = shared {
                        s.users
                            .insert(key.clone(), json!({"share":ticket,"activeZoneId":selected}));
                    } else {
                        s.users.insert(key.clone(), user);
                    }
                    Ok(())
                })
                .await?;
                return ctx.reply("✅ Zone aktif diperbarui").await;
            }
            if let Some(selected) = snapshot.users.get(key).and_then(|u| u.get("activeZoneId")) {
                user["activeZoneId"] = selected.clone();
            }
            let zone = user["activeZoneId"]
                .as_str()
                .context("Pilih zone terlebih dahulu")?;
            let path = format!("zones/{zone}/dns_records");
            let result = match ctx.arg(0) {
                "listdns" => api(ctx, &user, reqwest::Method::GET, &path, None).await?,
                "deldns" => {
                    let records = api(ctx, &user, reqwest::Method::GET, &path, None).await?;
                    let record = records
                        .as_array()
                        .context("Records kosong")?
                        .iter()
                        .find(|r| r["name"].as_str() == Some(ctx.arg(1)))
                        .context("Nama record tidak ditemukan")?;
                    api(
                        ctx,
                        &user,
                        reqwest::Method::DELETE,
                        &format!(
                            "{path}/{}",
                            record["id"].as_str().context("Record ID kosong")?
                        ),
                        None,
                    )
                    .await?
                }
                mode => {
                    let random = uuid::Uuid::new_v4().simple().to_string()[..6].to_string();
                    let (sub, typ, target, proxy) = if mode == "randomdns" {
                        (random.as_str(), ctx.arg(1), ctx.arg(2), ctx.arg(3))
                    } else {
                        (ctx.arg(1), ctx.arg(2), ctx.arg(3), ctx.arg(4))
                    };
                    anyhow::ensure!(
                        ["A", "CNAME"].contains(&typ.to_uppercase().as_str()) && !target.is_empty(),
                        "/cf adddns <sub> A|CNAME <target> on|off"
                    );
                    let domain = user["zones"]
                        .as_array()
                        .context("Zones kosong")?
                        .iter()
                        .find(|z| z["id"] == zone)
                        .and_then(|z| z["name"].as_str())
                        .context("Nama zone kosong")?;
                    let name = if sub == "@" {
                        domain.into()
                    } else {
                        format!("{sub}.{domain}")
                    };
                    api(ctx,&user,reqwest::Method::POST,&path,Some(json!({"type":typ.to_uppercase(),"name":name,"content":target,"proxied":proxy=="on"}))).await?
                }
            };
            ctx.json(&result).await
        }
        _ => {
            ctx.reply("/cf addkey|sharecf|addshare|setzone|listdns|adddns|randomdns|deldns")
                .await
        }
    }
}
