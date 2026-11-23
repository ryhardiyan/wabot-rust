use crate::{bot_lib::downloader, plugins::Context};
crate::plugins::plugin!(
    &["tricheck", "cektri", "tri", "tricek"],
    "internet",
    execute
);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let number = crate::config::clean_number(&ctx.text);
    anyhow::ensure!(
        regex::Regex::new(r"^6289[5-9]\d{5,10}$")?.is_match(&number),
        "Masukkan nomor Tri 0895–0899"
    );
    let v=downloader::json(ctx.app.http.post("https://tri.co.id/api/v1/information/sim-status").header("Origin","https://tri.co.id").header("Referer","https://tri.co.id/").json(&serde_json::json!({"action":"MSISDN_STATUS_WEB","input1":"","input2":"","language":"ID","msisdn":number})).send().await?).await?;
    ctx.json(&v).await
}
