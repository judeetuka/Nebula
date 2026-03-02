use std::time::Duration;

/// Centralized timeout management for task execution.
///
/// Clamps requested timeouts to a configurable [min, max] range and provides
/// utility methods for checking whether a task has timed out.
pub struct TimeoutManager {
    default_timeout: Duration,
    min_timeout: Duration,
    max_timeout: Duration,
}

impl TimeoutManager {
    /// Create a new timeout manager with the given bounds.
    pub fn new(default: Duration, min: Duration, max: Duration) -> Self {
        Self {
            default_timeout: default,
            min_timeout: min,
            max_timeout: max,
        }
    }

    /// Returns the default timeout.
    pub fn default_timeout(&self) -> Duration {
        self.default_timeout
    }

    /// Returns the minimum allowed timeout.
    pub fn min_timeout(&self) -> Duration {
        self.min_timeout
    }

    /// Returns the maximum allowed timeout.
    pub fn max_timeout(&self) -> Duration {
        self.max_timeout
    }

    /// Clamp the requested timeout (in seconds) to the [min, max] range.
    ///
    /// If `requested_secs` is 0, the default timeout is returned.
    pub fn effective_timeout(&self, requested_secs: u32) -> Duration {
        if requested_secs == 0 {
            return self.default_timeout;
        }

        let requested = Duration::from_secs(u64::from(requested_secs));

        if requested < self.min_timeout {
            self.min_timeout
        } else if requested > self.max_timeout {
            self.max_timeout
        } else {
            requested
        }
    }

    /// Check if a task has exceeded its timeout.
    ///
    /// `started_at` is in Unix milliseconds. `timeout_secs` is the requested
    /// timeout which will be clamped to the allowed range.
    pub fn is_timed_out(&self, started_at: i64, timeout_secs: u32) -> bool {
        let now_millis = chrono::Utc::now().timestamp_millis();
        let elapsed_millis = now_millis - started_at;

        if elapsed_millis < 0 {
            return false;
        }

        let effective = self.effective_timeout(timeout_secs);
        let effective_millis = effective.as_millis() as i64;

        elapsed_millis > effective_millis
    }

    /// Returns the time remaining before a task times out, or `None` if
    /// already timed out.
    ///
    /// `started_at` is in Unix milliseconds.
    pub fn time_remaining(&self, started_at: i64, timeout_secs: u32) -> Option<Duration> {
        let now_millis = chrono::Utc::now().timestamp_millis();
        let elapsed_millis = now_millis - started_at;

        if elapsed_millis < 0 {
            // Clock skew: treat as full time remaining
            return Some(self.effective_timeout(timeout_secs));
        }

        let effective = self.effective_timeout(timeout_secs);
        let effective_millis = effective.as_millis() as i64;
        let remaining_millis = effective_millis - elapsed_millis;

        if remaining_millis <= 0 {
            None
        } else {
            Some(Duration::from_millis(remaining_millis as u64))
        }
    }
}

