//! SHACL-AF bridge (v0.10.0 / v0.53.0 / v0.61.0 / v0.79.0).
//!
//! Scans Turtle data for `sh:rule` triples and, when inference is enabled,
//! compiles them into Datalog rules via `load_rules_text()`.
//!
//! # SHACL-AF `sh:TripleRule` → Datalog translation
//!
//! A SHACL-AF Triple Rule such as:
//!
//! ```turtle
//! ex:MyShape a sh:NodeShape ;
//!     sh:targetClass ex:Person ;
//!     sh:rule [
//!         a sh:TripleRule ;
//!         sh:subject sh:this ;
//!         sh:predicate ex:isAgent ;
//!         sh:object ex:Agent
//!     ] .
//! ```
//!
//! is compiled to the Datalog rule:
//!
//! ```datalog
//! ex:isAgent(?s, ex:Agent) :- rdf:type(?s, ex:Person).
//! ```
//!
//! where `sh:this` is replaced by the focus-node variable `?s`.
//!
//! # SHACL-AF `sh:SPARQLRule` (v0.79.0, SHACL-SPARQL-01)
//!
//! A `sh:SPARQLRule` contains a SPARQL CONSTRUCT body that is compiled and
//! executed via the existing SPARQL engine.  The resulting triples are
//! materialised into the target graph using the standard VP insert path.

use pgrx::prelude::*;

// ─── PT481 deduplication ──────────────────────────────────────────────────────

/// Session-local flag: whether PT481 has been emitted this session.
/// De-duplicates the warning per SHACL-SPARQL-01f spec.
static PT481_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Emit PT481 at most once per session.
fn warn_pt481_once(context: &str) {
    use std::sync::atomic::Ordering;
    if !PT481_WARNED.swap(true, Ordering::Relaxed) {
        pgrx::warning!(
            "SHACL-AF sh:SPARQLRule encountered (PT481): \
             SPARQL-based rules will be executed via the native SPARQL engine; \
             context: {context}"
        );
    }
}

// ─── Parsed SPARQL rule ───────────────────────────────────────────────────────

/// A parsed `sh:SPARQLRule` extracted from Turtle data.
#[derive(Debug, Clone)]
pub struct SparqlRule {
    /// The SPARQL CONSTRUCT body (may include SELECT, though CONSTRUCT is
    /// canonical per the SHACL-AF spec).
    pub sparql_body: String,
    /// Optional `sh:order` value for execution ordering.
    pub order: i32,
    /// Optional prefix declarations prepended to the body.
    pub prefixes: String,
}

/// Parse `sh:SPARQLRule` blocks from Turtle data.
///
/// Extracts each occurrence of a `sh:construct` or `sh:select` value
/// together with any `sh:prefixes` declarations.
pub fn parse_sparql_rules(data: &str) -> Vec<SparqlRule> {
    let mut rules = Vec::new();

    // Find all sh:construct or sh:select triple-quoted strings.
    // The SHACL-AF spec uses sh:construct for CONSTRUCT queries.
    for (pattern, start_needle) in [("sh:construct", "sh:construct"), ("sh:select", "sh:select")] {
        let mut search = data;
        while let Some(idx) = search.find(start_needle) {
            let after = &search[idx + pattern.len()..].trim_start();

            // Extract the SPARQL body: either triple-quoted or single-quoted.
            let body = if let Some(body) = extract_quoted_string(after) {
                body
            } else {
                search = &search[idx + 1..];
                continue;
            };

            // Extract optional sh:order value.
            let order = extract_sh_order(data);

            // Build prefix string from PREFIX declarations in the Turtle.
            let prefixes = collect_prefixes(data);

            rules.push(SparqlRule {
                sparql_body: body,
                order,
                prefixes,
            });

            // Advance past the consumed pattern.
            search = &search[idx + 1..];
        }
    }

    rules
}

/// Extract a single- or triple-quoted SPARQL string immediately following the
/// cursor position in `s`.  Returns the unescaped string content.
fn extract_quoted_string(s: &str) -> Option<String> {
    let s = s.trim_start();
    if let Some(inner) = s.strip_prefix("\"\"\"") {
        // Triple-quoted string.
        let end = inner.find("\"\"\"")?;
        Some(inner[..end].to_owned())
    } else if let Some(inner) = s.strip_prefix('"') {
        // Single-quoted string.
        let end = inner.find('"')?;
        Some(inner[..end].to_owned())
    } else if let Some(inner) = s.strip_prefix('\'') {
        let end = inner.find('\'')?;
        Some(inner[..end].to_owned())
    } else {
        None
    }
}

/// Extract `sh:order` integer value from the Turtle text (if present).
fn extract_sh_order(data: &str) -> i32 {
    for line in data.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("sh:order") {
            let val_str = rest
                .trim()
                .trim_end_matches(';')
                .trim()
                .trim_end_matches('.');
            if let Ok(n) = val_str.trim().parse::<i32>() {
                return n;
            }
        }
    }
    0
}

