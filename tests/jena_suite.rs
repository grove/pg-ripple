//! Apache Jena SPARQL test suite for pg_ripple.
//!
//! Runs ~1 000 tests across Jena's sparql-query, sparql-update,
//! sparql-syntax, and algebra sub-suites, reusing the pg_ripple test
//! infrastructure from the W3C suite.
//!
//! # Running locally
//!
//! ```sh
//! # With Jena test data in tests/jena/data/:
//! cargo test --test jena_suite
//!
//! # Or point to a custom directory:
//! JENA_TEST_DIR=/tmp/jena cargo test --test jena_suite
//!
//! # Download the Jena test suite first:
//! bash scripts/fetch_conformance_tests.sh --jena
//! ```
//!
//! Tests skip gracefully when neither the test data nor a pg_ripple database
//! is reachable.
//!
//! # CI job
//!
//! The `jena-suite` CI job is non-blocking until pass rate ≥ 95%.
//! See `.github/workflows/test.yml`.

#[path = "jena/mod.rs"]
mod jena;

#[path = "w3c/loader.rs"]
mod loader;

#[path = "conformance/mod.rs"]
mod conformance;

use std::io::Write;

use conformance::runner::{RunConfig, TestEntry, run_entries};
use jena::manifest::{JenaTestCase, JenaTestType};

// ── Known sub-suites ──────────────────────────────────────────────────────────

const CATEGORIES: &[&str] = &["sparql-query", "sparql-update", "sparql-syntax", "algebra"];

// ── Main test ─────────────────────────────────────────────────────────────────

#[test]
fn jena_suite() {
    // ── Pre-conditions ──────────────────────────────────────────────────────
    let data_dir = match jena::test_data_dir() {
        Some(d) => d,
        None => {
            println!("SKIP: Jena test data directory not found.");
            println!("      Run scripts/fetch_conformance_tests.sh --jena or set JENA_TEST_DIR.");
            return;
        }
    };

    let db_url = jena::db_connect_string();
    // Note: we do NOT skip the whole suite when DB is unavailable.
    // Syntax tests (PositiveSyntax / NegativeSyntax) run purely in-process via
    // spargebra and do not need a database.  Evaluation tests skip gracefully
    // per-test when the connection fails.

    // ── Build known failures set ────────────────────────────────────────────
    let project_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let kf_path = project_root
        .join("tests")
        .join("conformance")
        .join("known_failures.txt");
    let known_failures = conformance::load_known_failures(&kf_path, "jena");

    // ── Collect test entries from manifests ─────────────────────────────────
    let threads: usize = std::env::var("JENA_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);

    let mut entries: Vec<TestEntry> = Vec::new();

    for category in CATEGORIES {
        let manifest_path = data_dir.join(category).join("manifest.ttl");
        if !manifest_path.exists() {
            eprintln!("[jena] manifest not found for category '{category}', skipping");
            continue;
        }

        let cases = match jena::manifest::parse_manifest(&manifest_path, category) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[jena] manifest parse error for '{category}': {e}");
                continue;
            }
        };

        for case in cases {
            let db_url = db_url.clone();
            let data_dir = data_dir.clone();
            entries.push(build_entry(case, db_url, data_dir));
        }
    }

    if entries.is_empty() {
        println!("SKIP: No Jena test cases found in {}", data_dir.display());
        return;
    }

    let total_expected = entries.len();

    // Pre-test setup: ensure pg_ripple is at the current version.
    // DROP + CREATE is used instead of ALTER EXTENSION UPDATE to guarantee a
    // clean schema regardless of what was previously installed in this DB.
    if let Ok(mut setup) = postgres::Client::connect(&db_url, postgres::NoTls) {
        let _ = setup.batch_execute(
            "DROP EXTENSION IF EXISTS pg_ripple CASCADE; CREATE EXTENSION pg_ripple CASCADE",
        );
    }

    // ── Run the suite ───────────────────────────────────────────────────────
    let config = RunConfig {
        threads,
        timeout_secs: 5,
        max_tests: None,
        known_failures,
        suite: "jena".into(),
    };

    let start = std::time::Instant::now();
    let report = run_entries(entries, &config);
    let elapsed = start.elapsed();

    // ── Print report ────────────────────────────────────────────────────────
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "\n{}", report.summary()).ok();
    writeln!(out, "  elapsed: {:.1}s", elapsed.as_secs_f64()).ok();

    // Print failures.
    for r in &report.results {
        if r.outcome.is_unexpected_failure() {
            writeln!(out, "  FAIL  [{}ms]  {}", r.duration_ms, r.name).ok();
            if let conformance::runner::TestOutcome::Fail(msg) = &r.outcome {
                writeln!(out, "        {msg}").ok();
            }
        }
    }

    // ── Write unified report ────────────────────────────────────────────────
    let report_path = project_root
        .join("tests")
        .join("conformance")
        .join("report.json");
    conformance::report::write_report(&[&report], &report_path).ok();

    // ── Exit criteria ───────────────────────────────────────────────────────
    // All embedded fixtures must pass; unexpected failures are a hard error.
    if !report.is_clean() {
        panic!(
            "Jena suite: unexpected test failures.\n{}",
            report.summary()
        );
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

fn build_entry(case: JenaTestCase, db_url: String, _data_dir: std::path::PathBuf) -> TestEntry {
    let key = case.iri.clone();
    let name = case.name.clone();

    let run: Box<dyn FnOnce() -> Result<(), String> + Send + 'static> = Box::new(move || {
        match case.test_type {
            JenaTestType::NotClassified => {
                return Err(format!("SKIP: unrecognised test type for '{}'", case.iri));
            }
            JenaTestType::PositiveSyntax => {
                let query_path = case.query_file.as_ref().ok_or_else(|| {
                    format!(
                        "SKIP: no query file for positive-syntax test '{}'",
                        case.iri
                    )
                })?;
                let src = std::fs::read_to_string(query_path).map_err(|e| {
                    format!("SKIP: reading query file {}: {e}", query_path.display())
                })?;
                spargebra::SparqlParser::new()
                    .parse_query(&src)
                    .map(|_| ())
                    .or_else(|_| {
                        spargebra::SparqlParser::new()
                            .parse_update(&src)
                            .map(|_| ())
                    })
                    .map_err(|e| format!("syntax error (expected none): {e}"))?;
                return Ok(());
            }
            JenaTestType::NegativeSyntax => {
                let query_path = case.query_file.as_ref().ok_or_else(|| {
                    format!(
                        "SKIP: no query file for negative-syntax test '{}'",
                        case.iri
                    )
                })?;
                let src = std::fs::read_to_string(query_path).map_err(|e| {
                    format!("SKIP: reading query file {}: {e}", query_path.display())
                })?;
                let parser = spargebra::SparqlParser::new();
                let ok = parser.parse_query(&src).is_ok()
                    || spargebra::SparqlParser::new().parse_update(&src).is_ok();
                if ok {
                    return Err(format!(
                        "expected parse error but query parsed successfully"
                    ));
                }
                return Ok(());
            }
            JenaTestType::QueryEvaluation | JenaTestType::UpdateEvaluation => {
                // Full evaluation requires a live DB.
                let mut client = postgres::Client::connect(&db_url, postgres::NoTls)
                    .map_err(|e| format!("SKIP: DB connect failed: {e}"))?;

                run_evaluation_test(&mut client, &case)
            }
        }
    });

    TestEntry { key, name, run }
}

