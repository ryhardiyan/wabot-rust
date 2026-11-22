use super::Registry;

mod deposit;
#[path = "minecraft-server.rs"]
mod minecraft_server;
mod orderstars;
#[path = "owner-backup.rs"]
mod owner_backup;
#[path = "owner-eval.rs"]
mod owner_eval;
#[path = "owner-exec.rs"]
mod owner_exec;
#[path = "owner-file.rs"]
mod owner_file;
#[path = "owner-groupstatus.rs"]
mod owner_groupstatus;
#[path = "user-manage.rs"]
mod user_manage;

pub(super) fn register(plugins: &mut Registry) {
    plugins.extend([
        Box::new(deposit::Command) as Box<dyn crate::plugins::Plugin>,
        Box::new(minecraft_server::Command),
        Box::new(orderstars::Command),
        Box::new(owner_eval::Command),
        Box::new(owner_exec::Command),
        Box::new(owner_backup::Command),
        Box::new(owner_file::Command),
        Box::new(owner_groupstatus::Command),
        Box::new(user_manage::Command),
    ]);
}
