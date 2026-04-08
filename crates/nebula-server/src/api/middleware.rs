//! Rate limiting and CORS middleware.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

/// Sliding-window in-memory rate limiter (100 req/min per IP default).
#[derive(Clone)]
pub struct RateLimiter {
    state: Arc<Mutex<HashMap<IpAddr, Vec<Instant>>>>,
    max_requests: usize,
}

impl RateLimiter {
    pub fn new(max_requests: usize) -> Self {
        Self { state: Arc::new(Mutex::new(HashMap::new())), max_requests }
    }

    pub async fn check(&self, ip: IpAddr) -> bool {
        let mut state = self.state.lock().await;
        let now = Instant::now();
        let window = std::time::Duration::from_secs(60);
        let timestamps = state.entry(ip).or_default();
        timestamps.retain(|t| now.duration_since(*t) < window);
        if timestamps.len() >= self.max_requests { return false; }
        timestamps.push(now);
        true
    }
}

/// Build a permissive CORS layer. Tighten `allow_origin` for production.
pub fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .max_age(std::time::Duration::from_secs(3600))
}
