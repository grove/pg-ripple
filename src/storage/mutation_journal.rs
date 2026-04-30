//! Graph mutation journal (v0.67.0 MJOURNAL-01).
//!
//! A transaction-local journal that accumulates `(graph_id, WriteKind)` pairs
//! for the duration of a statement.  When the statement completes, the caller
//! flushes the journal to drive CONSTRUCT writeback, provenance cleanup, and
//! CWB metric increments from the journal rather than from individual API
//! wrappers.
//!
//! # Design
//!
//! - Journal is thread-local: concurrent transactions see independent journals.
//! - `record_write(g_id)` / `record_delete(g_id)` accumulate entries.
//! - `flush()` decodes graph IDs to IRIs, calls `on_graph_write` / `on_graph_delete`
//!   for each unique affected graph, then clears the journal.
//! - Zero-overhead when no construct rules are registered for any graph
//!   (the journal is not populated if `has_no_rules()` returns true, and
//!   the flush is a no-op on an empty journal).
//!
//! # Wiring status (v0.74.0 JOURNAL-DATALOG-01)
//!
//! The following write paths are wired through this journal:
//! - `dict_api.rs` — per-triple insert/delete (FLUSH-DEFER-01: deferred to stmt boundary)
//! - `bulk_load.rs` — all bulk loaders call `flush()` once after all rows (BULK-01)
//! - `datalog/seminaive.rs` — inference runs call `flush()` after materialization (JOURNAL-DATALOG-01)
//! - `r2rml.rs` — calls `load_ntriples()` which is already covered by bulk_load (JOURNAL-DATALOG-01)
//! - `cdc_bridge_api.rs` — calls `json_to_ntriples_and_load()` which is covered by bulk_load
//! - SPARQL Update — wired in v0.74.0 (CF-A fix)

use std::cell::RefCell;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WriteKind {
    Insert,
    Delete,
}

struct JournalEntry {
    graph_id: i64,
    kind: WriteKind,
}

thread_local! {
    static JOURNAL: RefCell<Vec<JournalEntry>> = const { RefCell::new(Vec::new()) };
}

/// Record a triple insertion into graph `g_id`.
///
/// The journal is not populated when no construct rules exist (fast path).
#[inline]
pub fn record_write(g_id: i64) {
    // Fast-path: skip journal when no rules are registered.
    if crate::construct_rules::has_no_rules() {
        return;
    }
    JOURNAL.with(|j| {
        j.borrow_mut().push(JournalEntry {
            graph_id: g_id,
            kind: WriteKind::Insert,
        });
    });
}

/// Record a triple deletion from graph `g_id`.
#[inline]
pub fn record_delete(g_id: i64) {
    if crate::construct_rules::has_no_rules() {
        return;
    }
    JOURNAL.with(|j| {
        j.borrow_mut().push(JournalEntry {
            graph_id: g_id,
            kind: WriteKind::Delete,
        });
    });
}

/// Clear the journal without firing any CWB hooks.
///
/// Called on transaction abort so that pending entries from a rolled-back
/// transaction do not fire after rollback (FLUSH-01).
pub fn clear() {
    JOURNAL.with(|j| j.borrow_mut().clear());
}

/// Flush the journal: call `on_graph_write` / `on_graph_delete` for each
/// unique affected graph, then clear the journal.
///
/// **Caller contract** (v0.74.0 DOC-JOURNAL-01 fix):
/// This function must be called at the *statement boundary* — once per SQL
/// statement, after all writes for that statement are complete.  Do **not**
/// call it once per triple (quadratic overhead; see FLUSH-DEFER-01).
///
/// Current call sites:
/// - End of every bulk-load function (`load_turtle`, `load_ntriples`, etc.) — BULK-01.
/// - End of Datalog inference runs (`run_inference_seminaive`, `run_inference`) — JOURNAL-DATALOG-01.
/// - End of `dict_api` write functions via executor-end hook — FLUSH-DEFER-01.
/// - SPARQL Update execution path — CF-A (v0.74.0).
///
/// Note: calling `flush()` when the journal is empty is a no-op (fast path).
pub fn flush() {
    JOURNAL.with(|j| {
        let mut entries = j.borrow_mut();
        if entries.is_empty() {
            return;
        }

        // Collect unique (graph_id, kind) pairs.  Process deletes first so
        // that CWB writeback sees a consistent state after retraction.
        let mut insert_graphs: Vec<i64> = Vec::new();
        let mut delete_graphs: Vec<i64> = Vec::new();

        for entry in entries.iter() {
            match entry.kind {
                WriteKind::Insert => {
                    if !insert_graphs.contains(&entry.graph_id) {
                        insert_graphs.push(entry.graph_id);
                    }
                }
                WriteKind::Delete => {
                    if !delete_graphs.contains(&entry.graph_id) {
                        delete_graphs.push(entry.graph_id);
                    }
                }
            }
        }
        entries.clear();
        // Release the borrow before calling into construct_rules (which may
        // itself call record_write for derived triples).
        drop(entries);

        // Process deletes first.
        for g_id in delete_graphs {
            let iri = graph_id_to_iri(g_id);
            crate::construct_rules::on_graph_delete(&iri);
        }
        // Then process inserts.
        for g_id in insert_graphs {
            let iri = graph_id_to_iri(g_id);
            crate::construct_rules::on_graph_write(&iri);
        }
    });
}

/// Decode a graph integer ID to its IRI string.
/// The default graph (id = 0) maps to `"default"`.
fn graph_id_to_iri(g_id: i64) -> String {
    if g_id == 0 {
        return "default".to_owned();
    }
    crate::dictionary::decode(g_id).unwrap_or_else(|| format!("__unknown_graph_{g_id}"))
}