/// Run a single query/update evaluation test against pg_ripple.
/// Extension setup (DROP + CREATE) is handled once in `jena_suite()` before
/// any parallel test closures execute.
fn run_evaluation_test(client: &mut postgres::Client, case: &JenaTestCase) -> Result<(), String> {
    let query_path = case
        .query_file
        .as_ref()
        .ok_or_else(|| format!("SKIP: no query file for test '{}'", case.iri))?;

    let query_src = std::fs::read_to_string(query_path)
        .map_err(|e| format!("SKIP: reading query {}: {e}", query_path.display()))?;

    let mut tx = client
        .transaction()
        .map_err(|e| format!("SKIP: begin transaction: {e}"))?;

    // Load data files.
    for data_file in &case.data_files {
        loader::load_default_graph(&mut tx, data_file)
            .map_err(|e| format!("loading data {}: {e}", data_file.display()))?;
    }
    for (graph_iri, data_file) in &case.named_graphs {
        loader::load_named_graph(&mut tx, graph_iri, data_file)
            .map_err(|e| format!("loading named graph {}: {e}", data_file.display()))?;
    }

    // Dispatch to the right pg_ripple function based on query type.
    // pg_ripple.sparql()        → TABLE(result jsonb)  — SELECT/CONSTRUCT/DESCRIBE
    // pg_ripple.sparql_ask()   → boolean              — ASK
    // pg_ripple.sparql_update() → bigint              — UPDATE
    let is_ask = spargebra::SparqlParser::new()
        .parse_query(&query_src)
        .map(|q| matches!(q, spargebra::Query::Ask { .. }))
        .unwrap_or(false);
    let is_update = spargebra::SparqlParser::new()
        .parse_update(&query_src)
        .is_ok()
        && spargebra::SparqlParser::new()
            .parse_query(&query_src)
            .is_err();

    if is_update {
        tx.execute("SELECT pg_ripple.sparql_update($1)", &[&query_src])
            .map_err(|e| format!("sparql_update error: {e}"))?;
    } else if is_ask {
        tx.query("SELECT pg_ripple.sparql_ask($1)", &[&query_src])
            .map_err(|e| format!("sparql_ask error: {e}"))?;
    } else {
        tx.query("SELECT * FROM pg_ripple.sparql($1)", &[&query_src])
            .map_err(|e| format!("sparql error: {e}"))?;
    }

    tx.rollback().map_err(|e| format!("rollback error: {e}"))?;
    Ok(())
}
