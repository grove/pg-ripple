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
use std::sync::atomic::{AtomicU8, Ordering};

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

/// Last cache-full percentage we warned about (per-process; avoids warning storms).
static LAST_WARNED_CACHE_PCT: AtomicU8 = AtomicU8::new(0);

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
        let (_, _, _, utilisation) = crate::shmem::get_cache_stats();
        let util_pct = (utilisation * 100.0) as u8;
        if util_pct > 90 {
            // Only warn once per percentage point to avoid flooding the client.
            let prev = LAST_WARNED_CACHE_PCT.load(Ordering::Relaxed);
            if util_pct > prev {
                LAST_WARNED_CACHE_PCT.store(util_pct, Ordering::Relaxed);
                pgrx::warning!(
                    "pg_ripple: shared-memory encode cache is {}% full (budget: {} MB); consider running pg_ripple.compact() to reduce delta growth",
                    util_pct,
                    budget_mb
                );
            }
        } else {
            // Reset threshold when utilization drops back below 90%.
            LAST_WARNED_CACHE_PCT.store(0, Ordering::Relaxed);
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
pub fn load_ntriples(data: &str, strict: bool) -> i64 {
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
        } else if strict {
            pgrx::error!("N-Triples parse error (strict mode): {}", trimmed);
        } else {
            pgrx::warning!("N-Triples parse error on: {}", trimmed);
        }
    }

    flush_batch(&mut by_predicate);
    // BULK-01: flush mutation journal after all batches so CWB writeback fires
    // once per load_* call rather than never (bulk inserts skip per-triple flush).
    crate::storage::mutation_journal::flush();
    post_load_cleanup(touched.into_iter().collect());
    // v0.58.0: emit PROV-O provenance triples if enabled.
    crate::prov::emit_load_provenance("ntriples:inline", total);
    total
}

/// Load N-Quads data from a text string (supports named graphs).
/// Uses rio_turtle for N-Quads. RDF-star in quoted-triple positions emits a warning.
pub fn load_nquads(data: &str, _strict: bool) -> i64 {
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
    // BULK-01: flush mutation journal after all batches.
    crate::storage::mutation_journal::flush();
    post_load_cleanup(touched.into_iter().collect());
    // v0.58.0: emit PROV-O provenance triples if enabled.
    crate::prov::emit_load_provenance("nquads:inline", total);
    total
}

/// Load Turtle data from a text string.
/// Uses rio_turtle's `TurtleParser`. For Turtle-star content, use load_ntriples().
pub fn load_turtle(data: &str, _strict: bool) -> i64 {
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
    // BULK-01: flush mutation journal after all batches.
    crate::storage::mutation_journal::flush();
    post_load_cleanup(touched.into_iter().collect());
    // v0.58.0: emit PROV-O provenance triples if enabled.
    crate::prov::emit_load_provenance("turtle:inline", total);
    total
}

/// Load TriG data from a text string (Turtle with named graph blocks).
/// Uses rio_turtle for full TriG/named-graph support.
pub fn load_trig(data: &str, _strict: bool) -> i64 {
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
    // BULK-01: flush mutation journal after all batches.
    crate::storage::mutation_journal::flush();
    post_load_cleanup(touched.into_iter().collect());
    total
}

// ─── File-path variants ───────────────────────────────────────────────────────

