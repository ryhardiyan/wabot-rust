use super::Registry;

#[path = "maker-brat.rs"]
mod maker_brat;
#[path = "maker-cardgenerate.rs"]
mod maker_cardgenerate;
#[path = "maker-qc.rs"]
mod maker_qc;
#[path = "maker-sticker.rs"]
mod maker_sticker;
#[path = "maker-stickeraudio.rs"]
mod maker_stickeraudio;

pub(super) fn register(plugins: &mut Registry) {
    plugins.extend([
        Box::new(maker_brat::Command) as Box<dyn crate::plugins::Plugin>,
        Box::new(maker_cardgenerate::Command),
        Box::new(maker_qc::Command),
        Box::new(maker_sticker::Command),
        Box::new(maker_stickeraudio::Command),
    ]);
}
