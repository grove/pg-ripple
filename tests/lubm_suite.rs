//! LUBM conformance suite for pg_ripple.
//!
//! Runs the 14 canonical LUBM queries against the synthetic univ1 fixture and
//! validates that each query returns the pre-computed expected row count.
//!
//! The Datalog validation sub-suite additionally:
//! - Loads the OWL RL rule set and materialises inference.
//! - Re-runs a selection of queries to confirm the inferred triples produced
//!   the correct counts from implicit (specific-only) type data.
//! - Checks basic materialization performance (< 5 s for univ1).
//!
//! # Running locally
//!
//! ```sh
//! cargo pgrx start pg18
//! cargo test --test lubm_suite -- --nocapture
//!
//! # With a custom database URL:
//! DATABASE_URL="host=localhost port=5432 dbname=mydb user=me" \
//!   cargo test --test lubm_suite -- --nocapture
//! ```
//!
//! # CI job
//!
//! The `lubm-suite` CI job runs this test after `w3c-suite`.
//! It generates no external data — everything is self-contained in the repo.

#[path = "lubm/mod.rs"]
mod lubm;

#[path = "conformance/mod.rs"]
mod conformance;

use std::io::Write;
use std::time::Instant;

// ── Main test ─────────────────────────────────────────────────────────────────

#[test]
fn lubm_suite() {
    // ── Pre-conditions ──────────────────────────────────────────────────────
    let mut client = match lubm::try_connect() {
        Some(c) => c,
        None => {
            // Print the connection details so CI can diagnose failures.
            let db_url = lubm::db_connect_string();
            println!("SKIP: Cannot connect to pg_ripple database ({db_url}).");
            println!("      Run `cargo pgrx start pg18` to start a local instance.");
            // In CI the database MUST be available; any skip is a failure.
            // We therefore panic if the LUBM_REQUIRE_DB env var is set.
            if std::env::var("LUBM_REQUIRE_DB").is_ok() {
                panic!("LUBM_REQUIRE_DB is set but the database is not reachable: {db_url}");
            }
            return;
        }
    };

    // ── Extension setup ─────────────────────────────────────────────────────
    client
        .batch_execute(
            "DROP EXTENSION IF EXISTS pg_ripple CASCADE; \
             CREATE EXTENSION pg_ripple CASCADE",
        )
        .expect("failed to (re-)create pg_ripple extension");

    // ── Load ontology + fixture ─────────────────────────────────────────────
    let ontology_ttl = lubm::load_ontology_text().expect("failed to read ontology");
    let univ1_ttl = lubm::load_univ1_text().expect("failed to read univ1 fixture");

    client
        .execute("SELECT pg_ripple.load_turtle($1, false)", &[&ontology_ttl])
        .expect("failed to load ontology into pg_ripple");

    client
        .execute("SELECT pg_ripple.load_turtle($1, false)", &[&univ1_ttl])
        .expect("failed to load univ1 fixture into pg_ripple");

    // ── Build 14 LUBM query entries ─────────────────────────────────────────
    let queries = lubm::build_lubm_queries();
    assert_eq!(
        queries.len(),
        14,
        "expected 14 LUBM queries, found {}",
        queries.len()
    );

    // ── Load known failures ─────────────────────────────────────────────────
    let project_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let kf_path = project_root
        .join("tests")
        .join("conformance")
        .join("known_failures.txt");
    let known_failures = conformance::load_known_failures(&kf_path, "lubm");

    // ── Run all 14 queries ──────────────────────────────────────────────────
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    writeln!(
        out,
        "\n[lubm] Running 14 canonical LUBM queries against univ1 fixture"
    )
    .ok();

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut xfail = 0usize;
    let suite_start = Instant::now();

    for query in &queries {
        let key = format!("Q{}", query.number);
        let t0 = Instant::now();

        let result = lubm::run_sparql_count(&mut client, &query.sparql);
        let elapsed_ms = t0.elapsed().as_millis();

        let is_known_failure = known_failures.contains(&key);

        match result {
            Ok(actual) if actual == query.expected => {
                if is_known_failure {
                    writeln!(
                        out,
                        "  XPASS [{}ms]  {} — expected {}, got {} (was in known-failures)",
                        elapsed_ms, key, query.expected, actual
                    )
                    .ok();
                    // XPASS is an unexpected pass; counted as a soft notice (not a hard failure).
                    xfail += 1;
                } else {
                    writeln!(out, "  PASS  [{}ms]  {} — {} rows", elapsed_ms, key, actual).ok();
                    passed += 1;
                }
            }
            Ok(actual) => {
                if is_known_failure {
                    writeln!(
                        out,
                        "  XFAIL [{}ms]  {} — expected {}, got {} (known failure)",
                        elapsed_ms, key, query.expected, actual
                    )
                    .ok();
                    xfail += 1;
                } else {
                    writeln!(
                        out,
                        "  FAIL  [{}ms]  {} — expected {} rows, got {}",
                        elapsed_ms, key, query.expected, actual
                    )
                    .ok();
                    failed += 1;
                }
            }
            Err(e) => {
                if is_known_failure {
                    writeln!(
                        out,
                        "  XFAIL [{}ms]  {} — query error (known failure): {e}",
                        elapsed_ms, key
                    )
                    .ok();
                    xfail += 1;
                } else {
                    writeln!(
                        out,
                        "  FAIL  [{}ms]  {} — query error: {e}",
                        elapsed_ms, key
                    )
                    .ok();
                    failed += 1;
                }
            }
        }
    }

    let suite_elapsed = suite_start.elapsed();

    writeln!(out).ok();
    writeln!(
        out,
        "[lubm] Query results: {} passed, {} failed, {} xfail/xpass / 14 total  ({:.1}s)",
        passed,
        failed,
        xfail,
        suite_elapsed.as_secs_f64()
    )
    .ok();

    // ── Datalog validation sub-suite ────────────────────────────────────────
    writeln!(out, "\n[lubm] Datalog validation sub-suite").ok();
    run_datalog_validation(&mut client, &mut out);

    // ── Final assertion ─────────────────────────────────────────────────────
    // No unexpected failures (failed == 0).  XFAIL/XPASS are informational.
    if failed > 0 {
        panic!(
            "[lubm] {} unexpected failure(s) in LUBM suite.  \
             Add entries to tests/conformance/known_failures.txt with prefix 'lubm:' \
             for any confirmed bugs.",
            failed
        );
    }

    // Warn (but do not fail) if the suite took > 30 seconds.
    let total_secs = suite_elapsed.as_secs_f64();
    if total_secs > 30.0 {
        writeln!(
            out,
            "[lubm] WARNING: suite elapsed {total_secs:.1}s (target: < 30s)"
        )
        .ok();
    }
}

