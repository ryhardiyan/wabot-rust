use super::Registry;

#[path = "internet-carbon.rs"]
mod internet_carbon;
#[path = "internet-cloudflare.rs"]
mod internet_cloudflare;
#[path = "internet-convert.rs"]
mod internet_convert;
#[path = "internet-perplexity.rs"]
mod internet_perplexity;
#[path = "internet-threecheck.rs"]
mod internet_threecheck;
#[path = "internet-trackpaket.rs"]
mod internet_trackpaket;
#[path = "internet-translate.rs"]
mod internet_translate;

pub(super) fn register(plugins: &mut Registry) {
    plugins.extend([
        Box::new(internet_carbon::Command) as Box<dyn crate::plugins::Plugin>,
        Box::new(internet_cloudflare::Command),
        Box::new(internet_convert::Command),
        Box::new(internet_perplexity::Command),
        Box::new(internet_threecheck::Command),
        Box::new(internet_trackpaket::Command),
        Box::new(internet_translate::Command),
    ]);
}
