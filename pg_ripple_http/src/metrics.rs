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

    // OBS-01 (v0.91.0): PageRank IVM queue gauges.
    /// Snapshot: number of dirty edges queued for incremental PageRank refresh (default topic).
    pagerank_queue_depth: AtomicU64,
    /// Snapshot: largest accumulated score delta in the dirty-edges queue, stored as f64 bits.
    pagerank_queue_max_delta_bits: AtomicU64,
    /// Snapshot: age in seconds of the oldest entry in the dirty-edges queue.
    pagerank_queue_oldest_enqueue_seconds: AtomicU64,

    // H15-03 (v0.94.0): bidi relay bounded channel counter.
    /// Total bidi relay dispatch calls dropped due to inflight overflow.
    bidi_relay_dropped_total: AtomicU64,

    // L18-02 (v0.129.0): JSON writeback drain-queue status-update failures.
    /// Total drain_json_writeback_queue() status-update failures.
    json_writeback_drain_errors_total: AtomicU64,

    // M15-19 (v0.96.0): four missing Prometheus metrics.
    /// Cumulative merge cycle wall-clock time in microseconds.
    merge_cycle_duration_us: AtomicU64,
    /// Cumulative Datalog stratum execution time in microseconds.
    datalog_stratum_duration_us: AtomicU64,
    /// Snapshot: SHACL async validation queue depth (number of pending validations).
    shacl_validation_queue_depth: AtomicU64,
    /// Snapshot: CDC replication slot lag in bytes (from pg_replication_slots).
    cdc_replication_slot_lag_bytes: AtomicU64,

    // M16-03 (v0.115.0): new subsystem Prometheus metrics.
    /// Cumulative ER stage latency in microseconds, stored per stage label index:
    /// 0=blocking, 1=embedding, 2=shacl, 3=canonicalization, 4=provenance.
    er_stage_duration_us: [AtomicU64; 5],
    /// Total owl:sameAs assertions made by the entity-resolution pipeline.
    sameas_assertions_total: AtomicU64,
    /// Cumulative Bayesian propagation latency in microseconds.
    bayesian_propagation_duration_us: AtomicU64,
    /// Snapshot: total temporal facts in the temporal_facts table.
    temporal_facts_total: AtomicU64,
    /// Total temporal fact queries.
    temporal_queries_total: AtomicU64,
    /// Total PPRL Bloom-filter encodes.
    pprl_bloom_encodes_total: AtomicU64,
    /// Total LLM explanation cache hits.
    llm_cache_hits_total: AtomicU64,
    /// Total LLM explanation cache misses.
    llm_cache_misses_total: AtomicU64,
    /// Cumulative proof-tree generation latency in microseconds.
    proof_tree_duration_us: AtomicU64,
    /// Total conflict detections raised by the rule conflict detector.
    conflict_detections_total: AtomicU64,

    // OBS-M-01 (v0.123.0): read-replica connection pool gauges.
    /// Snapshot: total size of the read-replica connection pool.
    replica_pool_size: AtomicU64,
    /// Snapshot: available (idle) connections in the read-replica pool.
    replica_pool_available: AtomicU64,

    // OBS-M-02 (v0.123.0): rule-library federation observability.
    /// Cumulative latency of `GET /rule-libraries/{name}/stream` responses in microseconds.
    rule_library_stream_duration_us: AtomicU64,
    /// Total errors returned by `POST /rule-libraries/{name}/subscribe`.
    rule_library_subscribe_errors_total: AtomicU64,

    // FEAT-02 (v0.125.0): temporal graph snapshot gauge.
    /// Snapshot: current number of registered graph snapshots.
    graph_snapshots_total: AtomicU64,
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
            pagerank_queue_depth: AtomicU64::new(0),
            pagerank_queue_max_delta_bits: AtomicU64::new(0),
            pagerank_queue_oldest_enqueue_seconds: AtomicU64::new(0),
            bidi_relay_dropped_total: AtomicU64::new(0),
            json_writeback_drain_errors_total: AtomicU64::new(0),
            merge_cycle_duration_us: AtomicU64::new(0),
            datalog_stratum_duration_us: AtomicU64::new(0),
            shacl_validation_queue_depth: AtomicU64::new(0),
            cdc_replication_slot_lag_bytes: AtomicU64::new(0),
            // M16-03 (v0.115.0)
            er_stage_duration_us: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            sameas_assertions_total: AtomicU64::new(0),
            bayesian_propagation_duration_us: AtomicU64::new(0),
            temporal_facts_total: AtomicU64::new(0),
            temporal_queries_total: AtomicU64::new(0),
            pprl_bloom_encodes_total: AtomicU64::new(0),
            llm_cache_hits_total: AtomicU64::new(0),
            llm_cache_misses_total: AtomicU64::new(0),
            proof_tree_duration_us: AtomicU64::new(0),
            conflict_detections_total: AtomicU64::new(0),
            // OBS-M-01 (v0.123.0)
            replica_pool_size: AtomicU64::new(0),
            replica_pool_available: AtomicU64::new(0),
            // OBS-M-02 (v0.123.0)
            rule_library_stream_duration_us: AtomicU64::new(0),
            rule_library_subscribe_errors_total: AtomicU64::new(0),
            // FEAT-02 (v0.125.0)
            graph_snapshots_total: AtomicU64::new(0),
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

    // OBS-01 (v0.91.0): PageRank IVM queue gauge updaters and accessors.

    /// Update the PageRank IVM queue gauge snapshot.
    ///
    /// Called by the metrics scrape handler after querying
    /// `pg_ripple.pagerank_queue_stats()` from the extension.
    pub fn update_pagerank_queue_stats(&self, depth: u64, max_delta: f64, oldest_seconds: f64) {
        self.pagerank_queue_depth.store(depth, Ordering::Relaxed);
        self.pagerank_queue_max_delta_bits
            .store(max_delta.to_bits(), Ordering::Relaxed);
        // Clamp to 0 to avoid NaN/negative storing oddities.
        let oldest_secs = if oldest_seconds.is_finite() && oldest_seconds >= 0.0 {
            oldest_seconds as u64
        } else {
            0
        };
        self.pagerank_queue_oldest_enqueue_seconds
            .store(oldest_secs, Ordering::Relaxed);
    }

    pub fn pagerank_queue_depth(&self) -> u64 {
        self.pagerank_queue_depth.load(Ordering::Relaxed)
    }

    pub fn pagerank_queue_max_delta(&self) -> f64 {
        f64::from_bits(self.pagerank_queue_max_delta_bits.load(Ordering::Relaxed))
    }

    pub fn pagerank_queue_oldest_enqueue_seconds(&self) -> u64 {
        self.pagerank_queue_oldest_enqueue_seconds
            .load(Ordering::Relaxed)
    }

    // H15-03 (v0.94.0): bidi relay dropped counter.

    /// Refresh the bidi relay dropped counter from the extension's streaming_metrics().
    pub fn update_bidi_relay_dropped_total(&self, dropped: u64) {
        self.bidi_relay_dropped_total
            .store(dropped, Ordering::Relaxed);
    }

    /// Return the total number of bidi relay calls dropped due to inflight overflow.
    pub fn bidi_relay_dropped_total(&self) -> u64 {
        self.bidi_relay_dropped_total.load(Ordering::Relaxed)
    }

    // L18-02 (v0.129.0): JSON writeback drain-queue status-update failures.

    /// Refresh the JSON writeback drain-error counter from the extension's streaming_metrics().
    pub fn update_json_writeback_drain_errors_total(&self, errors: u64) {
        self.json_writeback_drain_errors_total
            .store(errors, Ordering::Relaxed);
    }

    /// Return the total number of drain_json_writeback_queue() status-update failures.
    pub fn json_writeback_drain_errors_total(&self) -> u64 {
        self.json_writeback_drain_errors_total
            .load(Ordering::Relaxed)
    }

    // M15-19 (v0.96.0): four new Prometheus metrics.

    /// Record a completed merge cycle with its wall-clock duration.
    pub fn record_merge_cycle_duration(&self, duration: std::time::Duration) {
        self.merge_cycle_duration_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    /// Cumulative merge cycle duration in seconds.
    pub fn merge_cycle_duration_secs(&self) -> f64 {
        self.merge_cycle_duration_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    /// Record a completed Datalog stratum execution with its wall-clock duration.
    pub fn record_datalog_stratum_duration(&self, duration: std::time::Duration) {
        self.datalog_stratum_duration_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    /// Cumulative Datalog stratum execution duration in seconds.
    pub fn datalog_stratum_duration_secs(&self) -> f64 {
        self.datalog_stratum_duration_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    /// Update the SHACL validation queue depth snapshot.
    pub fn update_shacl_validation_queue_depth(&self, depth: u64) {
        self.shacl_validation_queue_depth
            .store(depth, Ordering::Relaxed);
    }

    /// Current SHACL validation queue depth.
    pub fn shacl_validation_queue_depth(&self) -> u64 {
        self.shacl_validation_queue_depth.load(Ordering::Relaxed)
    }

    /// Update the CDC replication slot lag bytes snapshot.
    pub fn update_cdc_replication_slot_lag_bytes(&self, lag_bytes: u64) {
        self.cdc_replication_slot_lag_bytes
            .store(lag_bytes, Ordering::Relaxed);
    }

    /// CDC replication slot lag in bytes.
    pub fn cdc_replication_slot_lag_bytes(&self) -> u64 {
        self.cdc_replication_slot_lag_bytes.load(Ordering::Relaxed)
    }

    // ── M16-03 (v0.115.0): new subsystem metrics ─────────────────────────────

    /// ER stage labels for histogram indexing.
    fn er_stage_index(stage: &str) -> usize {
        match stage {
            "blocking" => 0,
            "embedding" => 1,
            "shacl" => 2,
            "canonicalization" => 3,
            "provenance" => 4,
            _ => 4,
        }
    }

    /// Record entity-resolution stage latency.
    pub fn record_er_stage_duration(&self, stage: &str, duration: std::time::Duration) {
        let idx = Self::er_stage_index(stage);
        self.er_stage_duration_us[idx].fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    /// ER stage cumulative duration in seconds for the given stage label.
    pub fn er_stage_duration_secs(&self, stage: &str) -> f64 {
        let idx = Self::er_stage_index(stage);
        self.er_stage_duration_us[idx].load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    /// Increment the owl:sameAs assertions counter.
    pub fn record_sameas_assertion(&self) {
        self.sameas_assertions_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn sameas_assertions_total(&self) -> u64 {
        self.sameas_assertions_total.load(Ordering::Relaxed)
    }

    /// Record Bayesian propagation latency.
    pub fn record_bayesian_propagation_duration(&self, duration: std::time::Duration) {
        self.bayesian_propagation_duration_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn bayesian_propagation_duration_secs(&self) -> f64 {
        self.bayesian_propagation_duration_us
            .load(Ordering::Relaxed) as f64
            / 1_000_000.0
    }

    /// Update the temporal facts count snapshot.
    pub fn update_temporal_facts_total(&self, count: u64) {
        self.temporal_facts_total.store(count, Ordering::Relaxed);
    }

    pub fn temporal_facts_total(&self) -> u64 {
        self.temporal_facts_total.load(Ordering::Relaxed)
    }

    /// Increment the temporal query counter.
    pub fn record_temporal_query(&self) {
        self.temporal_queries_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn temporal_queries_total(&self) -> u64 {
        self.temporal_queries_total.load(Ordering::Relaxed)
    }

    /// Increment the PPRL Bloom encode counter.
    pub fn record_pprl_bloom_encode(&self) {
        self.pprl_bloom_encodes_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn pprl_bloom_encodes_total(&self) -> u64 {
        self.pprl_bloom_encodes_total.load(Ordering::Relaxed)
    }

    /// Record an LLM cache hit.
    pub fn record_llm_cache_hit(&self) {
        self.llm_cache_hits_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an LLM cache miss.
    pub fn record_llm_cache_miss(&self) {
        self.llm_cache_misses_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn llm_cache_hits_total(&self) -> u64 {
        self.llm_cache_hits_total.load(Ordering::Relaxed)
    }

    pub fn llm_cache_misses_total(&self) -> u64 {
        self.llm_cache_misses_total.load(Ordering::Relaxed)
    }

    /// Record proof-tree generation latency.
    pub fn record_proof_tree_duration(&self, duration: std::time::Duration) {
        self.proof_tree_duration_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn proof_tree_duration_secs(&self) -> f64 {
        self.proof_tree_duration_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    /// Increment the conflict detection counter.
    pub fn record_conflict_detection(&self) {
        self.conflict_detections_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn conflict_detections_total(&self) -> u64 {
        self.conflict_detections_total.load(Ordering::Relaxed)
    }

    // OBS-M-01 (v0.123.0): read-replica pool gauges.

    /// Update the read-replica pool size and available-connection snapshots.
    ///
    /// Pass `0, 0` when no replica pool is configured.
    pub fn update_replica_pool_stats(&self, size: u64, available: u64) {
        self.replica_pool_size.store(size, Ordering::Relaxed);
        self.replica_pool_available
            .store(available, Ordering::Relaxed);
    }

    pub fn replica_pool_size(&self) -> u64 {
        self.replica_pool_size.load(Ordering::Relaxed)
    }

    pub fn replica_pool_available(&self) -> u64 {
        self.replica_pool_available.load(Ordering::Relaxed)
    }

    // OBS-M-02 (v0.123.0): rule-library federation metrics.

    /// Record a completed `GET /rule-libraries/{name}/stream` response.
    pub fn record_rule_library_stream_duration(&self, duration: std::time::Duration) {
        self.rule_library_stream_duration_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn rule_library_stream_duration_secs(&self) -> f64 {
        self.rule_library_stream_duration_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    /// Increment the subscribe-error counter for `POST /rule-libraries/{name}/subscribe`.
    pub fn record_rule_library_subscribe_error(&self) {
        self.rule_library_subscribe_errors_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn rule_library_subscribe_errors_total(&self) -> u64 {
        self.rule_library_subscribe_errors_total
            .load(Ordering::Relaxed)
    }

    // FEAT-02 (v0.125.0): temporal graph snapshot gauge.

    /// Update the live graph-snapshot count.
    pub fn update_graph_snapshots_total(&self, count: u64) {
        self.graph_snapshots_total.store(count, Ordering::Relaxed);
    }

    pub fn graph_snapshots_total(&self) -> u64 {
        self.graph_snapshots_total.load(Ordering::Relaxed)
    }
}
