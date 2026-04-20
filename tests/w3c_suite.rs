//! W3C SPARQL 1.1 full conformance suite — ~3 000 tests across 13 sub-suites.
//!
//! Sub-suites: aggregates, bind, exists, functions, grouping, negation,
//! project-expression, property-path, service, subquery, syntax-query,
//! basic-update.
//!
//! Runs in parallel (default: 8 threads); target: < 2 minutes on an 8-core runner.
//! This job is informational (non-blocking) until pass rate reaches 95%.
//!
//! # Running locally
//!
//! ```sh
//! # With W3C test data already in tests/w3c/data/:
//! cargo test --test w3c_suite -- --test-threads 8
//!
//! # Or point to a custom directory:
//! W3C_TEST_DIR=/tmp/sparql11 cargo test --test w3c_suite -- --test-threads 8
//! ```
//!
//! Tests skip gracefully when neither the test data nor a pg_ripple database
//! is reachable.

#[path = "w3c/mod.rs"]
mod w3c;

use std::io::Write;

use w3c::{RunConfig, test_data_dir};

/// Run the full W3C SPARQL 1.1 test suite.
#[test]
fn w3c_suite() {
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

    let threads: usize = std::env::var("W3C_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);

    let config = RunConfig {
        threads,
        timeout_secs: 5,
        categories: vec![
            "aggregates".into(),
            "bind".into(),
            "exists".into(),
            "functions".into(),
            "grouping".into(),
            "negation".into(),
            "project-expression".into(),
            "property-path".into(),
            "service".into(),
            "subquery".into(),
            "syntax-query".into(),
            "basic-update".into(),
        ],
        max_tests: None,
        known_failures_path: Some(known_failures_path).filter(|p| p.exists()),
    };

    // ── Run the suite ───────────────────────────────────────────────────────
    let db_url = w3c::db_connect_string();
    let start = std::time::Instant::now();
    let report = w3c::run_test_suite(&db_url, &data_dir, &config);
    let elapsed = start.elapsed();

    if report.total == 0 {
        println!("SKIP: no test cases found in any W3C sub-suite");
        return;
    }

    // ── Report ──────────────────────────────────────────────────────────────
    println!(
        "\nW3C SPARQL 1.1 full suite results ({:.1}s):",
        elapsed.as_secs_f32()
    );
    println!("  {}", report.summary());

    // Per-category breakdown.
    let categories = [
        "aggregates",
        "bind",
        "exists",
        "functions",
        "grouping",
        "negation",
        "optional",
        "project-expression",
        "property-path",
        "service",
        "subquery",
        "syntax-query",
        "update",
    ];
    println!("\n  Per-category:");
    for cat in categories {
        let cat_results: Vec<_> = report
            .results
            .iter()
            .filter(|r| r.test_case.category == cat)
            .collect();
        if cat_results.is_empty() {
            continue;
        }
        let pass = cat_results
            .iter()
            .filter(|r| matches!(r.outcome, w3c::TestOutcome::Pass))
            .count();
        let fail = cat_results
            .iter()
            .filter(|r| matches!(r.outcome, w3c::TestOutcome::Fail(_)))
            .count();
        let skip = cat_results
            .iter()
            .filter(|r| matches!(r.outcome, w3c::TestOutcome::Skip(_)))
            .count();
        let xfail = cat_results
            .iter()
            .filter(|r| matches!(r.outcome, w3c::TestOutcome::XFail(_)))
            .count();
        let total = cat_results.len();
        println!("    {cat:<25} {pass}/{total}  (fail={fail} skip={skip} xfail={xfail})");
    }

    // Write report.json artifact.
    write_report_json(&report, &config);

    // Print unexpected failures (cap at 20 for readability).
    let failures: Vec<_> = report
        .results
        .iter()
        .filter(|r| r.outcome.is_unexpected_failure())
        .collect();
    if !failures.is_empty() {
        println!("\n  UNEXPECTED FAILURES (first 20):");
        for f in failures.iter().take(20) {
            match &f.outcome {
                w3c::TestOutcome::Fail(msg) => {
                    println!(
                        "  FAIL  [{}] {} — {}",
                        f.test_case.category, f.test_case.name, msg
                    );
                }
                w3c::TestOutcome::Timeout => {
                    println!("  TIMEOUT  [{}] {}", f.test_case.category, f.test_case.name);
                }
                w3c::TestOutcome::XPass => {
                    println!("  XPASS  [{}] {}", f.test_case.category, f.test_case.name);
                }
                _ => {}
            }
        }
    }

    // Full suite is informational — do not fail the test binary on failures.
    // (Failures are visible in CI via the uploaded report.json artifact.)
    println!(
        "\n  Pass rate: {:.1}%",
        if report.total > 0 {
            (report.passed as f64 / report.total as f64) * 100.0
        } else {
            0.0
        }
    );
    println!("  Elapsed: {:.1}s (target: < 120s)", elapsed.as_secs_f32());
}

/// Write a `report.json` artifact with per-category pass/fail/skip/timeout counts.
fn write_report_json(report: &w3c::RunReport, config: &RunConfig) {
    use serde_json::{json, to_string_pretty};

    let categories = &config.categories;
    let mut per_cat = serde_json::Map::new();
    for cat in categories {
        let cat_results: Vec<_> = report
            .results
            .iter()
            .filter(|r| &r.test_case.category == cat)
            .collect();
        if cat_results.is_empty() {
            continue;
        }
        per_cat.insert(
            cat.clone(),
            json!({
                "pass":    cat_results.iter().filter(|r| matches!(r.outcome, w3c::TestOutcome::Pass)).count(),
                "fail":    cat_results.iter().filter(|r| matches!(r.outcome, w3c::TestOutcome::Fail(_))).count(),
                "skip":    cat_results.iter().filter(|r| matches!(r.outcome, w3c::TestOutcome::Skip(_))).count(),
                "timeout": cat_results.iter().filter(|r| matches!(r.outcome, w3c::TestOutcome::Timeout)).count(),
                "xfail":   cat_results.iter().filter(|r| matches!(r.outcome, w3c::TestOutcome::XFail(_))).count(),
                "xpass":   cat_results.iter().filter(|r| matches!(r.outcome, w3c::TestOutcome::XPass)).count(),
                "total":   cat_results.len(),
            }),
        );
    }

    let pass_rate = if report.total > 0 {
        (report.passed as f64 / report.total as f64) * 100.0
    } else {
        0.0
    };

    let doc = json!({
        "version": "0.41.0",
        "total":   report.total,
        "passed":  report.passed,
        "failed":  report.failed,
        "skipped": report.skipped,
        "timeout": report.timeout,
        "xfail":   report.xfail,
        "xpass":   report.xpass,
        "pass_rate_pct": format!("{pass_rate:.1}"),
        "categories": per_cat,
    });

    let report_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("w3c")
        .join("report.json");

    if let Ok(mut f) = std::fs::File::create(&report_path) {
        if let Ok(s) = to_string_pretty(&doc) {
            let _ = f.write_all(s.as_bytes());
            println!("  Report written to {}", report_path.display());
        }
    }
}
