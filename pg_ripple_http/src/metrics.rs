//! Prometheus-compatible metrics for pg_ripple_http.
//!
//! Tracks SPARQL queries, Datalog queries, errors, and cumulative duration.
//! v0.67.0 FLIGHT-SEC-02: added Arrow Flight batch and rejection counters.
//! v0.82.0 METRICS-LABELS-01: added query_type and result_size_bucket dimensions.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Result-size buckets for the `result_size_bucket` Prometheus label.
#[derive(Clone, Copy)]
pub enum ResultSizeBucket {
    /// 0 rows
    Empty,
    /// 1–99 rows
    Small,
    /// 100–9 999 rows
    Medium,
    /// 10 000+ rows
    Large,
}

impl ResultSizeBucket {
    pub fn from_count(n: usize) -> Self {
        match n {
            0 => Self::Empty,
            1..=99 => Self::Small,
            100..=9_999 => Self::Medium,
            _ => Self::Large,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }
}

pub struct Metrics {
    /// Total SPARQL queries executed.
    sparql_queries: AtomicU64,
    /// Total Datalog API calls executed.
    datalog_queries: AtomicU64,
    errors: AtomicU64,
    total_duration_us: AtomicU64,
    /// Unix timestamp (seconds) of the last query, or 0 if no query yet.
    last_query_ts: AtomicU64,
    /// v0.67.0 FLIGHT-SEC-02: total Arrow record batches sent.
    arrow_batches_sent: AtomicU64,
    /// v0.67.0 FLIGHT-SEC-01: total Arrow ticket rejections.
    arrow_ticket_rejections: AtomicU64,

    // METRICS-LABELS-01 (v0.82.0): per-query-type counters and durations.
    select_count: AtomicU64,
    ask_count: AtomicU64,
    construct_count: AtomicU64,
    describe_count: AtomicU64,
    update_count: AtomicU64,
    select_duration_us: AtomicU64,
    ask_duration_us: AtomicU64,
    construct_duration_us: AtomicU64,
    describe_duration_us: AtomicU64,
    update_duration_us: AtomicU64,

    // METRICS-LABELS-01: result-size-bucket counters.
    result_empty: AtomicU64,
    result_small: AtomicU64,
    result_medium: AtomicU64,
    result_large: AtomicU64,

    // P13-08 (v0.85.0): dictionary hot-cache counters.
    // Populated by querying pg_ripple.dictionary_cache_stats() in the extension.
    /// Cumulative dictionary backend-local LRU cache hits.
    dictionary_hot_cache_hits: AtomicU64,
    /// Cumulative dictionary backend-local LRU cache misses.
    dictionary_hot_cache_misses: AtomicU64,

    // O13-02 (v0.86.0): new observability counters.
    /// Total federation endpoint request count (used to compute per-endpoint latency).
    federation_endpoint_requests: AtomicU64,
    /// Cumulative federation endpoint latency in microseconds.
    federation_endpoint_duration_us: AtomicU64,
    /// Snapshot of dictionary_cache_hit_ratio * 1e6 (stored as integer for atomic ops).
    dictionary_cache_hit_ratio_ppm: AtomicU64,
    /// Merge worker delta rows pending (snapshot from extension monitoring table).
    merge_worker_delta_rows_pending: AtomicU64,

