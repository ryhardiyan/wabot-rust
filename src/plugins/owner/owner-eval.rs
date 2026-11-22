use crate::plugins::Context;
use anyhow::Result;
crate::plugins::plugin!(
    &["eval"],
    "owner",
    "Evaluasi ekspresi Rust melalui rustc",
    execute,
    owner_only = true,
    custom_prefix = Some("> ")
);
async fn execute(ctx: &Context) -> Result<()> {
    anyhow::ensure!(!ctx.text.is_empty(), "> <ekspresi Rust>");
    tokio::fs::create_dir_all("tmp").await?;
    let dir = tempfile::Builder::new().prefix("eval-").tempdir_in("tmp")?;
    let source = dir.path().join("main.rs");
    let executable = dir.path().join("eval");
    tokio::fs::write(
        &source,
        format!(
            "fn main() {{ let value = {{ {} }}; println!(\"{{:?}}\", value); }}",
            ctx.text
        ),
    )
    .await?;
    let mut compile = tokio::process::Command::new("rustc");
    compile
        .arg("--edition=2021")
        .arg(&source)
        .arg("-o")
        .arg(&executable);
    let output = super::owner_exec::run(compile, 30).await?;
    if !executable.exists() {
        return ctx.reply(output).await;
    }
    let run = tokio::process::Command::new(&executable);
    ctx.reply(super::owner_exec::run(run, 10).await?).await
}
