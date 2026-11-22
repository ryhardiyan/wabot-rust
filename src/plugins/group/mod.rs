use super::Registry;

#[path = "group-hidetag.rs"]
mod group_hidetag;
#[path = "group-settings.rs"]
mod group_settings;
#[path = "group-statsgc.rs"]
mod group_statsgc;

pub(super) fn register(plugins: &mut Registry) {
    plugins.extend([
        Box::new(group_hidetag::Command) as Box<dyn crate::plugins::Plugin>,
        Box::new(group_settings::Command),
        Box::new(group_statsgc::Command),
    ]);
}
