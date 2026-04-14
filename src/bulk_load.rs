//! Bulk loading — N-Triples, N-Quads, Turtle, TriG.
//!
//! All loaders follow the same pipeline:
//!
//! 1. Advance the `_pg_ripple.load_generation_seq` sequence to scope blank nodes.
//! 2. Parse the input using `rio_turtle` streaming parsers.
//! 3. Encode all terms via the dictionary (with backend-local LRU cache).
//! 4. Batch-insert triples in groups of `BATCH_SIZE` per predicate.
//! 5. Call `promote_rare_predicates()` once after the entire load.
//! 6. Run `ANALYZE` on affected VP tables so the planner has fresh statistics.
//!
//! File-path variants read the file content via `pg_read_file()` (superuser-only
//! PostgreSQL built-in) and then delegate to the inline TEXT variants.

use std::collections::HashMap;

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use rio_api::model::{GraphName, Literal, NamedNode, Subject, Term};
use rio_api::parser::{QuadsParser, TriplesParser};
use rio_turtle::{NQuadsParser, NTriplesParser, TriGParser, TurtleError, TurtleParser};

use crate::dictionary;
use crate::storage;

/// Number of triples to collect before flushing a batch insert.
const BATCH_SIZE: usize = 10_000;

// ─── Term encoding helpers ───────────────────────────────────────────────────

fn encode_subject(subject: &Subject<'_>, generation: i64) -> i64 {
    match subject {
        Subject::NamedNode(n) => dictionary::encode(n.iri, dictionary::KIND_IRI),
        Subject::BlankNode(b) => {
            let scoped = format!("{}:{}", generation, b.id);
            dictionary::encode(&scoped, dictionary::KIND_BLANK)
        }
        // RDF-star triple terms — not supported in v0.2.0; encode as a placeholder IRI.
        Subject::Triple(_) => {
            pgrx::warning!("RDF-star quoted triples not supported in v0.2.0; skipping subject");
            dictionary::encode("_rdfstar_unsupported_", dictionary::KIND_IRI)
        }
    }
}

fn encode_named_node(n: &NamedNode<'_>) -> i64 {
    dictionary::encode(n.iri, dictionary::KIND_IRI)
}

fn encode_term(term: &Term<'_>, generation: i64) -> i64 {
    match term {
        Term::NamedNode(n) => dictionary::encode(n.iri, dictionary::KIND_IRI),
        Term::BlankNode(b) => {
            let scoped = format!("{}:{}", generation, b.id);
            dictionary::encode(&scoped, dictionary::KIND_BLANK)
        }
        Term::Literal(lit) => match lit {
            Literal::Simple { value } => dictionary::encode_plain_literal(value),
            Literal::LanguageTaggedString { value, language } => {
                dictionary::encode_lang_literal(value, language)
            }
            Literal::Typed { value, datatype } => {
                dictionary::encode_typed_literal(value, datatype.iri)
            }
        },
        // RDF-star — not supported in v0.2.0.
        Term::Triple(_) => {
            pgrx::warning!("RDF-star quoted triples not supported in v0.2.0; encoding as IRI");
            dictionary::encode("_rdfstar_unsupported_", dictionary::KIND_IRI)
        }
    }
}

fn encode_graph_name_opt(graph_name: &Option<GraphName<'_>>) -> i64 {
    match graph_name {
        Some(GraphName::NamedNode(n)) => dictionary::encode(n.iri, dictionary::KIND_IRI),
        _ => 0_i64,
    }
}

// ─── Batch flush helper ───────────────────────────────────────────────────────

type TripleRow = (i64, i64, i64);
type PredicateBatch = HashMap<i64, Vec<TripleRow>>;

/// Flush accumulated triples (grouped by predicate) in batched VP inserts.
fn flush_batch(by_predicate: &mut PredicateBatch) {
    let groups: Vec<(i64, Vec<TripleRow>)> = by_predicate.drain().collect();
    for (p_id, rows) in groups {
        storage::batch_insert_encoded(p_id, &rows);
    }
}

// ─── ANALYZE helper ──────────────────────────────────────────────────────────

/// Run ANALYZE on every VP table touched since the start of a load.
fn analyze_affected_tables(touched_predicates: &[i64]) {
    for p_id in touched_predicates {
        // Check if there's a dedicated table for this predicate.
        let table_name: Option<String> = Spi::get_one_with_args::<String>(
            "SELECT '_pg_ripple.vp_' || id::text \
             FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL",
            &[DatumWithOid::from(*p_id)],
        )
        .unwrap_or(None);

        if let Some(table) = table_name {
            Spi::run_with_args(&format!("ANALYZE {table}"), &[])
                .unwrap_or_else(|e| pgrx::warning!("ANALYZE {}: {}", table, e));
        }
    }
    // Also ANALYZE vp_rare (catches any rare predicates not yet promoted).
    Spi::run_with_args("ANALYZE _pg_ripple.vp_rare", &[])
        .unwrap_or_else(|e| pgrx::warning!("ANALYZE vp_rare: {}", e));
}

// ─── Post-load cleanup ────────────────────────────────────────────────────────

fn post_load_cleanup(touched_predicates: Vec<i64>) {
    storage::promote_rare_predicates();
    analyze_affected_tables(&touched_predicates);
}

// ─── Public loaders ──────────────────────────────────────────────────────────