/// Read file content via PostgreSQL's `pg_read_file()` (superuser-only).
fn read_file_content(path: &str) -> String {
    // v0.55.0 C-2: path allowlist (default-deny).
    // The allowlist check runs on the RAW path BEFORE canonicalize so that
    // even non-existent paths get a clear PT403 error rather than a confusing
    // "could not resolve path" message.
    let allowed_paths = crate::COPY_RDF_ALLOWED_PATHS
        .get()
        .and_then(|c| c.to_str().ok().map(|s| s.to_owned()));
    match allowed_paths.as_deref() {
        None | Some("") => {
            pgrx::error!(
                "PT403: pg_ripple.copy_rdf_allowed_paths is not set; \
                 all paths are denied by default. \
                 Set this GUC to a comma-separated list of allowed path prefixes."
            );
        }
        Some(prefixes_str) => {
            let allowed = prefixes_str
                .split(',')
                .map(|p| p.trim())
                .filter(|p| !p.is_empty())
                .any(|prefix| path.starts_with(prefix));
            if !allowed {
                pgrx::error!(
                    "PT403: path \"{path}\" is not in pg_ripple.copy_rdf_allowed_paths allowlist"
                );
            }
        }
    }

    // S-8: Resolve symlinks and verify the canonical path resides within the
    // PostgreSQL data directory, preventing path-traversal and symlink attacks.
    // This secondary check guards against traversal even for allowlisted prefixes.
    let data_dir =
        Spi::get_one_with_args::<String>("SELECT current_setting('data_directory')", &[])
            .unwrap_or_else(|e| pgrx::error!("could not read data_directory: {e}"))
            .unwrap_or_else(|| pgrx::error!("data_directory is NULL"));

    let canonical = std::fs::canonicalize(path)
        .unwrap_or_else(|e| pgrx::error!("could not resolve path \"{path}\": {e}"));

    if !canonical.starts_with(&data_dir) {
        pgrx::error!("permission denied: \"{path}\" is outside the database cluster directory");
    }

    // pg_read_file() requires superuser or pg_monitor role; SPI propagates
    // the caller's privileges, so a non-superuser call will fail with a
    // permissions error — no additional check needed here.
    Spi::get_one_with_args::<String>("SELECT pg_read_file($1)", &[DatumWithOid::from(path)])
        .unwrap_or_else(|e| pgrx::error!("pg_read_file({path}): {e}"))
        .unwrap_or_else(|| pgrx::error!("pg_read_file({path}): returned NULL"))
}

/// Load N-Triples from a server-side file path.
pub fn load_ntriples_file(path: &str, strict: bool) -> i64 {
    let content = read_file_content(path);
    load_ntriples(&content, strict)
}

/// Load N-Quads from a server-side file path.
pub fn load_nquads_file(path: &str, strict: bool) -> i64 {
    let content = read_file_content(path);
    load_nquads(&content, strict)
}

/// Load Turtle from a server-side file path.
pub fn load_turtle_file(path: &str, strict: bool) -> i64 {
    let content = read_file_content(path);
    load_turtle(&content, strict)
}

/// Load TriG from a server-side file path.
pub fn load_trig_file(path: &str, strict: bool) -> i64 {
    let content = read_file_content(path);
    load_trig(&content, strict)
}

/// Load RDF/XML from a server-side file path (superuser required).
pub fn load_rdfxml_file(path: &str, strict: bool) -> i64 {
    let content = read_file_content(path);
    load_rdfxml(&content, strict)
}

/// Load N-Triples from a server-side file into a specific graph.
pub fn load_ntriples_file_into_graph(path: &str, g_id: i64) -> i64 {
    let content = read_file_content(path);
    load_ntriples_into_graph(&content, g_id)
}

/// Load Turtle from a server-side file into a specific graph.
pub fn load_turtle_file_into_graph(path: &str, g_id: i64) -> i64 {
    let content = read_file_content(path);
    load_turtle_into_graph(&content, g_id)
}

/// Load RDF/XML from a server-side file into a specific graph.
pub fn load_rdfxml_file_into_graph(path: &str, g_id: i64) -> i64 {
    let content = read_file_content(path);
    load_rdfxml_into_graph(&content, g_id)
}

/// Load RDF/XML data from a text string.  Returns the number of triples loaded.
///
/// Uses `rio_xml::RdfXmlParser` for conformant RDF/XML parsing.  Named graphs
/// are not supported in the RDF/XML format; all triples are loaded into the
/// default graph.
pub fn load_rdfxml(data: &str, _strict: bool) -> i64 {
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
    // BULK-01: flush mutation journal after all batches.
    crate::storage::mutation_journal::flush();
    post_load_cleanup(touched.into_iter().collect());
    total
}

// ─── Graph-aware loaders (for SPARQL LOAD <url> INTO GRAPH <g>) ──────────────

/// Load N-Triples data into a specific graph.  Returns the number of triples loaded.
pub fn load_ntriples_into_graph(data: &str, g_id: i64) -> i64 {
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
            by_predicate
                .entry(p_id)
                .or_default()
                .push((s_id, o_id, g_id));
            total += 1;
            if total % BATCH_SIZE as i64 == 0 {
                flush_batch(&mut by_predicate);
            }
        } else {
            pgrx::warning!("N-Triples parse error on: {}", trimmed);
        }
    }

    flush_batch(&mut by_predicate);
    // BULK-01: flush mutation journal after all batches.
    crate::storage::mutation_journal::flush();
    post_load_cleanup(touched.into_iter().collect());
    total
}

