use crate::{bot_lib::downloader, plugins::Context};
use anyhow::Result;
use serde_json::{json, Value};
crate::plugins::plugin!(&["perplexity", "pplx"], "internet", execute);
async fn execute(ctx: &Context) -> Result<()> {
    anyhow::ensure!(!ctx.text.is_empty(), "/perplexity <pertanyaan>");
    let response=ctx.app.http.post("https://www.perplexity.ai/rest/sse/perplexity_ask").header("origin","https://www.perplexity.ai").header("referer","https://www.perplexity.ai/")
        .json(&json!({"query_str":ctx.text,"params":{"source":"default","language":"id-ID","timezone":"Asia/Jakarta","search_focus":"internet","mode":"concise","frontend_uuid":uuid::Uuid::new_v4().to_string(),"frontend_context_uuid":uuid::Uuid::new_v4().to_string()}})).send().await?;
    let body = String::from_utf8(downloader::bounded(response, 2 * 1024 * 1024).await?)?;
    let mut answer = String::new();
    for line in body.lines().filter_map(|l| l.strip_prefix("data: ")) {
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            if let Some(s) = value
                .get("answer")
                .or_else(|| value.get("text"))
                .and_then(Value::as_str)
            {
                if let Ok(nested) = serde_json::from_str::<Value>(s) {
                    if let Some(s) = nested["answer"].as_str() {
                        answer = s.into();
                    }
                } else {
                    answer = s.into();
                }
            }
        }
    }
    anyhow::ensure!(
        !answer.is_empty(),
        "Perplexity tidak mengembalikan jawaban SSE; endpoint membutuhkan sesi valid"
    );
    ctx.reply(format!("*PERPLEXITY*\n\n{answer}")).await
}
