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
//! # MJOURNAL-02 wiring
//!
//! After this module lands, `dict_api.rs` removes its direct `on_graph_write` /
//! `on_graph_delete` calls and instead delegates to `record_write` / `record_delete`.
//! Bulk loaders call `flush()` once after all rows are inserted.
//! SPARQL Update calls `flush()` at the end of each statement execution.

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

/// Flush the journal: call `on_graph_write` / `on_graph_delete` for each
/// unique affected graph, then clear the journal.
///
/// This must be called:
/// - At the end of every public `dict_api` write function.
/// - At the end of every bulk-load function (`load_turtle`, `load_ntriples`, etc.).
/// - At the end of every SPARQL Update execution.
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