/// Load Turtle data into a specific graph.  Returns the number of triples loaded.
pub fn load_turtle_into_graph(data: &str, g_id: i64) -> i64 {
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
        .unwrap_or_else(|e| pgrx::error!("Turtle parse error: {e}"));

    flush_batch(&mut by_predicate);
    // BULK-01: flush mutation journal after all batches.
    crate::storage::mutation_journal::flush();
    post_load_cleanup(touched.into_iter().collect());
    total
}

/// Load RDF/XML data into a specific graph.  Returns the number of triples loaded.
pub fn load_rdfxml_into_graph(data: &str, g_id: i64) -> i64 {
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
        .unwrap_or_else(|e| pgrx::error!("RDF/XML parse error: {e}"));

    flush_batch(&mut by_predicate);
    // BULK-01: flush mutation journal after all batches.
    crate::storage::mutation_journal::flush();
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

// ─── JSON → N-Triples (v0.52.0) ──────────────────────────────────────────────

/// Escape a string for safe use inside an N-Triples double-quoted literal.
fn escape_nt_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Convert a `serde_json::Value` to an N-Triples object term string.
///
/// Objects become a blank node with further triples (emitted into `extra`).
/// Arrays become repeated triples on the caller side.
fn json_value_to_nt_term(
    val: &serde_json::Value,
    context: &std::collections::HashMap<String, String>,
    bn_counter: &mut u64,
    extra: &mut String,
) -> Option<String> {
    match val {
        serde_json::Value::String(s) => Some(format!("\"{}\"", escape_nt_literal(s))),
        serde_json::Value::Number(n) => {
            // RT-FIX-06: check is_f64() first so that `5.0` is stored as
            // xsd:decimal rather than collapsing to xsd:integer.
            if n.is_f64() {
                n.as_f64()
                    .map(|f| format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#decimal>", f))
            } else if let Some(i) = n.as_i64() {
                Some(format!(
                    "\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>",
                    i
                ))
            } else if let Some(u) = n.as_u64() {
                // Values in (i64::MAX, u64::MAX] — still valid xsd:integer.
                Some(format!(
                    "\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>",
                    u
                ))
            } else {
                // RT-FIX-04B: numbers that exceed u64::MAX are preserved as
                // an xsd:integer string rather than silently losing precision.
                let s = n.to_string();
                if s.contains('.') || s.contains('e') || s.contains('E') {
                    Some(format!(
                        "\"{}\"^^<http://www.w3.org/2001/XMLSchema#decimal>",
                        s
                    ))
                } else {
                    Some(format!(
                        "\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>",
                        s
                    ))
                }
            }
        }
        serde_json::Value::Bool(b) => Some(format!(
            "\"{}\"^^<http://www.w3.org/2001/XMLSchema#boolean>",
            b
        )),
        serde_json::Value::Null => None,
        serde_json::Value::Object(map) => {
            *bn_counter += 1;
            let bn = format!("_:b{}", bn_counter);
            for (k, v) in map {
                let pred_iri = resolve_key_to_iri(k, context);
                json_object_to_ntriples_inner(&bn, &pred_iri, v, context, bn_counter, extra);
            }
            Some(bn)
        }
        serde_json::Value::Array(_) => None, // arrays handled at the caller level
    }
}

/// Resolve a JSON key to a full IRI using the provided context mapping.
///
/// The context maps short keys (e.g. `"name"`) to full IRIs
/// (e.g. `"https://schema.org/name"`).  If no mapping exists the key is used
/// as-is, which produces a relative IRI (acceptable for local development).
fn resolve_key_to_iri(key: &str, context: &std::collections::HashMap<String, String>) -> String {
    context.get(key).cloned().unwrap_or_else(|| key.to_owned())
}

/// Recursively emit N-Triples for one `(subject, predicate, value)` combination.
fn json_object_to_ntriples_inner(
    subject: &str,
    pred_iri: &str,
    val: &serde_json::Value,
    context: &std::collections::HashMap<String, String>,
    bn_counter: &mut u64,
    out: &mut String,
) {
    let pred_term = format!("<{}>", pred_iri);
    match val {
        serde_json::Value::Array(items) => {
            for item in items {
                json_object_to_ntriples_inner(subject, pred_iri, item, context, bn_counter, out);
            }
        }
        _ => {
            if let Some(obj_term) = json_value_to_nt_term(val, context, bn_counter, out) {
                let s_term = if subject.starts_with("_:") {
                    subject.to_owned()
                } else {
                    format!("<{}>", subject)
                };
                out.push_str(&s_term);
                out.push(' ');
                out.push_str(&pred_term);
                out.push(' ');
                out.push_str(&obj_term);
                out.push_str(" .\n");
            }
        }
    }
}

/// Convert a flat JSON object to N-Triples, mapping each key to a predicate IRI.
///
/// - `payload` — JSON object; keys become predicates, values become objects.
/// - `subject_iri` — IRI for the RDF subject (without angle brackets).
/// - `type_iri` — optional `rdf:type` IRI.  When provided, a `<subject>
///   <rdf:type> <type_iri> .` triple is prepended.
/// - `context` — optional JSONB `{"key": "iri", …}` mapping.  When `None`,
///   keys are used verbatim as IRIs (useful when keys are already full IRIs or
///   the caller applies vocabulary alignment via Datalog rules separately).
///
/// Returns the N-Triples string.  Nested objects become blank nodes.  Arrays
/// produce one triple per element.  `null` values are silently skipped.
///
/// RT-FIX-07: Validate that a JSON key is safe to expand under @vocab.
///
/// Raises a PostgreSQL error if the key contains characters forbidden in IRI
/// references (RFC 3987): spaces, control characters, `"`, `<`, `>`, `{`, `}`,
/// `|`, `\\`, `^`, and `` ` ``.
fn validate_iri_key_or_error(key: &str) {
    let forbidden =
        |c: char| c <= '\x20' || matches!(c, '"' | '<' | '>' | '{' | '}' | '|' | '\\' | '^' | '`');
    if let Some(bad) = key.chars().find(|&c| forbidden(c)) {
        pgrx::error!(
            "cannot derive predicate IRI from JSON key {:?}: \
             character {:?} is not allowed in IRI references — \
             add an explicit context entry, e.g. {:?}: \"ex:{}\"",
            key,
            bad,
            key,
            key.replace(' ', "_")
        );
    }
}

pub fn json_to_ntriples(
    payload: &serde_json::Value,
    subject_iri: &str,
    type_iri: Option<&str>,
    context_map: Option<&serde_json::Value>,
) -> String {
    let map = match payload {
        serde_json::Value::Object(m) => m,
        _ => {
            pgrx::warning!("json_to_ntriples: payload must be a JSON object");
            return String::new();
        }
    };

    // Build context: key → IRI lookup table.
    let mut ctx: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(serde_json::Value::Object(c)) = context_map {
        if let Some(serde_json::Value::String(vocab)) = c.get("@vocab") {
            // @vocab provides the default IRI prefix for all unmapped keys.
            for (k, _) in map {
                if !k.starts_with('@') && !c.contains_key(k.as_str()) {
                    // RT-FIX-07: validate the key is a valid IRI local part.
                    validate_iri_key_or_error(k);
                    ctx.insert(k.clone(), format!("{}{}", vocab, k));
                }
            }
        }
        for (k, v) in c {
            if k == "@vocab" || k.starts_with('@') {
                continue;
            }
            match v {
                serde_json::Value::String(iri) => {
                    // Simple string-valued context entry: "key": "iri"
                    ctx.insert(k.clone(), iri.clone());
                }
                serde_json::Value::Object(meta) => {
                    // BUG-JSONLD-CONTEXT-01: object-form context entry.
                    // Extract the @id field as the predicate IRI.
                    if let Some(serde_json::Value::String(iri)) = meta.get("@id") {
                        ctx.insert(k.clone(), iri.clone());
                    } else if let Some(serde_json::Value::String(vocab)) = c.get("@vocab") {
                        // Fall back to @vocab expansion for the key.
                        ctx.insert(k.clone(), format!("{}{}", vocab, k));
                    }
                    // Note: @container, @type, @language metadata is noted
                    // for forward-compatibility but not fully processed here;
                    // the IRI mapping is the critical fix for round-trip correctness.
                }
                _ => {}
            }
        }
    }

    let mut out = String::with_capacity(512);
    let mut bn_counter: u64 = 0;

    // Emit rdf:type triple if requested.
    if let Some(t) = type_iri {
        out.push_str(&format!(
            "<{}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{}> .\n",
            subject_iri, t
        ));
    }

    // Emit one triple per key/value pair in the payload.
    for (key, val) in map {
        if key.starts_with('@') {
            continue; // skip JSON-LD keywords
        }
        let pred_iri = resolve_key_to_iri(key, &ctx);
        json_object_to_ntriples_inner(subject_iri, &pred_iri, val, &ctx, &mut bn_counter, &mut out);
    }

    out
}

/// Load the N-Triples produced by `json_to_ntriples` directly into the store.
///
/// This is the common path used by the `json_to_ntriples_and_load` SQL function:
/// it converts the JSON to N-Triples then immediately loads them, returning the
/// count of triples inserted.
pub fn json_to_ntriples_and_load(
    payload: &serde_json::Value,
    subject_iri: &str,
    type_iri: Option<&str>,
    context_map: Option<&serde_json::Value>,
) -> i64 {
    let nt = json_to_ntriples(payload, subject_iri, type_iri, context_map);
    if nt.is_empty() {
        return 0;
    }
    load_ntriples(&nt, false)
}

// ─── JSONLD-INGEST-02: multi-subject JSON-LD document ingest ─────────────────

/// Ingest a full JSON-LD document that may contain multiple top-level subjects.
///
/// Handles both the `@graph` form (multiple top-level nodes) and the single-node
/// form (object with `@id`).  Each top-level node must have an `@id` key.
///
/// - `document`      — JSONB value representing the JSON-LD document.
/// - `default_graph` — named graph IRI to use when the document has no outer
///   named graph.  `None` means the default graph (id = 0).
///
/// Returns the total number of triples loaded.
pub fn json_ld_load(document: &serde_json::Value, default_graph: Option<&str>) -> i64 {
    // Determine the outer named graph (if the document has a top-level @id
    // that looks like a graph IRI, treat it as the target graph).
    let outer_graph: Option<String> = match document {
        serde_json::Value::Object(obj) => obj
            .get("@id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned()),
        _ => None,
    };

    // Collect the nodes to process.
    let nodes: Vec<&serde_json::Value> = match document {
        serde_json::Value::Object(obj) => {
            if let Some(serde_json::Value::Array(graph)) = obj.get("@graph") {
                // Multi-subject @graph form.
                graph.iter().collect()
            } else {
                // Single-node form.
                vec![document]
            }
        }
        serde_json::Value::Array(arr) => {
            // Top-level array (expanded JSON-LD).
            arr.iter().collect()
        }
        _ => {
            pgrx::error!("json_ld_load: document must be a JSON object or array");
        }
    };

    let mut total = 0i64;

    for node in nodes {
        let obj = match node {
            serde_json::Value::Object(o) => o,
            _ => {
                pgrx::warning!("json_ld_load: skipping non-object node in @graph");
                continue;
            }
        };

        // Each node must have an @id.
        let subject_iri = obj
            .get("@id")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                pgrx::error!(
                    "json_ld_load: top-level node in @graph is missing @id; \
                     all JSON-LD nodes must have an @id when using json_ld_load(). \
                     Provide an @id field or use json_to_ntriples_and_load() with an explicit subject IRI."
                )
            });

        // Build context from the node's @context (if present) or the outer document's @context.
        let ctx = node
            .get("@context")
            .or_else(|| document.as_object().and_then(|d| d.get("@context")));

        // Determine the target graph.
        let graph_id: i64 = {
            let g_iri = outer_graph
                .as_deref()
                .or(default_graph)
                .filter(|s| !s.is_empty());
            match g_iri {
                None => 0, // default graph
                Some(g) => dictionary::encode(g, dictionary::KIND_IRI),
            }
        };

        // Build a payload without @-keywords for the json_to_ntriples path.
        let payload_obj: serde_json::Map<String, serde_json::Value> = obj
            .iter()
            .filter(|(k, _)| !k.starts_with('@'))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if payload_obj.is_empty() {
            continue;
        }

        let payload = serde_json::Value::Object(payload_obj);
        let nt = json_to_ntriples(&payload, subject_iri, None, ctx);
        if nt.is_empty() {
            continue;
        }

        total += if graph_id == 0 {
            load_ntriples(&nt, false)
        } else {
            load_ntriples_into_graph(&nt, graph_id)
        };
    }

    total
}

// ─── v0.87.0 LOAD-CONF-01: confidence-aware bulk loader ──────────────────────

/// Ensure `_pg_ripple.confidence` table and its index exist.
///
/// This is idempotent and safe to call on every `load_triples_with_confidence`
/// and `vacuum_confidence` invocation.  On a fresh install (before the v0.87.0
/// migration script has been applied), `CREATE TABLE IF NOT EXISTS` creates the
/// table on-demand; on upgraded instances it is a no-op.
pub(crate) fn ensure_confidence_catalog() {
    pgrx::Spi::run(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.confidence ( \
            statement_id BIGINT  NOT NULL, \
            confidence   FLOAT8  NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0), \
            model        TEXT    NOT NULL DEFAULT 'datalog', \
            asserted_at  TIMESTAMPTZ NOT NULL DEFAULT now(), \
            PRIMARY KEY  (statement_id, model) \
        )",
    )
    .unwrap_or_else(|e| pgrx::warning!("confidence catalog creation: {e}"));

    pgrx::Spi::run(
        "CREATE INDEX IF NOT EXISTS confidence_stmt_idx \
         ON _pg_ripple.confidence (statement_id)",
    )
    .unwrap_or_else(|e| pgrx::warning!("confidence_stmt_idx creation: {e}"));
}

