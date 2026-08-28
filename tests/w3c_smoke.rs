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
//! Local runs may skip when fixtures or PostgreSQL are unavailable. CI sets
//! `REQUIRE_CONFORMANCE=1` so the required gate cannot pass by skipping.

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
            if std::env::var("REQUIRE_CONFORMANCE").is_ok() {
                panic!("required W3C corpus is missing");
            }
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
        if std::env::var("REQUIRE_CONFORMANCE").is_ok() {
            panic!("required W3C smoke suite executed zero tests");
        }
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

    write_smoke_report(&report, report.duration_seconds);

    // ── Assert no unexpected failures ───────────────────────────────────────
    assert!(
        report.is_clean(),
        "\nW3C smoke subset: {} unexpected failure(s). See output above.\n{}",
        failures.len(),
        report.summary(),
    );
}

fn write_smoke_report(report: &w3c::RunReport, duration_seconds: f64) {
    let unexpected_failures: Vec<_> = report
        .results
        .iter()
        .filter(|result| result.outcome.is_unexpected_failure())
        .map(|result| {
            serde_json::json!({
                "key": result.test_case.iri,
                "name": result.test_case.name,
                "detail": format!("{:?}", result.outcome),
            })
        })
        .collect();
    let document = serde_json::json!({
        "pg_ripple_version": std::env::var("CONFORMANCE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned()),
        "git_sha": std::env::var("GITHUB_SHA").unwrap_or_else(|_| "unknown".to_owned()),
        "artifact_digest": std::env::var("CONFORMANCE_ARTIFACT_DIGEST").unwrap_or_else(|_| "uncomputed".to_owned()),
        "postgres_version": std::env::var("POSTGRES_VERSION").unwrap_or_else(|_| "unknown".to_owned()),
        "suite": "w3c_sparql11",
        "suite_commit": std::env::var("CONFORMANCE_SUITE_COMMIT").unwrap_or_else(|_| "unknown".to_owned()),
        "started_at": std::env::var("CONFORMANCE_STARTED_AT").unwrap_or_else(|_| "unknown".to_owned()),
        "duration_seconds": duration_seconds,
        "expected_total": report.total,
        "executed_total": report.total,
        "total": report.total,
        "passed": report.passed,
        "failed": report.failed,
        "skipped": report.skipped,
        "timeout": report.timeout,
        "xfail": report.xfail,
        "xpass": report.xpass,
        "unexpected_failures": unexpected_failures,
    });
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("results/conformance")
        .join(
            std::env::var("CONFORMANCE_VERSION")
                .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned()),
        );
    if let Err(error) = std::fs::create_dir_all(&directory).and_then(|_| {
        std::fs::write(
            directory.join("w3c_sparql11.json"),
            serde_json::to_string_pretty(&document).map_err(std::io::Error::other)?,
        )
    }) {
        panic!("failed to write W3C smoke conformance report: {error}");
    }
}
