//! Small, process-local query-vector cache. Each instance belongs to exactly
//! one provider configuration; it never stores document/search-result state.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::indexing::EmbeddingProvider;

const CAPACITY: usize = 64;
const TTL: Duration = Duration::from_secs(300);

type Entry = (String, Instant, Vec<f32>);

pub struct QueryEmbeddingCache {
    provider: Arc<dyn EmbeddingProvider>,
    entries: Mutex<VecDeque<Entry>>,
}

impl QueryEmbeddingCache {
    pub fn new(provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            provider,
            entries: Mutex::new(VecDeque::new()),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for QueryEmbeddingCache {
    fn enabled(&self) -> bool {
        self.provider.enabled()
    }

    fn model_name(&self) -> &str {
        self.provider.model_name()
    }

    fn dimensions(&self) -> Option<usize> {
        self.provider.dimensions()
    }

    async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        if !self.enabled() || texts.len() != 1 {
            return self.provider.embed(texts).await;
        }
        let key = texts[0].clone();
        {
            let mut entries = self
                .entries
                .lock()
                .map_err(|_| anyhow::anyhow!("query cache poisoned"))?;
            entries.retain(|(_, created, _)| created.elapsed() < TTL);
            if let Some(index) = entries.iter().position(|(text, _, _)| text == &key) {
                if let Some(entry) = entries.remove(index) {
                    let vector = entry.2.clone();
                    entries.push_back(entry);
                    tracing::debug!("[SEARCH_TIMING] query-vector cache hit");
                    return Ok(vec![vector]);
                }
            }
        }
        // Never hold the cache lock during network I/O. Failed, cancelled,
        // empty, non-finite and dimension-mismatched responses are not cached.
        let vectors = self.provider.embed(texts).await?;
        if vectors.len() == 1
            && !vectors[0].is_empty()
            && vectors[0].iter().all(|value| value.is_finite())
            && self
                .dimensions()
                .is_none_or(|expected| vectors[0].len() == expected)
            && key.len() <= 16384
        {
            let mut entries = self
                .entries
                .lock()
                .map_err(|_| anyhow::anyhow!("query cache poisoned"))?;
            entries.retain(|(text, created, _)| text != &key && created.elapsed() < TTL);
            while entries.len() >= CAPACITY {
                entries.pop_front();
            }
            entries.push_back((key, Instant::now(), vectors[0].clone()));
        }
        Ok(vectors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl EmbeddingProvider for Fake {
        fn enabled(&self) -> bool {
            true
        }
        fn model_name(&self) -> &str {
            "synthetic"
        }
        fn dimensions(&self) -> Option<usize> {
            Some(2)
        }
        async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            anyhow::ensure!(texts[0] != "fail", "synthetic failure");
            Ok(texts.into_iter().map(|_| vec![1.0, 0.0]).collect())
        }
    }

    #[tokio::test]
    async fn repeats_reuse_only_successful_query_vectors() {
        let provider = Arc::new(Fake {
            calls: AtomicUsize::new(0),
        });
        let cache = QueryEmbeddingCache::new(provider.clone());
        for _ in 0..2 {
            cache.embed(vec!["query".into()]).await.unwrap();
        }
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        for _ in 0..2 {
            assert!(cache.embed(vec!["fail".into()]).await.is_err());
        }
        assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn cache_is_bounded_expires_and_does_not_cross_instances() {
        let provider = Arc::new(Fake {
            calls: AtomicUsize::new(0),
        });
        let cache = QueryEmbeddingCache::new(provider.clone());
        for index in 0..=CAPACITY {
            cache.embed(vec![index.to_string()]).await.unwrap();
        }
        assert_eq!(cache.entries.lock().unwrap().len(), CAPACITY);
        cache.embed(vec!["0".into()]).await.unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), CAPACITY + 2);
        cache.entries.lock().unwrap().back_mut().unwrap().1 = Instant::now() - TTL;
        cache.embed(vec!["0".into()]).await.unwrap();
        let other = QueryEmbeddingCache::new(provider.clone());
        other.embed(vec!["0".into()]).await.unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), CAPACITY + 4);
    }
}
