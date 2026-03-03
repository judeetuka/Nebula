//! Per-key rate limiter for plugin orchestration.
//!
//! Uses a simple minimum-interval strategy: each key must wait at least
//! `interval` duration between consecutive acquires. Keys that have not been
//! configured fall back to `default_interval`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A per-key rate limiter based on minimum intervals between acquires.
pub struct RateLimiter {
    intervals: HashMap<String, Duration>,
    last_acquire: HashMap<String, Instant>,
    default_interval: Duration,
}

impl RateLimiter {
    /// Create a new rate limiter with the given default minimum interval.
    pub fn new(default_interval: Duration) -> Self {
        Self {
            intervals: HashMap::new(),
            last_acquire: HashMap::new(),
            default_interval,
        }
    }

    /// Set the minimum interval for a specific key.
    pub fn set_interval(&mut self, key: &str, interval: Duration) {
        self.intervals.insert(key.to_string(), interval);
    }

    /// Return the configured interval for a key, falling back to `default_interval`.
    fn interval_for(&self, key: &str) -> Duration {
        self.intervals
            .get(key)
            .copied()
            .unwrap_or(self.default_interval)
    }

    /// Check whether the key can be acquired right now without actually acquiring it.
    pub fn can_acquire(&self, key: &str) -> bool {
        let interval = self.interval_for(key);
        match self.last_acquire.get(key) {
            Some(last) => last.elapsed() >= interval,
            None => true,
        }
    }

    /// Attempt to acquire the rate-limited key.
    ///
    /// Returns `Duration::ZERO` if the key was successfully acquired (i.e., the
    /// minimum interval has elapsed since the last acquire). Otherwise returns
    /// the remaining wait duration.
    ///
    /// On a successful acquire, the internal timestamp is updated.
    pub fn acquire(&mut self, key: &str) -> Duration {
        let interval = self.interval_for(key);
        let now = Instant::now();

        match self.last_acquire.get(key) {
            Some(last) => {
                let elapsed = now.duration_since(*last);
                if elapsed >= interval {
                    self.last_acquire.insert(key.to_string(), now);
                    Duration::ZERO
                } else {
                    interval - elapsed
                }
            }
            None => {
                self.last_acquire.insert(key.to_string(), now);
                Duration::ZERO
            }
        }
    }

    /// Return how long until the key can be acquired.
    ///
    /// Returns `Duration::ZERO` if the key is immediately available.
    pub fn time_until_ready(&self, key: &str) -> Duration {
        let interval = self.interval_for(key);
        match self.last_acquire.get(key) {
            Some(last) => {
                let elapsed = last.elapsed();
                if elapsed >= interval {
                    Duration::ZERO
                } else {
                    interval - elapsed
                }
            }
            None => Duration::ZERO,
        }
    }

    /// Reset the rate limit state for a key, allowing immediate acquire.
    pub fn reset(&mut self, key: &str) {
        self.last_acquire.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_first_acquire_returns_zero() {
        let mut limiter = RateLimiter::new(Duration::from_millis(100));
        assert_eq!(limiter.acquire("key1"), Duration::ZERO);
    }

    #[test]
    fn test_immediate_second_acquire_returns_wait() {
        let mut limiter = RateLimiter::new(Duration::from_millis(100));
        limiter.acquire("key1");
        let wait = limiter.acquire("key1");
        assert!(wait > Duration::ZERO);
        assert!(wait <= Duration::from_millis(100));
    }

    #[test]
    fn test_acquire_after_interval_returns_zero() {
        let mut limiter = RateLimiter::new(Duration::from_millis(10));
        limiter.acquire("key1");
        thread::sleep(Duration::from_millis(15));
        assert_eq!(limiter.acquire("key1"), Duration::ZERO);
    }

    #[test]
    fn test_per_key_isolation() {
        let mut limiter = RateLimiter::new(Duration::from_millis(100));
        limiter.acquire("key1");
        // key2 has never been acquired, so it should be immediately available.
        assert_eq!(limiter.acquire("key2"), Duration::ZERO);
    }

    #[test]
    fn test_can_acquire_without_side_effects() {
        let mut limiter = RateLimiter::new(Duration::from_millis(100));
        // Never acquired => available.
        assert!(limiter.can_acquire("key1"));

        limiter.acquire("key1");
        // Immediately after acquire => not available.
        assert!(!limiter.can_acquire("key1"));
    }

    #[test]
    fn test_set_interval_per_key() {
        let mut limiter = RateLimiter::new(Duration::from_millis(100));
        limiter.set_interval("slow", Duration::from_millis(500));

        limiter.acquire("slow");
        let wait = limiter.acquire("slow");
        // Should be close to 500ms, definitely > 100ms default.
        assert!(wait > Duration::from_millis(100));
    }

    #[test]
    fn test_time_until_ready_never_acquired() {
        let limiter = RateLimiter::new(Duration::from_millis(100));
        assert_eq!(limiter.time_until_ready("key1"), Duration::ZERO);
    }

    #[test]
    fn test_time_until_ready_after_acquire() {
        let mut limiter = RateLimiter::new(Duration::from_millis(100));
        limiter.acquire("key1");
        let remaining = limiter.time_until_ready("key1");
        assert!(remaining > Duration::ZERO);
        assert!(remaining <= Duration::from_millis(100));
    }

    #[test]
    fn test_reset_allows_immediate_acquire() {
        let mut limiter = RateLimiter::new(Duration::from_millis(100));
        limiter.acquire("key1");
        assert!(!limiter.can_acquire("key1"));

        limiter.reset("key1");
        assert!(limiter.can_acquire("key1"));
        assert_eq!(limiter.acquire("key1"), Duration::ZERO);
    }

    #[test]
    fn test_reset_nonexistent_key_is_noop() {
        let mut limiter = RateLimiter::new(Duration::from_millis(100));
        // Should not panic.
        limiter.reset("nonexistent");
    }

    #[test]
    fn test_default_interval_used_when_no_override() {
        let mut limiter = RateLimiter::new(Duration::from_millis(50));
        limiter.acquire("key1");
        let wait = limiter.acquire("key1");
        assert!(wait <= Duration::from_millis(50));
    }
}