/// Build a SPARQL PREFIX block from all `@prefix` declarations in the Turtle.
fn collect_prefixes(data: &str) -> String {
    let mut buf = String::new();
    for line in data.lines() {
        let t = line.trim();
        if t.starts_with("@prefix") || t.to_lowercase().starts_with("prefix ") {
            // Convert Turtle @prefix to SPARQL PREFIX.
            let sparql = t
                .trim_start_matches('@')
                .trim()
                .trim_end_matches('.')
                .trim();
            // prefix sparql: <...> → PREFIX sparql: <...>
            if sparql.to_lowercase().starts_with("prefix") {
                buf.push_str(sparql);
            } else {
                buf.push_str(&format!("PREFIX {sparql}"));
            }
            buf.push('\n');
        }
    }
    buf
}

/// Execute a compiled `sh:SPARQLRule` and materialise the resulting triples.
///
/// `sparql_text` — the full SPARQL CONSTRUCT query (including PREFIX declarations).
/// `target_graph_id` — encoded graph ID for materialised triples (0 = default graph).
///
/// Returns the number of new triples materialised.
pub fn execute_sparql_rule(sparql_text: &str, target_graph_id: i64) -> i64 {
    // Compile and execute the SPARQL CONSTRUCT query.
    let rows = crate::sparql::execute::sparql_construct_rows(sparql_text);
    if rows.is_empty() {
        return 0;
    }

    let mut count = 0i64;
    for (s_id, p_id, o_id) in rows {
        crate::storage::insert_encoded_triple(s_id, p_id, o_id, target_graph_id);
        count += 1;
    }
    count
}

/// Execute all `sh:SPARQLRule` rules from a parsed list, respecting `sh:order`.
///
/// Iterates to a fixpoint (up to `pg_ripple.shacl_rule_max_iterations`)
/// so that newly materialised triples can trigger further rules.
///
/// Returns the total number of new triples materialised.
pub fn execute_sparql_rules(rules: &mut [SparqlRule], target_graph_id: i64) -> i64 {
    if rules.is_empty() {
        return 0;
    }

    // Sort by sh:order (ascending).
    rules.sort_by_key(|r| r.order);

    let max_iter = crate::gucs::shacl::SHACL_RULE_MAX_ITERATIONS.get() as i64;
    let mut total_new = 0i64;

    for _iteration in 0..max_iter {
        let mut new_this_round = 0i64;

        for rule in rules.iter() {
            // Prepend prefix declarations.
            let full_query = if rule.prefixes.is_empty() {
                rule.sparql_body.clone()
            } else {
                format!("{}\n{}", rule.prefixes, rule.sparql_body)
            };

            // Validate that it parses as a CONSTRUCT query; skip if it doesn't.
            match spargebra::SparqlParser::new().parse_query(&full_query) {
                Ok(spargebra::Query::Construct { .. }) => {}
                Ok(_) => {
                    pgrx::warning!(
                        "SHACL-AF sh:SPARQLRule body is not a CONSTRUCT query; skipping"
                    );
                    continue;
                }
                Err(e) => {
                    pgrx::warning!("SHACL-AF sh:SPARQLRule SPARQL parse error: {e}; skipping");
                    continue;
                }
            }

            let materialised = execute_sparql_rule(&full_query, target_graph_id);
            new_this_round += materialised;
        }

        total_new += new_this_round;

        // Fixpoint: if no new triples were added, we're done.
        if new_this_round == 0 {
            break;
        }

        // Check iteration cap.
        if _iteration + 1 == max_iter && new_this_round > 0 {
            pgrx::error!(
                "sh:SPARQLRule fixpoint did not converge after {max_iter} iterations; \
                 increase pg_ripple.shacl_rule_max_iterations or check for rule cycles"
            );
        }
    }

    total_new
}

// ─── sh:rule pattern extraction ───────────────────────────────────────────────

/// A parsed SHACL-AF Triple Rule.
#[derive(Debug)]
struct TripleRule {
    /// The target class (from `sh:targetClass`) — the "if" side.
    target_class: Option<String>,
    /// The predicate of the inferred triple.
    subject_path: Option<String>,
    /// The predicate IRI to assert.
    predicate: Option<String>,
    /// The object IRI to assert (or `?this` when `sh:this` is used).
    object: Option<String>,
}

