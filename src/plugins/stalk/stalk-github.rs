use crate::{bot_lib::downloader, plugins::Context};
crate::plugins::plugin!(
    &["ghstalk", "githubstalk", "stalkgh"],
    "stalk",
    "Profil publik GitHub",
    execute
);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let user = ctx.arg(0);
    anyhow::ensure!(
        !user.is_empty() && user.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "/ghstalk <username>"
    );
    let v = downloader::get_json(
        &ctx.app.http,
        &format!("https://api.github.com/users/{user}"),
        &[],
    )
    .await?;
    ctx.json(&v).await
}
