//! Enkripsi Signal, pre-key, rotasi kunci, dan persistensi kredensial dimiliki
//! whatsapp-rust + SqliteStore. Tidak ada serialisasi ulang secret ke JSON.

use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Default)]
pub struct AuthState {
    logged_out: AtomicBool,
}
impl AuthState {
    pub fn mark_logged_out(&self) {
        self.logged_out.store(true, Ordering::Release);
    }
    pub fn logged_out(&self) -> bool {
        self.logged_out.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn logout_is_terminal_flag() {
        let state = super::AuthState::default();
        assert!(!state.logged_out());
        state.mark_logged_out();
        assert!(state.logged_out());
    }
}
