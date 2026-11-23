use crate::plugins::Context;
use anyhow::Result;

crate::plugins::plugin!(&["acc"], "owner", execute, owner_only = true);

async fn execute(ctx: &Context) -> Result<()> {
    crate::verify::approve(ctx).await
}