    // S13-03 (v0.86.0): CORS permissive-origin request counter.
    /// Requests served under the CORS wildcard-origin (*) policy.
    cors_permissive_requests_total: AtomicU64,
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
            last_query_ts: AtomicU64::new(0),
            arrow_batches_sent: AtomicU64::new(0),
            arrow_ticket_rejections: AtomicU64::new(0),
            select_count: AtomicU64::new(0),
            ask_count: AtomicU64::new(0),
            construct_count: AtomicU64::new(0),
            describe_count: AtomicU64::new(0),
            update_count: AtomicU64::new(0),
            select_duration_us: AtomicU64::new(0),
            ask_duration_us: AtomicU64::new(0),
            construct_duration_us: AtomicU64::new(0),
            describe_duration_us: AtomicU64::new(0),
            update_duration_us: AtomicU64::new(0),
            result_empty: AtomicU64::new(0),
            result_small: AtomicU64::new(0),
            result_medium: AtomicU64::new(0),
            result_large: AtomicU64::new(0),
            dictionary_hot_cache_hits: AtomicU64::new(0),
            dictionary_hot_cache_misses: AtomicU64::new(0),
            federation_endpoint_requests: AtomicU64::new(0),
            federation_endpoint_duration_us: AtomicU64::new(0),
            dictionary_cache_hit_ratio_ppm: AtomicU64::new(0),
            merge_worker_delta_rows_pending: AtomicU64::new(0),
            cors_permissive_requests_total: AtomicU64::new(0),
        }
    }

    pub fn record_query(&self, duration: Duration) {
        self.sparql_queries.fetch_add(1, Ordering::Relaxed);
        self.total_duration_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
        self.last_query_ts.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            Ordering::Relaxed,
        );
    }

    /// METRICS-LABELS-01 (v0.82.0): record query with query_type label and row count.
    pub fn record_query_typed(&self, duration: Duration, query_type: &str, row_count: usize) {
        self.record_query(duration);
        let dur_us = duration.as_micros() as u64;
        match query_type {
            "SELECT" => {
                self.select_count.fetch_add(1, Ordering::Relaxed);
                self.select_duration_us.fetch_add(dur_us, Ordering::Relaxed);
            }
            "ASK" => {
                self.ask_count.fetch_add(1, Ordering::Relaxed);
                self.ask_duration_us.fetch_add(dur_us, Ordering::Relaxed);
            }
            "CONSTRUCT" => {
                self.construct_count.fetch_add(1, Ordering::Relaxed);
                self.construct_duration_us
                    .fetch_add(dur_us, Ordering::Relaxed);
            }
            "DESCRIBE" => {
                self.describe_count.fetch_add(1, Ordering::Relaxed);
                self.describe_duration_us
                    .fetch_add(dur_us, Ordering::Relaxed);
            }
            "UPDATE" => {
                self.update_count.fetch_add(1, Ordering::Relaxed);
                self.update_duration_us.fetch_add(dur_us, Ordering::Relaxed);
            }
            _ => {}
        }
        match ResultSizeBucket::from_count(row_count) {
            ResultSizeBucket::Empty => self.result_empty.fetch_add(1, Ordering::Relaxed),
            ResultSizeBucket::Small => self.result_small.fetch_add(1, Ordering::Relaxed),
            ResultSizeBucket::Medium => self.result_medium.fetch_add(1, Ordering::Relaxed),
            ResultSizeBucket::Large => self.result_large.fetch_add(1, Ordering::Relaxed),
        };
    }

    pub fn record_datalog_query(&self, duration: Duration) {
        self.datalog_queries.fetch_add(1, Ordering::Relaxed);
        self.total_duration_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Record Arrow record batches sent (v0.67.0 FLIGHT-SEC-02).
    pub fn record_arrow_batches_sent(&self, n: u64) {
        self.arrow_batches_sent.fetch_add(n, Ordering::Relaxed);
    }

    /// Record an Arrow ticket rejection (v0.67.0 FLIGHT-SEC-01).
    pub fn record_arrow_ticket_rejection(&self) {
        self.arrow_ticket_rejections.fetch_add(1, Ordering::Relaxed);
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

    /// Unix timestamp (seconds) of the last query, or 0 if no query has been made.
    pub fn last_query_ts(&self) -> u64 {
        self.last_query_ts.load(Ordering::Relaxed)
    }

    pub fn arrow_batches_sent(&self) -> u64 {
        self.arrow_batches_sent.load(Ordering::Relaxed)
    }

    pub fn arrow_ticket_rejections(&self) -> u64 {
        self.arrow_ticket_rejections.load(Ordering::Relaxed)
    }

    // METRICS-LABELS-01: accessors for query_type and result_size_bucket labels.

    pub fn select_count(&self) -> u64 {
        self.select_count.load(Ordering::Relaxed)
    }
    pub fn ask_count(&self) -> u64 {
        self.ask_count.load(Ordering::Relaxed)
    }
    pub fn construct_count(&self) -> u64 {
        self.construct_count.load(Ordering::Relaxed)
    }
    pub fn describe_count(&self) -> u64 {
        self.describe_count.load(Ordering::Relaxed)
    }
    pub fn update_count(&self) -> u64 {
        self.update_count.load(Ordering::Relaxed)
    }
    pub fn select_duration_secs(&self) -> f64 {
        self.select_duration_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }
    pub fn ask_duration_secs(&self) -> f64 {
        self.ask_duration_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }
    pub fn construct_duration_secs(&self) -> f64 {
        self.construct_duration_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }
    pub fn describe_duration_secs(&self) -> f64 {
        self.describe_duration_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }
    pub fn update_duration_secs(&self) -> f64 {
        self.update_duration_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }
    pub fn result_empty_count(&self) -> u64 {
        self.result_empty.load(Ordering::Relaxed)
    }
    pub fn result_small_count(&self) -> u64 {
        self.result_small.load(Ordering::Relaxed)
    }
    pub fn result_medium_count(&self) -> u64 {
        self.result_medium.load(Ordering::Relaxed)
    }
    pub fn result_large_count(&self) -> u64 {
        self.result_large.load(Ordering::Relaxed)
    }

    // P13-08 (v0.85.0): dictionary hot-cache accessors and updaters.

    pub fn dictionary_hot_cache_hits(&self) -> u64 {
        self.dictionary_hot_cache_hits.load(Ordering::Relaxed)
    }
    pub fn dictionary_hot_cache_misses(&self) -> u64 {
        self.dictionary_hot_cache_misses.load(Ordering::Relaxed)
    }

    /// Update the dictionary hot-cache counters from values queried from the extension.
    pub fn update_dictionary_cache_stats(&self, hits: u64, misses: u64) {
        self.dictionary_hot_cache_hits
            .store(hits, Ordering::Relaxed);
        self.dictionary_hot_cache_misses
            .store(misses, Ordering::Relaxed);
        // Update the hit-ratio snapshot (parts-per-million).
        let total = hits + misses;
        let ppm = total
            .checked_div(1)
            .map(|_| hits * 1_000_000 / total)
            .unwrap_or(0);
        self.dictionary_cache_hit_ratio_ppm
            .store(ppm, Ordering::Relaxed);
    }

    // O13-02 (v0.86.0): federation endpoint metrics.

    /// Record a completed federation SERVICE call.
    pub fn record_federation_request(&self, duration: std::time::Duration) {
        self.federation_endpoint_requests
            .fetch_add(1, Ordering::Relaxed);
        self.federation_endpoint_duration_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn federation_endpoint_requests(&self) -> u64 {
        self.federation_endpoint_requests.load(Ordering::Relaxed)
    }

    pub fn federation_endpoint_duration_secs(&self) -> f64 {
        self.federation_endpoint_duration_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    /// Dictionary cache hit ratio (0.0–1.0) derived from the hot-cache counters.
    pub fn dictionary_cache_hit_ratio(&self) -> f64 {
        self.dictionary_cache_hit_ratio_ppm.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    /// Update the merge worker delta rows pending snapshot.
    pub fn update_merge_worker_delta_rows_pending(&self, rows: u64) {
        self.merge_worker_delta_rows_pending
            .store(rows, Ordering::Relaxed);
    }

    pub fn merge_worker_delta_rows_pending(&self) -> u64 {
        self.merge_worker_delta_rows_pending.load(Ordering::Relaxed)
    }

    // S13-03 (v0.86.0): CORS permissive counter.

    /// Increment the CORS permissive-origin request counter.
    pub fn record_cors_permissive_request(&self) {
        self.cors_permissive_requests_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn cors_permissive_requests_total(&self) -> u64 {
        self.cors_permissive_requests_total.load(Ordering::Relaxed)
    }
}