impl Default for TimeoutManager {
    /// Default timeout manager: 90s default, 60s min, 120s max.
    fn default() -> Self {
        Self::new(
            Duration::from_secs(90),
            Duration::from_secs(60),
            Duration::from_secs(120),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_manager() -> TimeoutManager {
        TimeoutManager::default()
    }

    // -----------------------------------------------------------------------
    // Default construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_values() {
        let tm = default_manager();
        assert_eq!(tm.default_timeout(), Duration::from_secs(90));
        assert_eq!(tm.min_timeout(), Duration::from_secs(60));
        assert_eq!(tm.max_timeout(), Duration::from_secs(120));
    }

    #[test]
    fn test_custom_values() {
        let tm = TimeoutManager::new(
            Duration::from_secs(45),
            Duration::from_secs(30),
            Duration::from_secs(180),
        );
        assert_eq!(tm.default_timeout(), Duration::from_secs(45));
        assert_eq!(tm.min_timeout(), Duration::from_secs(30));
        assert_eq!(tm.max_timeout(), Duration::from_secs(180));
    }

    // -----------------------------------------------------------------------
    // effective_timeout
    // -----------------------------------------------------------------------

    #[test]
    fn test_effective_timeout_zero_returns_default() {
        let tm = default_manager();
        assert_eq!(tm.effective_timeout(0), Duration::from_secs(90));
    }

    #[test]
    fn test_effective_timeout_within_range() {
        let tm = default_manager();
        assert_eq!(tm.effective_timeout(90), Duration::from_secs(90));
        assert_eq!(tm.effective_timeout(60), Duration::from_secs(60));
        assert_eq!(tm.effective_timeout(120), Duration::from_secs(120));
        assert_eq!(tm.effective_timeout(75), Duration::from_secs(75));
    }

    #[test]
    fn test_effective_timeout_below_min_clamped() {
        let tm = default_manager();
        assert_eq!(tm.effective_timeout(10), Duration::from_secs(60));
        assert_eq!(tm.effective_timeout(59), Duration::from_secs(60));
        assert_eq!(tm.effective_timeout(1), Duration::from_secs(60));
    }

    #[test]
    fn test_effective_timeout_above_max_clamped() {
        let tm = default_manager();
        assert_eq!(tm.effective_timeout(121), Duration::from_secs(120));
        assert_eq!(tm.effective_timeout(300), Duration::from_secs(120));
        assert_eq!(tm.effective_timeout(u32::MAX), Duration::from_secs(120));
    }

    // -----------------------------------------------------------------------
    // is_timed_out
    // -----------------------------------------------------------------------

    #[test]
    fn test_not_timed_out_recently_started() {
        let tm = default_manager();
        let now = chrono::Utc::now().timestamp_millis();
        assert!(!tm.is_timed_out(now, 90));
    }

    #[test]
    fn test_timed_out_long_ago() {
        let tm = default_manager();
        // Started 200 seconds ago, timeout is 90s (clamped to 90s)
        let started_at = chrono::Utc::now().timestamp_millis() - 200_000;
        assert!(tm.is_timed_out(started_at, 90));
    }

    #[test]
    fn test_not_timed_out_future_start() {
        let tm = default_manager();
        // Started in the future (clock skew)
        let started_at = chrono::Utc::now().timestamp_millis() + 60_000;
        assert!(!tm.is_timed_out(started_at, 90));
    }

    #[test]
    fn test_timed_out_with_small_timeout_clamped_to_min() {
        let tm = default_manager();
        // Requested 10s timeout, clamped to 60s min. Started 70s ago.
        let started_at = chrono::Utc::now().timestamp_millis() - 70_000;
        assert!(tm.is_timed_out(started_at, 10));
    }

    #[test]
    fn test_not_timed_out_with_small_timeout_clamped_to_min() {
        let tm = default_manager();
        // Requested 10s timeout, clamped to 60s min. Started 30s ago.
        let started_at = chrono::Utc::now().timestamp_millis() - 30_000;
        assert!(!tm.is_timed_out(started_at, 10));
    }

    // -----------------------------------------------------------------------
    // time_remaining
    // -----------------------------------------------------------------------

    #[test]
    fn test_time_remaining_recently_started() {
        let tm = default_manager();
        let now = chrono::Utc::now().timestamp_millis();
        let remaining = tm.time_remaining(now, 90);

        assert!(remaining.is_some());
        // Should be close to 90 seconds
        let secs = remaining.unwrap().as_secs();
        assert!(secs >= 85 && secs <= 90);
    }

    #[test]
    fn test_time_remaining_none_when_expired() {
        let tm = default_manager();
        let started_at = chrono::Utc::now().timestamp_millis() - 200_000;
        assert!(tm.time_remaining(started_at, 90).is_none());
    }

    #[test]
    fn test_time_remaining_with_future_start() {
        let tm = default_manager();
        let started_at = chrono::Utc::now().timestamp_millis() + 60_000;
        let remaining = tm.time_remaining(started_at, 90);

        assert!(remaining.is_some());
        assert_eq!(remaining.unwrap(), Duration::from_secs(90));
    }

    #[test]
    fn test_time_remaining_partially_elapsed() {
        let tm = default_manager();
        // Started 30 seconds ago, timeout is 90s
        let started_at = chrono::Utc::now().timestamp_millis() - 30_000;
        let remaining = tm.time_remaining(started_at, 90);

        assert!(remaining.is_some());
        let secs = remaining.unwrap().as_secs();
        // Should be approximately 60 seconds remaining
        assert!(secs >= 55 && secs <= 60);
    }
}