/// Load N-Triples data from a text string.
/// Returns the number of triples loaded.
pub fn load_ntriples(data: &str) -> i64 {
    let generation = storage::next_load_generation();
    let mut by_predicate: HashMap<i64, Vec<(i64, i64, i64)>> = HashMap::new();
    let mut touched: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut total = 0i64;

    let mut parser = NTriplesParser::new(data.as_bytes());
    parser
        .parse_all::<TurtleError>(&mut |triple| {
            let s_id = encode_subject(&triple.subject, generation);
            let p_id = encode_named_node(&triple.predicate);
            let o_id = encode_term(&triple.object, generation);
            touched.insert(p_id);
            by_predicate.entry(p_id).or_default().push((s_id, o_id, 0));
            total += 1;
            if total % BATCH_SIZE as i64 == 0 {
                flush_batch(&mut by_predicate);
            }
            Ok(())
        })
        .unwrap_or_else(|e| pgrx::error!("N-Triples parse error: {e}"));

    flush_batch(&mut by_predicate);
    post_load_cleanup(touched.into_iter().collect());
    total
}

/// Load N-Quads data from a text string (named graph support).
pub fn load_nquads(data: &str) -> i64 {
    let generation = storage::next_load_generation();
    let mut by_predicate: HashMap<i64, Vec<(i64, i64, i64)>> = HashMap::new();
    let mut touched: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut total = 0i64;

    let mut parser = NQuadsParser::new(data.as_bytes());
    parser
        .parse_all::<TurtleError>(&mut |quad| {
            let s_id = encode_subject(&quad.subject, generation);
            let p_id = encode_named_node(&quad.predicate);
            let o_id = encode_term(&quad.object, generation);
            let g_id = encode_graph_name_opt(&quad.graph_name);
            touched.insert(p_id);
            by_predicate
                .entry(p_id)
                .or_default()
                .push((s_id, o_id, g_id));
            total += 1;
            if total % BATCH_SIZE as i64 == 0 {
                flush_batch(&mut by_predicate);
            }
            Ok(())
        })
        .unwrap_or_else(|e| pgrx::error!("N-Quads parse error: {e}"));

    flush_batch(&mut by_predicate);
    post_load_cleanup(touched.into_iter().collect());
    total
}

/// Load Turtle data from a text string.
pub fn load_turtle(data: &str) -> i64 {
    let generation = storage::next_load_generation();
    let mut by_predicate: HashMap<i64, Vec<(i64, i64, i64)>> = HashMap::new();
    let mut touched: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut total = 0i64;

    let mut parser = TurtleParser::new(data.as_bytes(), None);
    parser
        .parse_all::<TurtleError>(&mut |triple| {
            let s_id = encode_subject(&triple.subject, generation);
            let p_id = encode_named_node(&triple.predicate);
            let o_id = encode_term(&triple.object, generation);
            touched.insert(p_id);
            by_predicate.entry(p_id).or_default().push((s_id, o_id, 0));
            total += 1;
            if total % BATCH_SIZE as i64 == 0 {
                flush_batch(&mut by_predicate);
            }
            Ok(())
        })
        .unwrap_or_else(|e| pgrx::error!("Turtle parse error: {e}"));

    flush_batch(&mut by_predicate);
    post_load_cleanup(touched.into_iter().collect());
    total
}

/// Load TriG data from a text string (Turtle with named graph blocks).
pub fn load_trig(data: &str) -> i64 {
    let generation = storage::next_load_generation();
    let mut by_predicate: HashMap<i64, Vec<(i64, i64, i64)>> = HashMap::new();
    let mut touched: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut total = 0i64;

    let mut parser = TriGParser::new(data.as_bytes(), None);
    parser
        .parse_all::<TurtleError>(&mut |quad| {
            let s_id = encode_subject(&quad.subject, generation);
            let p_id = encode_named_node(&quad.predicate);
            let o_id = encode_term(&quad.object, generation);
            let g_id = encode_graph_name_opt(&quad.graph_name);
            touched.insert(p_id);
            by_predicate
                .entry(p_id)
                .or_default()
                .push((s_id, o_id, g_id));
            total += 1;
            if total % BATCH_SIZE as i64 == 0 {
                flush_batch(&mut by_predicate);
            }
            Ok(())
        })
        .unwrap_or_else(|e| pgrx::error!("TriG parse error: {e}"));

    flush_batch(&mut by_predicate);
    post_load_cleanup(touched.into_iter().collect());
    total
}

// ─── File-path variants ───────────────────────────────────────────────────────

/// Read file content via PostgreSQL's `pg_read_file()` (superuser-only).
fn read_file_content(path: &str) -> String {
    // pg_read_file() requires superuser or pg_monitor role; SPI propagates
    // the caller's privileges, so a non-superuser call will fail with a
    // permissions error — no additional check needed here.
    Spi::get_one_with_args::<String>("SELECT pg_read_file($1)", &[DatumWithOid::from(path)])
        .unwrap_or_else(|e| pgrx::error!("pg_read_file({path}): {e}"))
        .unwrap_or_else(|| pgrx::error!("pg_read_file({path}): returned NULL"))
}

/// Load N-Triples from a server-side file path.
pub fn load_ntriples_file(path: &str) -> i64 {
    let content = read_file_content(path);
    load_ntriples(&content)
}

/// Load N-Quads from a server-side file path.
pub fn load_nquads_file(path: &str) -> i64 {
    let content = read_file_content(path);
    load_nquads(&content)
}

/// Load Turtle from a server-side file path.
pub fn load_turtle_file(path: &str) -> i64 {
    let content = read_file_content(path);
    load_turtle(&content)
}

/// Load TriG from a server-side file path.
pub fn load_trig_file(path: &str) -> i64 {
    let content = read_file_content(path);
    load_trig(&content)
}