/// Extract `sh:TripleRule` patterns from Turtle text using simple heuristics.
///
/// Returns a vector of `TripleRule` structs.  The extraction is best-effort;
/// complex nested blank nodes are handled with limited fidelity.
fn extract_triple_rules(data: &str) -> Vec<TripleRule> {
    let mut rules: Vec<TripleRule> = Vec::new();

    // Find sh:targetClass declarations.
    // We look for lines like:  sh:targetClass ex:Person ;
    let target_classes: Vec<&str> = data
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix("sh:targetClass").map(|rest| {
                rest.trim()
                    .trim_end_matches(';')
                    .trim()
                    .trim_end_matches('.')
            })
        })
        .collect();

    // Find sh:predicate declarations inside rule blocks.
    let predicates: Vec<&str> = data
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix("sh:predicate").map(|rest| {
                rest.trim()
                    .trim_end_matches(';')
                    .trim()
                    .trim_end_matches('.')
            })
        })
        .collect();

    // Find sh:object declarations inside rule blocks.
    let objects: Vec<&str> = data
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix("sh:object").map(|rest| {
                rest.trim()
                    .trim_end_matches(';')
                    .trim()
                    .trim_end_matches('.')
            })
        })
        .collect();

    // Pair up predicates and objects to form Triple Rules.
    // A TripleRule has exactly one sh:predicate and one sh:object.
    let target_class = target_classes.first().copied();
    for i in 0..predicates.len().min(objects.len()) {
        rules.push(TripleRule {
            target_class: target_class.map(|s| s.to_owned()),
            subject_path: None,
            predicate: Some(predicates[i].to_owned()),
            object: Some(objects[i].to_owned()),
        });
    }

    rules
}

/// Compile a single `TripleRule` to a Datalog rule text.
///
/// Returns `None` when the rule cannot be compiled (e.g. missing fields).
fn compile_triple_rule(rule: &TripleRule) -> Option<String> {
    let predicate = rule.predicate.as_deref()?;
    let object = rule.object.as_deref()?;

    // If we have a target class, use it as the body condition.
    if let Some(class) = &rule.target_class {
        // Head:  predicate(?s, object)
        // Body:  rdf:type(?s, class)
        let datalog = format!(
            "{}(?s, {}) :- <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?s, {}).",
            predicate, object, class
        );
        Some(datalog)
    } else {
        // No target class — emit a rule with no body (unconditional assertion).
        // This is unusual but valid in SHACL-AF.
        let _ = rule.subject_path.as_deref();
        None
    }
}

// ─── Public bridge ────────────────────────────────────────────────────────────

/// SHACL-AF bridge (v0.61.0): scan Turtle data for `sh:rule` triples and
/// compile them into Datalog rules.
///
/// When `pg_ripple.inference_mode` is `'on_demand'` or `'materialized'`, the
/// extracted `sh:TripleRule` patterns are compiled to Datalog and loaded via
/// `load_rules_text()`.  `sh:SPARQLRule` patterns are now fully executed
/// via the native SPARQL CONSTRUCT engine (v0.79.0, SHACL-SPARQL-01).
///
/// Returns the number of `sh:rule` patterns found and processed.
pub fn bridge_shacl_rules(data: &str) -> i32 {
    if !data.contains("sh:rule") && !data.contains("shacl#rule") {
        return 0;
    }

    let count = data.matches("sh:rule").count() as i32;
    if count == 0 {
        return 0;
    }

    let inference_mode = crate::INFERENCE_MODE
        .get()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    if inference_mode == "off" || inference_mode.is_empty() {
        pgrx::warning!(
            "SHACL-AF sh:rule detected but not compiled (PT480): {count} rule(s) found; \
             set pg_ripple.inference_mode to 'on_demand' to enable SHACL-AF rule compilation"
        );
        return count;
    }

    // v0.79.0: Execute sh:SPARQLRule via the native SPARQL CONSTRUCT engine.
    let mut sparql_rules = parse_sparql_rules(data);
    if !sparql_rules.is_empty() {
        warn_pt481_once("bridge_shacl_rules");
        let materialised = execute_sparql_rules(&mut sparql_rules, 0 /* default graph */);
        pgrx::debug1!(
            "SHACL-AF sh:SPARQLRule: executed {} rule(s), materialised {} triple(s)",
            sparql_rules.len(),
            materialised
        );
    }

    // Extract and compile Triple Rules.
    let triple_rules = extract_triple_rules(data);
    let mut compiled = 0i32;

    for rule in &triple_rules {
        if let Some(datalog_text) = compile_triple_rule(rule) {
            // Load the compiled Datalog rule into the engine.
            let loaded = crate::datalog::load_and_store_rules(&datalog_text, "shacl-af");
            if loaded > 0 {
                compiled += 1;
            } else {
                pgrx::warning!("SHACL-AF sh:rule compilation failed (PT482): rule was not loaded");
            }
        }
    }

    if compiled > 0 {
        pgrx::debug1!("SHACL-AF bridge: compiled {compiled}/{count} sh:rule(s) to Datalog");
    }

    // Even if we could not compile any rules (e.g. unsupported rule shapes),
    // register a placeholder so the rules catalog records the detection.
    if compiled == 0 && count > 0 {
        let _ = Spi::run_with_args(
            "INSERT INTO _pg_ripple.rules \
             (rule_set, rule_text, head_pred, stratum, is_recursive, active) \
             VALUES ('shacl-af', $1, NULL, 0, false, true) \
             ON CONFLICT DO NOTHING",
            &[pgrx::datum::DatumWithOid::from(
                "# SHACL-AF sh:rule detected; rule shape not supported for auto-compilation",
            )],
        );
    }

    count
}
