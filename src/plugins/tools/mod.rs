use super::Registry;

#[path = "tools-fetch.rs"]
mod tools_fetch;
#[path = "tools-hdvideo.rs"]
mod tools_hdvideo;
#[path = "tools-readviewonce.rs"]
mod tools_readviewonce;
#[path = "tools-tourl.rs"]
mod tools_tourl;
#[path = "tools-tourlgithub.rs"]
mod tools_tourlgithub;
#[path = "tools-upscale.rs"]
mod tools_upscale;
#[path = "tools-whatmusic.rs"]
mod tools_whatmusic;
#[path = "tools-wink.rs"]
mod tools_wink;

pub(super) fn register(plugins: &mut Registry) {
    plugins.extend([
        Box::new(tools_fetch::Command) as Box<dyn crate::plugins::Plugin>,
        Box::new(tools_hdvideo::Command),
        Box::new(tools_readviewonce::Command),
        Box::new(tools_tourl::Command),
        Box::new(tools_tourlgithub::Command),
        Box::new(tools_upscale::Command),
        Box::new(tools_whatmusic::Command),
        Box::new(tools_wink::Command),
    ]);
}
