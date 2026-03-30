//! Retry policy with exponential backoff and jitter.
//!
//! Provides a configurable retry strategy that plugins can use when sending
//! messages or performing other fallible operations.

use std::time::Duration;

/// A retry policy with exponential backoff.
///
/// The delay for attempt `n` is:
/// ```text
/// min(initial_delay * multiplier^n, max_delay) + jitter
/// ```
///
/// Jitter is deterministic based on the attempt number to keep the
/// implementation pure (no RNG dependency). It adds up to 10% of the
/// calculated delay.
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl RetryPolicy {
    /// Create a policy with sensible defaults:
    /// 3 retries, 1 s initial delay, 30 s max delay, 2x multiplier.
    pub fn new() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 30_000,
            backoff_multiplier: 2.0,
        }
    }

    /// Builder: set maximum retry count.
    pub fn with_max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    /// Builder: set initial delay in milliseconds.
    pub fn with_initial_delay(mut self, ms: u64) -> Self {
        self.initial_delay_ms = ms;
        self
    }

    /// Builder: set maximum delay in milliseconds.
    pub fn with_max_delay(mut self, ms: u64) -> Self {
        self.max_delay_ms = ms;
        self
    }

    /// Builder: set backoff multiplier.
    pub fn with_backoff_multiplier(mut self, m: f64) -> Self {
        self.backoff_multiplier = m;
        self
    }

    /// Returns `true` if the given attempt number should be retried.
    ///
    /// Attempt numbering is 0-based: the first failure is attempt 0.
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_retries
    }

    /// Compute the delay duration for the given attempt.
    ///
    /// Includes deterministic jitter (up to 10% of the base delay).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let base = (self.initial_delay_ms as f64) * self.backoff_multiplier.powi(attempt as i32);
        let clamped = base.min(self.max_delay_ms as f64) as u64;

        // Deterministic jitter: up to 10% based on attempt number.
        let jitter_fraction = ((attempt as u64 * 7 + 3) % 10) as f64 / 100.0;
        let jitter = (clamped as f64 * jitter_fraction) as u64;

        Duration::from_millis(clamped + jitter)
    }

    /// Compute the absolute timestamp (in epoch ms) for the next retry.
    pub fn next_retry_at(&self, attempt: u32, now_ms: i64) -> i64 {
        let delay = self.delay_for_attempt(attempt);
        now_ms + delay.as_millis() as i64
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let policy = RetryPolicy::new();
        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.initial_delay_ms, 1000);
        assert_eq!(policy.max_delay_ms, 30_000);
        assert!((policy.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_builder_pattern() {
        let policy = RetryPolicy::new()
            .with_max_retries(5)
            .with_initial_delay(500)
            .with_max_delay(10_000)
            .with_backoff_multiplier(3.0);

        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.initial_delay_ms, 500);
        assert_eq!(policy.max_delay_ms, 10_000);
        assert!((policy.backoff_multiplier - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_should_retry_within_limit() {
        let policy = RetryPolicy::new().with_max_retries(3);
        assert!(policy.should_retry(0));
        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
    }

    #[test]
    fn test_should_retry_at_and_beyond_limit() {
        let policy = RetryPolicy::new().with_max_retries(3);
        assert!(!policy.should_retry(3));
        assert!(!policy.should_retry(4));
        assert!(!policy.should_retry(100));
    }

    #[test]
    fn test_should_retry_zero_retries() {
        let policy = RetryPolicy::new().with_max_retries(0);
        assert!(!policy.should_retry(0));
    }

    #[test]
    fn test_exponential_backoff_increases() {
        let policy = RetryPolicy::new()
            .with_initial_delay(1000)
            .with_max_delay(100_000)
            .with_backoff_multiplier(2.0);

        let d0 = policy.delay_for_attempt(0);
        let d1 = policy.delay_for_attempt(1);
        let d2 = policy.delay_for_attempt(2);

        // Each delay should be roughly 2x the previous (plus small jitter).
        assert!(d1 > d0);
        assert!(d2 > d1);
    }

    #[test]
    fn test_delay_clamped_at_max() {
        let policy = RetryPolicy::new()
            .with_initial_delay(10_000)
            .with_max_delay(15_000)
            .with_backoff_multiplier(10.0);

        // Attempt 1 would be 10_000 * 10 = 100_000, clamped to 15_000.
        let delay = policy.delay_for_attempt(1);
        // With up to 10% jitter, max is 16_500.
        assert!(delay.as_millis() <= 16_500);
        assert!(delay.as_millis() >= 15_000);
    }

    #[test]
    fn test_delay_for_attempt_zero_is_initial() {
        let policy = RetryPolicy::new()
            .with_initial_delay(1000)
            .with_backoff_multiplier(2.0);

        let delay = policy.delay_for_attempt(0);
        // Base is 1000ms, jitter adds up to 10% => [1000, 1100].
        assert!(delay.as_millis() >= 1000);
        assert!(delay.as_millis() <= 1100);
    }

    #[test]
    fn test_next_retry_at() {
        let policy = RetryPolicy::new().with_initial_delay(1000);
        let now = 50_000_i64;
        let next = policy.next_retry_at(0, now);
        // Should be now + ~1000ms (with jitter).
        assert!(next >= now + 1000);
        assert!(next <= now + 1100);
    }

    #[test]
    fn test_deterministic_jitter() {
        let policy = RetryPolicy::new().with_initial_delay(1000);
        let d1 = policy.delay_for_attempt(2);
        let d2 = policy.delay_for_attempt(2);
        assert_eq!(d1, d2, "jitter must be deterministic for the same attempt");
    }

    #[test]
    fn test_default_trait() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 3);
    }
}
