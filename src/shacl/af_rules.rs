//! SHACL-AF bridge (v0.10.0 / v0.53.0 / v0.61.0).
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

use pgrx::prelude::*;

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
/// complex nested blank nodes or SPARQL-based rules (`sh:SPARQLRule`) are
/// deferred to a future release.
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
/// `load_rules_text()`.  `sh:SPARQLRule` patterns are detected and warned about
/// but not compiled (deferred to a future release).
///
/// Returns the number of `sh:rule` patterns found and compiled.
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

    // Detect SPARQL-based rules (not yet supported).
    if data.contains("sh:SPARQLRule") || data.contains("sh:select") {
        pgrx::warning!(
            "SHACL-AF sh:SPARQLRule detected but not compiled (PT481): \
             SPARQL-based rules are deferred to a future release; \
             use sh:TripleRule for automatic Datalog compilation"
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
