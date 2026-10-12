//! Tracks `X-RateLimit-*` headers, waits proactively when the budget is spent,
//! and bounds concurrency for bulk work. Shared via `Arc` so parallel callers
//! (bulk CLI commands, concurrent MCP tool calls) don't stampede.

use std::{
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::header::HeaderMap;
use tokio::sync::{Semaphore, SemaphorePermit};

pub const DEFAULT_CONCURRENCY: usize = 4;
/// Never sleep longer than this for one rate-limit wait.
const MAX_WAIT: Duration = Duration::from_secs(65);

#[derive(Debug, Default, Clone, Copy)]
struct State {
    remaining: Option<u32>,
    reset_at: Option<i64>,
}

#[derive(Debug)]
pub struct RateLimiter {
    state: Mutex<State>,
    permits: Semaphore,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(DEFAULT_CONCURRENCY)
    }
}

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// 250..750ms, without pulling in a RNG crate.
fn jitter() -> Duration {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    Duration::from_millis(250 + u64::from(nanos % 500))
}

fn header_i64(headers: &HeaderMap, name: &str) -> Option<i64> {
    headers
        .get(name)?
        .to_str()
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()
        .map(|v| v as i64)
}

impl RateLimiter {
    pub fn new(concurrency: usize) -> Self {
        Self {
            state: Mutex::new(State::default()),
            permits: Semaphore::new(concurrency.max(1)),
        }
    }

    /// Bounded-concurrency permit for bulk operations.
    pub async fn permit(&self) -> SemaphorePermit<'_> {
        self.permits
            .acquire()
            .await
            .expect("rate limiter semaphore is never closed")
    }

    /// Sleep if the last response said the budget is exhausted.
    pub async fn wait(&self) {
        let state = *self.state.lock().expect("rate limiter lock");
        if state.remaining == Some(0) {
            if let Some(reset) = state.reset_at {
                let secs = reset - now_secs();
                if secs > 0 {
                    let wait = (Duration::from_secs(secs as u64) + jitter()).min(MAX_WAIT);
                    tracing::debug!(?wait, "rate limit budget spent; waiting for reset");
                    tokio::time::sleep(wait).await;
                }
            }
        }
    }

    pub fn observe(&self, headers: &HeaderMap) {
        let remaining = header_i64(headers, "x-ratelimit-remaining");
        let reset = header_i64(headers, "x-ratelimit-reset");
        if remaining.is_none() && reset.is_none() {
            return;
        }
        let mut state = self.state.lock().expect("rate limiter lock");
        if let Some(r) = remaining {
            state.remaining = Some(r.max(0) as u32);
        }
        if reset.is_some() {
            state.reset_at = reset;
        }
    }

    pub fn remaining(&self) -> Option<u32> {
        self.state.lock().expect("rate limiter lock").remaining
    }
}

/// How long to wait after a 429, plus the reset timestamp to report.
/// Honors `Retry-After` (seconds) if ClickUp ever sends it.
pub fn retry_delay(headers: &HeaderMap) -> (Duration, i64) {
    let now = now_secs();
    if let Some(after) = header_i64(headers, "retry-after") {
        let after = after.max(0);
        return (
            (Duration::from_secs(after as u64) + jitter()).min(MAX_WAIT),
            now + after,
        );
    }
    let reset = header_i64(headers, "x-ratelimit-reset").unwrap_or(now);
    let secs = (reset - now).max(0) as u64;
    ((Duration::from_secs(secs) + jitter()).min(MAX_WAIT), reset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    fn headers(pairs: &[(&'static str, String)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn observe_tracks_headers() {
        let l = RateLimiter::default();
        l.observe(&headers(&[
            ("x-ratelimit-remaining", "7".into()),
            ("x-ratelimit-reset", "1700000000".into()),
        ]));
        assert_eq!(l.remaining(), Some(7));
        l.observe(&HeaderMap::new());
        assert_eq!(l.remaining(), Some(7));
    }

    #[test]
    fn retry_delay_uses_reset_and_retry_after() {
        let (d, reset) = retry_delay(&headers(&[(
            "x-ratelimit-reset",
            (now_secs() + 2).to_string(),
        )]));
        assert!(d >= Duration::from_millis(1250) && d <= Duration::from_millis(2750));
        assert!(reset > now_secs());

        let (d, _) = retry_delay(&headers(&[("retry-after", "1".into())]));
        assert!(d >= Duration::from_millis(1250) && d < Duration::from_secs(2));

        let (d, _) = retry_delay(&HeaderMap::new());
        assert!(d < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn wait_is_noop_with_budget_left() {
        let l = RateLimiter::default();
        l.observe(&headers(&[
            ("x-ratelimit-remaining", "0".into()),
            ("x-ratelimit-reset", "0".into()),
        ]));
        let start = std::time::Instant::now();
        l.wait().await;
        assert!(start.elapsed() < Duration::from_millis(100));
    }
}
