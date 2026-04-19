//! Prometheus-compatible metrics for pg_ripple_http.
//!
//! Tracks SPARQL queries, Datalog queries, errors, and cumulative duration.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub struct Metrics {
    /// Total SPARQL queries executed.
    sparql_queries: AtomicU64,
    /// Total Datalog API calls executed.
    datalog_queries: AtomicU64,
    errors: AtomicU64,
    total_duration_us: AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            sparql_queries: AtomicU64::new(0),
            datalog_queries: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            total_duration_us: AtomicU64::new(0),
        }
    }

    pub fn record_query(&self, duration: Duration) {
        self.sparql_queries.fetch_add(1, Ordering::Relaxed);
        self.total_duration_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn record_datalog_query(&self, duration: Duration) {
        self.datalog_queries.fetch_add(1, Ordering::Relaxed);
        self.total_duration_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn sparql_query_count(&self) -> u64 {
        self.sparql_queries.load(Ordering::Relaxed)
    }

    pub fn datalog_query_count(&self) -> u64 {
        self.datalog_queries.load(Ordering::Relaxed)
    }

    /// Kept for backward compatibility with the `/metrics` endpoint formatter.
    pub fn query_count(&self) -> u64 {
        self.sparql_queries.load(Ordering::Relaxed)
    }

    pub fn error_count(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    pub fn total_duration_secs(&self) -> f64 {
        self.total_duration_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }
}
