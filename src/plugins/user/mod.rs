use super::Registry;

#[path = "user-register.rs"]
pub(super) mod user_register;

pub(super) fn register(plugins: &mut Registry) {
    plugins.push(Box::new(user_register::Command));
}