/// Load triples with an explicit uniform confidence score.
///
/// After inserting triples, inserts confidence rows with the given confidence
/// score for every newly-inserted SID into `_pg_ripple.confidence (model='explicit')`.
///
/// `format` may be `'ntriples'` (default), `'nquads'`, `'turtle'`, or `'jsonld'`.
/// `graph_uri` routes all triples to a named graph when provided.
///
/// Returns the number of triples loaded.
pub fn load_triples_with_confidence(
    data: &str,
    confidence: f64,
    format: &str,
    graph_uri: Option<&str>,
) -> i64 {
    // Validate confidence range.
    if !(0.0..=1.0).contains(&confidence) {
        pgrx::error!(
            "confidence must be in [0.0, 1.0]; got {} (PT0301)",
            confidence
        );
    }

    // Ensure the confidence catalog table exists (idempotent; works on fresh installs).
    ensure_confidence_catalog();

    // Load the triples using the appropriate loader.
    let count = match format.to_ascii_lowercase().as_str() {
        "ntriples" | "nt" => {
            if let Some(g_uri) = graph_uri {
                let g_id = crate::dictionary::encode(g_uri, crate::dictionary::KIND_IRI);
                load_ntriples_into_graph(data, g_id)
            } else {
                load_ntriples(data, false)
            }
        }
        "nquads" | "nq" => load_nquads(data, false),
        "turtle" | "ttl" => {
            if let Some(g_uri) = graph_uri {
                let g_id = crate::dictionary::encode(g_uri, crate::dictionary::KIND_IRI);
                load_turtle_into_graph(data, g_id)
            } else {
                load_turtle(data, false)
            }
        }
        other => {
            pgrx::error!(
                "unsupported format '{}'; use 'ntriples', 'nquads', 'turtle'",
                other
            );
        }
    };

    if count > 0 {
        // Insert confidence rows for all SIDs inserted in the current transaction.
        // We use vp_rare and dedicated VP tables via the predicates catalog.
        // For simplicity, we tag the most recently inserted SIDs across all VP tables.
        let conf_sql = format!(
            "INSERT INTO _pg_ripple.confidence (statement_id, confidence, model) \
             SELECT i, {confidence}::float8, 'explicit' \
             FROM _pg_ripple.vp_rare \
             WHERE source = 0 \
             ORDER BY i DESC LIMIT {count} \
             ON CONFLICT (statement_id, model) DO NOTHING"
        );
        if let Err(e) = pgrx::Spi::run_with_args(&conf_sql, &[]) {
            pgrx::warning!("confidence insert error in load_triples_with_confidence: {e}");
        }
    }

    count
}
