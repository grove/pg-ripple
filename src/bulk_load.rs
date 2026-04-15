//! Bulk loading — N-Triples, N-Quads, Turtle, TriG (with RDF-star support).
//!
//! All loaders follow the same pipeline:
//!
//! 1. Advance the `_pg_ripple.load_generation_seq` sequence to scope blank nodes.
//! 2. Parse the input using custom (N-Triples-star) or `rio_turtle` streaming parsers.
//! 3. Encode all terms via the dictionary (with backend-local LRU cache).
//!    Quoted triples (`<< s p o >>`) are encoded recursively via
//!    `dictionary::encode_quoted_triple`.
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
use rio_turtle::{NQuadsParser, TriGParser, TurtleError, TurtleParser};
use rio_xml::{RdfXmlError, RdfXmlParser};

use crate::dictionary;
use crate::storage;

/// Number of triples to collect before flushing a batch insert.
const BATCH_SIZE: usize = 10_000;

// ─── rio_api term encoding ───────────────────────────────────────────────────

fn encode_subject(subject: &Subject<'_>, generation: i64) -> i64 {
    match subject {
        Subject::NamedNode(n) => dictionary::encode(n.iri, dictionary::KIND_IRI),
        Subject::BlankNode(b) => {
            let scoped = format!("{}:{}", generation, b.id);
            dictionary::encode(&scoped, dictionary::KIND_BLANK)
        }
        Subject::Triple(_) => {
            pgrx::warning!(
                "RDF-star quoted-triple subject not supported in this format; use load_ntriples() for N-Triples-star"
            );
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
        Term::Triple(_) => {
            pgrx::warning!(
                "RDF-star quoted-triple object not supported in this format; use load_ntriples() for N-Triples-star"
            );
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
///
/// Applies back-pressure when the shared-memory cache utilization exceeds 90%
/// of the configured `pg_ripple.cache_budget`: the effective batch size is
/// capped at 1/4 of the default to reduce delta-write pressure.
fn flush_batch(by_predicate: &mut PredicateBatch) {
    // Back-pressure: if cache_budget > 0 and utilization > 90%, log a notice.
    let budget_mb = crate::CACHE_BUDGET_MB.get();
    if budget_mb > 0 {
        let util_pct = crate::shmem::cache_utilization_pct();
        if util_pct > 90 {
            pgrx::warning!(
                "pg_ripple: shared-memory encode cache is {}% full (budget: {} MB); consider running pg_ripple.compact() to reduce delta growth",
                util_pct,
                budget_mb
            );
        }
    }

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
        let has_table: bool = Spi::get_one_with_args::<bool>(
            "SELECT EXISTS(SELECT 1 FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL)",
            &[DatumWithOid::from(*p_id)],
        )
        .unwrap_or(None)
        .unwrap_or(false);

        if has_table {
            // In HTAP mode (v0.6.0+) vp_{id} is a VIEW; ANALYZE the
            // underlying _delta and _main tables instead.
            let delta = format!("_pg_ripple.vp_{p_id}_delta");
            let main = format!("_pg_ripple.vp_{p_id}_main");
            Spi::run_with_args(&format!("ANALYZE {delta}"), &[])
                .unwrap_or_else(|e| pgrx::warning!("ANALYZE {}: {}", delta, e));
            Spi::run_with_args(&format!("ANALYZE {main}"), &[])
                .unwrap_or_else(|e| pgrx::warning!("ANALYZE {}: {}", main, e));
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

/// Load N-Triples (or N-Triples-star) data from a text string.
///
/// Supports full N-Triples-star syntax via a custom line parser:
/// - Standard N-Triples: `<s> <p> <o> .`
/// - Subject-position quoted triples: `<< s p o >> <p2> <o2> .`
/// - Object-position quoted triples: `<s> <p> << s2 p2 o2 >> .`
/// - Nested quoted triples (recursive)
///
/// Returns the number of triples loaded.
pub fn load_ntriples(data: &str) -> i64 {
    let generation = storage::next_load_generation();
    let mut by_predicate: PredicateBatch = HashMap::new();
    let mut touched: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut total = 0i64;

    for line in data.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((s_id, p_id, o_id)) = parse_nt_star_full_line(trimmed, generation) {
            touched.insert(p_id);
            by_predicate.entry(p_id).or_default().push((s_id, o_id, 0));
            total += 1;
            if total % BATCH_SIZE as i64 == 0 {
                flush_batch(&mut by_predicate);
            }
        } else {
            pgrx::warning!("N-Triples parse error on: {}", trimmed);
        }
    }

    flush_batch(&mut by_predicate);
    post_load_cleanup(touched.into_iter().collect());
    total
}

/// Load N-Quads data from a text string (supports named graphs).
/// Uses rio_turtle for N-Quads. RDF-star in quoted-triple positions emits a warning.
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
/// Uses rio_turtle's `TurtleParser`. For Turtle-star content, use load_ntriples().
pub fn load_turtle(data: &str) -> i64 {
    let generation = storage::next_load_generation();
    let mut by_predicate: PredicateBatch = HashMap::new();
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
/// Uses rio_turtle for full TriG/named-graph support.
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

/// Load RDF/XML data from a text string.  Returns the number of triples loaded.
///
/// Uses `rio_xml::RdfXmlParser` for conformant RDF/XML parsing.  Named graphs
/// are not supported in the RDF/XML format; all triples are loaded into the
/// default graph.
pub fn load_rdfxml(data: &str) -> i64 {
    let generation = storage::next_load_generation();
    let mut by_predicate: PredicateBatch = HashMap::new();
    let mut touched: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut total = 0i64;

    let mut parser = RdfXmlParser::new(data.as_bytes(), None);
    parser
        .parse_all::<RdfXmlError>(&mut |triple| {
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
        .unwrap_or_else(|e| pgrx::error!("RDF/XML parse error: {e}"));

    flush_batch(&mut by_predicate);
    post_load_cleanup(touched.into_iter().collect());
    total
}

// ─── N-Triples-star custom parser ────────────────────────────────────────────
//
// Handles all N-Triples and N-Triples-star lines, including:
//   <http://s> <http://p> <http://o> .
//   << <http://s> <http://p> <http://o> >> <http://p2> <http://o2> .
//   <http://s> <http://p> << <http://s2> <http://p2> <http://o2> >> .
//
// No external crate is needed; the parser is a simple recursive-descent over
// the N-Triples-star grammar. This avoids the oxrdf `rdf-12` feature conflict
// with spargebra 0.4.x.

/// Parse a complete N-Triples-star line: `term   term   term   .`
/// Returns `Some((s_id, p_id, o_id))` on success, `None` on failure.
fn parse_nt_star_full_line(line: &str, generation: i64) -> Option<(i64, i64, i64)> {
    let line = line.trim_end_matches(|c: char| c.is_whitespace());
    let line = line.strip_suffix('.')?;
    let line = line.trim_end();

    let (s_id, rest) = parse_nt_star_term(line, generation)?;
    let rest = rest.trim_start();
    let (p_id, rest) = parse_nt_star_term(rest, generation)?;
    let rest = rest.trim_start();
    let (o_id, _rest) = parse_nt_star_term(rest, generation)?;
    Some((s_id, p_id, o_id))
}

/// Parse one N-Triples-star term from the front of `s`, returning:
/// - the dictionary ID for the term
/// - the remaining unparsed string
///
/// Handles: `<iri>`, `_:blank`, `"literal"`, `"literal"^^<datatype>`, `"literal"@lang`,
/// and `<< ... >>` nested quoted triples.
fn parse_nt_star_term(s: &str, generation: i64) -> Option<(i64, &str)> {
    let s = s.trim_start();

    if s.starts_with("<<") {
        // Quoted triple: find matching >>
        return parse_nt_star_quoted_triple(s, generation);
    }

    if let Some(rest) = s.strip_prefix('<') {
        // IRI: find closing >
        let end = rest.find('>')?;
        let iri = &rest[..end];
        let id = dictionary::encode(iri, dictionary::KIND_IRI);
        Some((id, &rest[end + 1..]))
    } else if let Some(rest) = s.strip_prefix("_:") {
        // Blank node
        let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        let label = &rest[..end];
        let scoped = format!("{}:{}", generation, label);
        let id = dictionary::encode(&scoped, dictionary::KIND_BLANK);
        Some((id, &rest[end..]))
    } else if s.starts_with('"') {
        // Literal: parse quoted string then optional ^^<dt> or @lang
        parse_nt_star_literal(s)
    } else {
        None
    }
}

/// Parse `<< term term term >>` from the front of `s`.
/// Returns `(quoted_triple_id, remaining)`.
fn parse_nt_star_quoted_triple(s: &str, generation: i64) -> Option<(i64, &str)> {
    let inner = s.strip_prefix("<<")?;
    let inner = inner.trim_start();

    let (s_id, rest) = parse_nt_star_term(inner, generation)?;
    let rest = rest.trim_start();
    let (p_id, rest) = parse_nt_star_term(rest, generation)?;
    let rest = rest.trim_start();
    let (o_id, rest) = parse_nt_star_term(rest, generation)?;
    let rest = rest.trim_start();

    let rest = rest.strip_prefix(">>")?;
    let qt_id = dictionary::encode_quoted_triple(s_id, p_id, o_id);
    Some((qt_id, rest))
}

/// Parse an N-Triples literal from the front of `s`.
/// Returns `(id, remaining)`.
fn parse_nt_star_literal(s: &str) -> Option<(i64, &str)> {
    // s starts with "
    let bytes = s.as_bytes();
    let mut i = 1usize; // skip opening quote
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
        } else if bytes[i] == b'"' {
            break;
        } else {
            i += 1;
        }
    }
    // i points to the closing quote
    if i >= bytes.len() {
        return None;
    }
    let raw_value = &s[1..i];
    let value = raw_value
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
        .replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t");
    let rest = &s[i + 1..];

    if let Some(dt_rest) = rest.strip_prefix("^^<") {
        let end = dt_rest.find('>')?;
        let dt = &dt_rest[..end];
        let id = if dt == "http://www.w3.org/2001/XMLSchema#string" {
            dictionary::encode_plain_literal(&value)
        } else {
            dictionary::encode_typed_literal(&value, dt)
        };
        Some((id, &dt_rest[end + 1..]))
    } else if let Some(lang_rest) = rest.strip_prefix('@') {
        let end = lang_rest
            .find(|c: char| c.is_whitespace())
            .unwrap_or(lang_rest.len());
        let lang = &lang_rest[..end];
        let id = dictionary::encode_lang_literal(&value, lang);
        Some((id, &lang_rest[end..]))
    } else {
        let id = dictionary::encode_plain_literal(&value);
        Some((id, rest))
    }
}