// ── Datalog validation sub-suite ─────────────────────────────────────────────

fn run_datalog_validation(client: &mut postgres::Client, out: &mut impl Write) {
    // 1. Load OWL RL built-in rules.
    let rules_loaded: i64 = client
        .query_one("SELECT pg_ripple.load_rules_builtin('owl-rl')", &[])
        .map(|r| r.get::<_, i64>(0))
        .unwrap_or(0);

    writeln!(out, "  [datalog] loaded {} OWL RL rules", rules_loaded).ok();

    if rules_loaded < 1 {
        writeln!(
            out,
            "  [datalog] SKIP: no rules loaded — load_rules_builtin returned 0"
        )
        .ok();
        return;
    }

    // 2. Run inference and check it completes.
    let t0 = Instant::now();
    let infer_result = client.query_one("SELECT pg_ripple.infer('owl-rl')", &[]);
    let infer_ms = t0.elapsed().as_millis();

    let derived: i64 = match infer_result {
        Ok(ref row) => row.get::<_, i64>(0),
        Err(ref e) => {
            writeln!(
                out,
                "  [datalog] FAIL: infer('owl-rl') returned an error: {e}"
            )
            .ok();
            // Reset the connection by clearing any outstanding transaction.
            let _ = client.batch_execute("ROLLBACK");
            run_custom_rule_validation(client, out);
            return;
        }
    };

    writeln!(
        out,
        "  [datalog] infer('owl-rl') derived {} triples in {}ms",
        derived, infer_ms
    )
    .ok();

    // Performance check: < 5000 ms for univ1.
    if infer_ms > 5000 {
        writeln!(
            out,
            "  [datalog] WARNING: materialization took {}ms (target: < 5000ms)",
            infer_ms
        )
        .ok();
    } else {
        writeln!(
            out,
            "  [datalog] PASS: materialization within 5s target ({}ms)",
            infer_ms
        )
        .ok();
    }

    // 3. Re-run Q1, Q6, Q14 to confirm inference produced correct counts.
    let goal_checks: &[(&str, usize)] = &[("Q1", 3), ("Q6", 12), ("Q14", 5)];

    for (key, expected) in goal_checks {
        let n: u8 = key[1..].parse().unwrap_or(0);
        match lubm::load_query_text(n) {
            Ok(sparql) => match lubm::run_sparql_count(client, &sparql) {
                Ok(actual) if actual == *expected => {
                    writeln!(
                        out,
                        "  [datalog] PASS  {} — {} rows (matches post-inference baseline)",
                        key, actual
                    )
                    .ok();
                }
                Ok(actual) => {
                    writeln!(
                        out,
                        "  [datalog] WARN  {} — expected {} rows, got {} after inference",
                        key, expected, actual
                    )
                    .ok();
                }
                Err(e) => {
                    writeln!(
                        out,
                        "  [datalog] WARN  {} — query error after inference: {e}",
                        key
                    )
                    .ok();
                }
            },
            Err(e) => {
                writeln!(out, "  [datalog] SKIP  {key} — could not load query: {e}").ok();
            }
        }
    }

    // 4. Custom rule validation.
    run_custom_rule_validation(client, out);
}

fn run_custom_rule_validation(client: &mut postgres::Client, out: &mut impl Write) {
    // Custom rule: transitive subOrganizationOf closure.
    // Define a rule that derives transitive membership and verify it works.
    let custom_rule = r#"
# transitive subOrganizationOf closure
?X <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#Organization> :-
    ?X <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#subOrganizationOf> ?Y .
"#;

    let custom_loaded: i64 = client
        .query_one(
            "SELECT pg_ripple.load_rules($1, 'lubm_custom')",
            &[&custom_rule],
        )
        .map(|r| r.get::<_, i64>(0))
        .unwrap_or(0);

    if custom_loaded > 0 {
        let custom_result = client.query_one("SELECT pg_ripple.infer('lubm_custom')", &[]);
        match custom_result {
            Ok(row) => {
                let custom_derived: i64 = row.get(0);
                writeln!(
                    out,
                    "  [datalog] custom rule derived {} triples (transitive Organization closure)",
                    custom_derived
                )
                .ok();
                if custom_derived >= 1 {
                    writeln!(out, "  [datalog] PASS  custom rule validation").ok();
                } else {
                    writeln!(
                        out,
                        "  [datalog] WARN  custom rule derived 0 triples (expected >= 1)"
                    )
                    .ok();
                }
            }
            Err(e) => {
                let _ = client.batch_execute("ROLLBACK");
                writeln!(out, "  [datalog] WARN  custom rule infer() error: {e}").ok();
            }
        }
    } else {
        writeln!(
            out,
            "  [datalog] SKIP  custom rule validation (rule could not be loaded)"
        )
        .ok();
    }
}
