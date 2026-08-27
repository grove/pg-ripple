//! Streaming and distributed observability metrics (v0.66.0 -- OBS-01).
//!
//! Provides atomic counters for cursor paging, Arrow export, and distributed
//! execution. Counters are process-lifetime (reset on pg_ripple reload).
//!
//! SQL surface: `pg_ripple.streaming_metrics()` returns a JSONB object.

use std::sync::atomic::{AtomicI64, Ordering};

// -- Atomic counters ----------------------------------------------------------

/// Total number of SPARQL cursors (portals) opened via `sparql_cursor`.
static CURSOR_PAGES_OPENED: AtomicI64 = AtomicI64::new(0);

/// Total number of pages fetched by `CursorIter::fetch_page`.
static CURSOR_PAGES_FETCHED: AtomicI64 = AtomicI64::new(0);

/// Total rows emitted by `sparql_cursor` (across all sessions, process-lifetime).
static CURSOR_ROWS_STREAMED: AtomicI64 = AtomicI64::new(0);

/// Total Arrow IPC batches sent by the flight endpoint.
static ARROW_BATCHES_SENT: AtomicI64 = AtomicI64::new(0);

/// Total Arrow ticket validation failures.
static ARROW_TICKET_REJECTIONS: AtomicI64 = AtomicI64::new(0);

/// Total Citus BRIN summarise operations completed after merge.
static CITUS_BRIN_SUMMARISE_COMPLETED: AtomicI64 = AtomicI64::new(0);

// ─── v0.94.0 bidi relay counters (H15-03) ─────────────────────────────────────

/// Current number of in-flight bidi relay dispatch calls (per process).
/// Used to gate new relay calls against `bidi_relay_max_inflight`.
pub(crate) static BIDI_RELAY_INFLIGHT: AtomicI64 = AtomicI64::new(0);

/// Total number of bidi relay dispatch calls dropped due to inflight overflow.
/// Exposed via `streaming_metrics()` and the `/metrics` Prometheus endpoint.
pub(crate) static BIDI_RELAY_DROPPED_TOTAL: AtomicI64 = AtomicI64::new(0);

// ─── v0.129.0 JSON writeback drain-queue counters (L18-02) ────────────────────

/// Total status-update failures inside `drain_json_writeback_queue()`.
/// Exposed via `streaming_metrics()` and the `/metrics` Prometheus endpoint.
static JSON_WRITEBACK_DRAIN_ERRORS_TOTAL: AtomicI64 = AtomicI64::new(0);

/// Increment the writeback drain-queue status-update error counter (L18-02).
pub fn increment_json_writeback_drain_errors() {
    JSON_WRITEBACK_DRAIN_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
}

// -- Increment helpers --------------------------------------------------------

pub fn increment_cursor_pages_opened() {
    CURSOR_PAGES_OPENED.fetch_add(1, Ordering::Relaxed);
}

pub fn increment_cursor_pages_fetched() {
    CURSOR_PAGES_FETCHED.fetch_add(1, Ordering::Relaxed);
}

// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[allow(dead_code)]
pub fn increment_cursor_rows_streamed(n: i64) {
    CURSOR_ROWS_STREAMED.fetch_add(n, Ordering::Relaxed);
}

// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[allow(dead_code)]
pub fn increment_arrow_batches_sent(n: i64) {
    ARROW_BATCHES_SENT.fetch_add(n, Ordering::Relaxed);
}

// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[allow(dead_code)]
pub fn increment_arrow_ticket_rejections() {
    ARROW_TICKET_REJECTIONS.fetch_add(1, Ordering::Relaxed);
}

pub fn increment_citus_brin_summarise_completed(n: i64) {
    CITUS_BRIN_SUMMARISE_COMPLETED.fetch_add(n, Ordering::Relaxed);
}

// ─── v0.94.0 bidi relay helpers ───────────────────────────────────────────────

/// Try to acquire an inflight slot for a bidi relay dispatch.
/// Returns `true` if the slot was acquired (caller should call `relay_inflight_release` when done).
/// Returns `false` if max_inflight is reached; the dropped counter is incremented.
pub fn relay_inflight_acquire() -> bool {
    let max = crate::BIDI_RELAY_MAX_INFLIGHT.get() as i64;
    let current = BIDI_RELAY_INFLIGHT.load(Ordering::Relaxed);
    if current >= max {
        BIDI_RELAY_DROPPED_TOTAL.fetch_add(1, Ordering::Relaxed);
        false
    } else {
        BIDI_RELAY_INFLIGHT.fetch_add(1, Ordering::Relaxed);
        true
    }
}

/// Release an inflight slot acquired by `relay_inflight_acquire`.
pub fn relay_inflight_release() {
    BIDI_RELAY_INFLIGHT.fetch_sub(1, Ordering::Relaxed);
}

// -- SQL API ------------------------------------------------------------------

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    /// Return streaming and distributed observability metrics as JSONB.
    ///
    /// Counters are process-lifetime and reset on extension reload.
    ///
    /// Keys returned:
    /// - `cursor_pages_opened`            -- portals opened by sparql_cursor
    /// - `cursor_pages_fetched`           -- pages fetched by CursorIter
    /// - `cursor_rows_streamed`           -- total rows emitted
    /// - `arrow_batches_sent`             -- Arrow IPC batches sent (HTTP service)
    /// - `arrow_ticket_rejections`        -- invalid/expired ticket rejections
    /// - `citus_brin_summarise_completed` -- BRIN summarise ops after merge
    /// - `json_writeback_drain_errors_total` -- drain_json_writeback_queue() status-update failures (L18-02)
    ///
    /// ```sql
    /// SELECT pg_ripple.streaming_metrics();
    /// ```
    #[pg_extern]
    pub fn streaming_metrics() -> pgrx::JsonB {
        use super::*;
        pgrx::JsonB(serde_json::json!({
            "cursor_pages_opened":            CURSOR_PAGES_OPENED.load(Ordering::Relaxed),
            "cursor_pages_fetched":           CURSOR_PAGES_FETCHED.load(Ordering::Relaxed),
            "cursor_rows_streamed":           CURSOR_ROWS_STREAMED.load(Ordering::Relaxed),
            "arrow_batches_sent":             ARROW_BATCHES_SENT.load(Ordering::Relaxed),
            "arrow_ticket_rejections":        ARROW_TICKET_REJECTIONS.load(Ordering::Relaxed),
            "citus_brin_summarise_completed": CITUS_BRIN_SUMMARISE_COMPLETED.load(Ordering::Relaxed),
            "bidi_relay_inflight":            super::BIDI_RELAY_INFLIGHT.load(Ordering::Relaxed),
            "bidi_relay_dropped_total":       super::BIDI_RELAY_DROPPED_TOTAL.load(Ordering::Relaxed),
            "json_writeback_drain_errors_total": super::JSON_WRITEBACK_DRAIN_ERRORS_TOTAL.load(Ordering::Relaxed)
        }))
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    // A16-CQ: unused_imports here is intentional for test/cfg-gated code paths.
    #[allow(unused_imports)]
    use pgrx::prelude::*;
}
