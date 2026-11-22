use super::Registry;

#[path = "stalk-getcontact.rs"]
mod stalk_getcontact;
#[path = "stalk-github.rs"]
mod stalk_github;
#[path = "stalk-roblox.rs"]
mod stalk_roblox;
#[path = "stalk-stalk.rs"]
mod stalk_stalk;
#[path = "stalk-stalktiktok.rs"]
mod stalk_stalktiktok;

pub(super) fn register(plugins: &mut Registry) {
    tracing::debug!(
        reason = stalk_getcontact::DISABLED_REASON,
        "Plugin Getcontact tetap nonaktif"
    );
    plugins.extend([
        Box::new(stalk_github::Command) as Box<dyn crate::plugins::Plugin>,
        Box::new(stalk_roblox::Command),
        Box::new(stalk_stalk::Command),
        Box::new(stalk_stalktiktok::Command),
    ]);
}
