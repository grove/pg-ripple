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

// -- Increment helpers --------------------------------------------------------

pub fn increment_cursor_pages_opened() {
    CURSOR_PAGES_OPENED.fetch_add(1, Ordering::Relaxed);
}

pub fn increment_cursor_pages_fetched() {
    CURSOR_PAGES_FETCHED.fetch_add(1, Ordering::Relaxed);
}

#[allow(dead_code)]
pub fn increment_cursor_rows_streamed(n: i64) {
    CURSOR_ROWS_STREAMED.fetch_add(n, Ordering::Relaxed);
}

#[allow(dead_code)]
pub fn increment_arrow_batches_sent(n: i64) {
    ARROW_BATCHES_SENT.fetch_add(n, Ordering::Relaxed);
}

#[allow(dead_code)]
pub fn increment_arrow_ticket_rejections() {
    ARROW_TICKET_REJECTIONS.fetch_add(1, Ordering::Relaxed);
}

pub fn increment_citus_brin_summarise_completed(n: i64) {
    CITUS_BRIN_SUMMARISE_COMPLETED.fetch_add(n, Ordering::Relaxed);
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
            "citus_brin_summarise_completed": CITUS_BRIN_SUMMARISE_COMPLETED.load(Ordering::Relaxed)
        }))
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    #[allow(unused_imports)]
    use pgrx::prelude::*;
}
