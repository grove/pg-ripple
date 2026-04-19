//! W3C SPARQL 1.1 smoke subset — 180 curated tests.
//!
//! Covers the three categories most likely to expose SQL-generation bugs:
//! - `optional`   (OPTIONAL / LEFT JOIN patterns)
//! - `aggregates` (GROUP BY, COUNT, SUM, AVG, MIN, MAX, SAMPLE)
//! - `grouping`   (SPARQL 1.1 GROUP BY edge cases)
//!
//! Runs on every PR and push to `main`; target: < 30 seconds.
//!
//! # Running locally
//!
//! ```sh
//! # With W3C test data already in tests/w3c/data/:
//! cargo test --test w3c_smoke
//!
//! # Or point to a custom directory:
//! W3C_TEST_DIR=/tmp/sparql11 cargo test --test w3c_smoke
//! ```
//!
//! Tests skip gracefully when neither the test data nor a pg_ripple database
//! is reachable.

#[path = "w3c/mod.rs"]
mod w3c;

use w3c::{RunConfig, test_data_dir};

/// Run the W3C SPARQL 1.1 smoke subset (optional + aggregates + grouping).
#[test]
fn w3c_smoke() {
    // ── Pre-conditions ──────────────────────────────────────────────────────
    let data_dir = match test_data_dir() {
        Some(d) => d,
        None => {
            println!("SKIP: W3C test data directory not found.");
            println!(
                "      Run scripts/fetch_w3c_tests.sh or set W3C_TEST_DIR to enable this test."
            );
            return;
        }
    };

    // ── Build run config ────────────────────────────────────────────────────
    let project_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let known_failures_path = project_root
        .join("tests")
        .join("w3c")
        .join("known_failures.txt");

    let config = RunConfig {
        threads: 1,
        timeout_secs: 30,
        categories: vec!["optional".into(), "aggregates".into(), "grouping".into()],
        max_tests: Some(180),
        known_failures_path: Some(known_failures_path).filter(|p| p.exists()),
    };

    // ── Run the suite ───────────────────────────────────────────────────────
    let db_url = w3c::db_connect_string();
    let report = w3c::run_test_suite(&db_url, &data_dir, &config);

    if report.total == 0 {
        println!("SKIP: no test cases found in categories: optional, aggregates, grouping");
        return;
    }

    // ── Report ──────────────────────────────────────────────────────────────
    println!("\nW3C smoke subset results:");
    println!("  {}", report.summary());

    // Print per-category breakdown.
    let categories = ["optional", "aggregates", "grouping"];
    for cat in categories {
        let cat_results: Vec<_> = report
            .results
            .iter()
            .filter(|r| r.test_case.category == cat)
            .collect();
        let cat_pass = cat_results
            .iter()
            .filter(|r| matches!(r.outcome, w3c::TestOutcome::Pass))
            .count();
        let cat_total = cat_results.len();
        println!("  {cat}: {cat_pass}/{cat_total}");
    }

    // Print failures.
    let failures: Vec<_> = report
        .results
        .iter()
        .filter(|r| r.outcome.is_unexpected_failure())
        .collect();
    if !failures.is_empty() {
        println!("\n  UNEXPECTED FAILURES:");
        for f in &failures {
            match &f.outcome {
                w3c::TestOutcome::Fail(msg) => {
                    println!("  FAIL  {} — {}", f.test_case.name, msg);
                }
                w3c::TestOutcome::Timeout => {
                    println!("  TIMEOUT  {}", f.test_case.name);
                }
                w3c::TestOutcome::XPass => {
                    println!(
                        "  XPASS  {} (remove from known_failures.txt)",
                        f.test_case.name
                    );
                }
                _ => {}
            }
        }
    }

    // ── Assert no unexpected failures ───────────────────────────────────────
    assert!(
        report.is_clean(),
        "\nW3C smoke subset: {} unexpected failure(s). See output above.\n{}",
        failures.len(),
        report.summary(),
    );
}
