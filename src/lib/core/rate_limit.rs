use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    Limited { retry_after_secs: u64, notify: bool },
}

pub struct RateLimiter {
    limit: u32,
    window: Duration,
    buckets: Mutex<HashMap<String, Bucket>>,
}

#[derive(Default)]
struct Bucket {
    hits: VecDeque<Instant>,
    last_notice: Option<Instant>,
}

impl RateLimiter {
    pub fn new(limit_per_minute: u32) -> Self {
        Self {
            limit: limit_per_minute,
            window: Duration::from_secs(60),
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Mengembalikan `Allowed` atau `Limited` beserta sisa tunggu.
    /// Notifikasi hanya dikirim sekali per jendela waktu agar tidak membanjiri chat.
    pub async fn check(&self, key: &str) -> Decision {
        if self.limit == 0 {
            return Decision::Allowed;
        }
        let now = Instant::now();
        let mut buckets = self.buckets.lock().await;
        if buckets.len() > 1024 {
            buckets.retain(|_, bucket| {
                bucket
                    .hits
                    .back()
                    .is_some_and(|at| now.duration_since(*at) < self.window)
            });
        }
        let bucket = buckets.entry(key.to_string()).or_default();
        while let Some(oldest) = bucket.hits.front() {
            if now.duration_since(*oldest) >= self.window {
                bucket.hits.pop_front();
            } else {
                break;
            }
        }
        if bucket.hits.len() >= self.limit as usize {
            let oldest = bucket.hits.front().copied().unwrap_or(now);
            let retry_after_secs = self
                .window
                .saturating_sub(now.duration_since(oldest))
                .as_secs()
                .max(1);
            let notify = bucket
                .last_notice
                .is_none_or(|at| now.duration_since(at) >= self.window);
            if notify {
                bucket.last_notice = Some(now);
            }
            Decision::Limited {
                retry_after_secs,
                notify,
            }
        } else {
            bucket.hits.push_back(now);
            Decision::Allowed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn limits_per_key_after_quota() {
        let limiter = RateLimiter::new(2);
        assert_eq!(limiter.check("a").await, Decision::Allowed);
        assert_eq!(limiter.check("a").await, Decision::Allowed);
        assert!(matches!(
            limiter.check("a").await,
            Decision::Limited { notify: true, .. }
        ));
        assert!(matches!(
            limiter.check("a").await,
            Decision::Limited { notify: false, .. }
        ));
        assert_eq!(limiter.check("b").await, Decision::Allowed);
    }

    #[tokio::test]
    async fn zero_limit_disables_check() {
        let limiter = RateLimiter::new(0);
        for _ in 0..100 {
            assert_eq!(limiter.check("a").await, Decision::Allowed);
        }
    }
}
