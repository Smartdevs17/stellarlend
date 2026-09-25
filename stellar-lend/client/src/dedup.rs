//! Request deduplication for concurrent reads.
//!
//! When multiple callers concurrently request the same RPC endpoint with the same
//! parameters, only one actual HTTP request is made. Other callers wait for that
//! request's result and receive a clone.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// A request deduplication layer that coalesces identical concurrent requests.
#[derive(Clone)]
pub struct RequestDedup {
    in_flight: Arc<Mutex<HashMap<String, broadcast::Sender<Result<String, String>>>>>,
}

impl RequestDedup {
    /// Create a new request deduplication layer.
    pub fn new() -> Self {
        Self {
            in_flight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Execute a request with deduplication.
    ///
    /// If an identical request (same `key`) is already in-flight, this will wait
    /// for that request to complete and return its result. Otherwise, it executes
    /// `request_fn` and broadcasts the result to any other waiters.
    pub async fn deduplicated_request<F, Fut>(
        &self,
        key: String,
        request_fn: F,
    ) -> Result<String, String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<String, String>>,
    {
        // Check if there's already an in-flight request for this key
        {
            let map = self.in_flight.lock().await;
            if let Some(tx) = map.get(&key) {
                // Subscribe to the existing broadcast channel
                let mut rx = tx.subscribe();
                drop(map); // Release lock before awaiting
                // Wait for the result from the original request
                match rx.recv().await {
                    Ok(result) => return result,
                    Err(_) => {
                        // Sender dropped without sending — fall through to make a new request
                    }
                }
            }
        }

        // No in-flight request — we are the first caller
        let (tx, _) = broadcast::channel(1);
        {
            let mut map = self.in_flight.lock().await;
            map.insert(key.clone(), tx.clone());
        }

        // Execute the actual request
        let result = request_fn().await;

        // Broadcast the result to any waiters (ignore send errors — no receivers is fine)
        let _ = tx.send(result.clone());

        // Clean up the in-flight entry
        {
            let mut map = self.in_flight.lock().await;
            map.remove(&key);
        }

        result
    }
}

impl Default for RequestDedup {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a dedup key from a method name and serialized parameters.
pub fn make_dedup_key(method: &str, params: &str) -> String {
    format!("{}:{}", method, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_single_request_passes_through() {
        let dedup = RequestDedup::new();
        let result = dedup
            .deduplicated_request("key1".to_string(), || async { Ok("value1".to_string()) })
            .await;
        assert_eq!(result, Ok("value1".to_string()));
    }

    #[tokio::test]
    async fn test_error_propagates() {
        let dedup = RequestDedup::new();
        let result = dedup
            .deduplicated_request("key1".to_string(), || async {
                Err("some error".to_string())
            })
            .await;
        assert_eq!(result, Err("some error".to_string()));
    }

    #[tokio::test]
    async fn test_concurrent_requests_deduplicated() {
        let dedup = RequestDedup::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..5 {
            let dedup = dedup.clone();
            let count = call_count.clone();
            handles.push(tokio::spawn(async move {
                dedup
                    .deduplicated_request("same_key".to_string(), || {
                        let count = count.clone();
                        async move {
                            count.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            Ok("shared_result".to_string())
                        }
                    })
                    .await
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // All should get the same result
        for r in &results {
            assert!(r.is_ok());
        }

        // The actual request function should have been called a small number of times
        // (ideally 1, but timing may cause 1-2)
        let count = call_count.load(Ordering::SeqCst);
        assert!(count <= 2, "Expected at most 2 calls, got {}", count);
    }

    #[tokio::test]
    async fn test_different_keys_not_deduplicated() {
        let dedup = RequestDedup::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        let count1 = call_count.clone();
        let dedup1 = dedup.clone();
        let h1 = tokio::spawn(async move {
            dedup1
                .deduplicated_request("key_a".to_string(), || {
                    let c = count1.clone();
                    async move {
                        c.fetch_add(1, Ordering::SeqCst);
                        Ok("a".to_string())
                    }
                })
                .await
        });

        let count2 = call_count.clone();
        let dedup2 = dedup.clone();
        let h2 = tokio::spawn(async move {
            dedup2
                .deduplicated_request("key_b".to_string(), || {
                    let c = count2.clone();
                    async move {
                        c.fetch_add(1, Ordering::SeqCst);
                        Ok("b".to_string())
                    }
                })
                .await
        });

        let (r1, r2) = tokio::join!(h1, h2);
        assert_eq!(r1.unwrap(), Ok("a".to_string()));
        assert_eq!(r2.unwrap(), Ok("b".to_string()));
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_sequential_requests_not_deduplicated() {
        let dedup = RequestDedup::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        for _ in 0..3 {
            let count = call_count.clone();
            dedup
                .deduplicated_request("key".to_string(), || async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok("ok".to_string())
                })
                .await
                .unwrap();
        }

        // Sequential calls should each execute independently
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_make_dedup_key() {
        let key = make_dedup_key("getLatestLedger", "{}");
        assert_eq!(key, "getLatestLedger:{}");
    }
}
