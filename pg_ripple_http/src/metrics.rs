//! Simple Prometheus-compatible metrics for pg_ripple_http.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub struct Metrics {
    queries: AtomicU64,
    errors: AtomicU64,
    total_duration_us: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            queries: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            total_duration_us: AtomicU64::new(0),
        }
    }

    pub fn record_query(&self, duration: Duration) {
        self.queries.fetch_add(1, Ordering::Relaxed);
        self.total_duration_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn query_count(&self) -> u64 {
        self.queries.load(Ordering::Relaxed)
    }

    pub fn error_count(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    pub fn total_duration_secs(&self) -> f64 {
        self.total_duration_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }
}
