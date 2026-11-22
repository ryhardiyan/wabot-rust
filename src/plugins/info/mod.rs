use super::Registry;

#[path = "info-cekganda.rs"]
mod info_cekganda;
#[path = "info-enka.rs"]
mod info_enka;
#[path = "info-menu.rs"]
mod info_menu;
#[path = "info-ping.rs"]
mod info_ping;

pub(super) fn register(plugins: &mut Registry) {
    plugins.extend([
        Box::new(info_ping::Command) as Box<dyn crate::plugins::Plugin>,
        Box::new(info_menu::Command),
        Box::new(info_enka::Command),
        Box::new(info_cekganda::Command),
    ]);
}
