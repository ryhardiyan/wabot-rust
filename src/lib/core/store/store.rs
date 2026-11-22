use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

pub struct Cache<V> {
    entries: RwLock<HashMap<String, (Instant, V)>>,
    capacity: usize,
    ttl: Duration,
}
impl<V: Clone> Cache<V> {
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            capacity,
            ttl,
        }
    }
    pub async fn get(&self, key: &str) -> Option<V> {
        self.entries
            .read()
            .await
            .get(key)
            .filter(|(at, _)| at.elapsed() < self.ttl)
            .map(|(_, v)| v.clone())
    }
    pub async fn insert(&self, key: String, value: V) {
        if self.capacity == 0 {
            return;
        }
        let mut map = self.entries.write().await;
        map.retain(|_, (at, _)| at.elapsed() < self.ttl);
        if map.len() >= self.capacity {
            if let Some(oldest) = map
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(k, _)| k.clone())
            {
                map.remove(&oldest);
            }
        }
        map.insert(key, (Instant::now(), value));
    }
    pub async fn remove(&self, key: &str) {
        self.entries.write().await.remove(key);
    }
    pub async fn insert_if_absent(&self, key: String, value: V) -> bool {
        let mut map = self.entries.write().await;
        map.retain(|_, (at, _)| at.elapsed() < self.ttl);
        if map.contains_key(&key) || self.capacity == 0 {
            return false;
        }
        if map.len() >= self.capacity {
            if let Some(oldest) = map
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(k, _)| k.clone())
            {
                map.remove(&oldest);
            }
        }
        map.insert(key, (Instant::now(), value));
        true
    }
}

pub struct Store {
    pub groups: Cache<whatsapp_rust::features::GroupMetadata>,
    pub contacts: Cache<String>,
    pub messages: Cache<std::sync::Arc<whatsapp_rust::waproto::whatsapp::Message>>,
    pub seen: Cache<()>,
}
impl Default for Store {
    fn default() -> Self {
        Self {
            groups: Cache::new(64, Duration::from_secs(120)),
            contacts: Cache::new(512, Duration::from_secs(3600)),
            messages: Cache::new(128, Duration::from_secs(300)),
            seen: Cache::new(2048, Duration::from_secs(600)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn bounded_and_expiring() {
        let c = Cache::new(1, Duration::from_millis(5));
        c.insert("a".into(), 1).await;
        c.insert("b".into(), 2).await;
        assert_eq!(c.get("a").await, None);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(c.get("b").await, None);
    }
    #[tokio::test]
    async fn dedup_is_atomic() {
        let cache = std::sync::Arc::new(Cache::new(10, Duration::from_secs(10)));
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..10 {
            let cache = cache.clone();
            tasks.spawn(async move { cache.insert_if_absent("same".into(), ()).await });
        }
        let mut inserted = 0;
        while let Some(r) = tasks.join_next().await {
            if r.is_ok_and(|b| b) {
                inserted += 1;
            }
        }
        assert_eq!(inserted, 1);
    }
}
