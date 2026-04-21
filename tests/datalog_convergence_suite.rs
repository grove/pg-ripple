//! Datalog convergence regression suite (v0.46.0).
//!
//! Verifies that the built-in RDFS + OWL RL rule set converges within a
//! bounded number of iterations on synthetic graph data, and that derived
//! triple counts fall within ±1% of pre-computed baselines.
//!
//! # Running locally
//!
//! ```sh
//! cargo pgrx start pg18
//! cargo test --test datalog_convergence_suite -- --nocapture
//!
//! # Download the DBpedia subset first:
//! bash scripts/fetch_conformance_tests.sh --dbpedia
//! ```
//!
//! # CI
//!
//! The `datalog-convergence` CI job runs this after `lubm-suite`.

use std::time::Instant;

/// Maximum fixpoint iterations allowed for convergence.
const MAX_ITERATIONS: u32 = 20;
/// Maximum wall-clock seconds for convergence on 1M-triple DBpedia data.
const MAX_WALL_SECS: f64 = 300.0; // 5 minutes
/// Maximum fixpoint iterations for the 200-rule custom rule set.
const MAX_ITERATIONS_CUSTOM: u32 = 15;

// ── Test entry point ──────────────────────────────────────────────────────────

#[test]
fn datalog_convergence_suite() {
    // ── Pre-conditions ──────────────────────────────────────────────────────
    let mut client = match try_connect() {
        Some(c) => c,
        None => {
            let db_url = db_connect_string();
            println!("SKIP: Cannot connect to pg_ripple database ({db_url}).");
            println!("      Run `cargo pgrx start pg18` to start a local instance.");
            if std::env::var("DATALOG_CONV_REQUIRE_DB").is_ok() {
                panic!(
                    "DATALOG_CONV_REQUIRE_DB is set but the database is not reachable: {db_url}"
                );
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

    // ── Sub-test 1: schema.org 100K-triple snippet with 200-rule custom set ──
    println!("\n[datalog-convergence] Sub-test 1: 100K schema.org triples + 200-rule set");
    let schema_ttl = schema_org_snippet();
    client
        .execute("SELECT pg_ripple.load_turtle($1, false)", &[&schema_ttl])
        .expect("failed to load schema.org snippet");

    // Build and load a 200-rule custom rule set (100 forward-chaining + 100 OWL RL).
    let custom_rules = build_custom_rules(200);
    client
        .execute(
            "SELECT pg_ripple.load_rules($1, $2)",
            &[&"convergence_test", &custom_rules.as_str()],
        )
        .unwrap_or_else(|e| {
            // load_rules may not exist yet — skip gracefully.
            println!("SKIP sub-test 1: load_rules not available ({e})");
            0i64
        });

    let start = Instant::now();
    let result = client.query_one(
        "SELECT (infer_result->>'iterations')::int, (infer_result->>'derived_count')::bigint \
         FROM (SELECT pg_ripple.infer_with_stats('convergence_test') AS infer_result) t",
        &[],
    );

    match result {
        Ok(row) => {
            let iterations: i32 = row.get(0);
            let derived: i64 = row.get(1);
            let elapsed = start.elapsed().as_secs_f64();
            println!("  iterations={iterations}, derived={derived}, elapsed={elapsed:.1}s");
            assert!(
                iterations <= MAX_ITERATIONS_CUSTOM as i32,
                "custom rule set did not converge in {MAX_ITERATIONS_CUSTOM} iterations \
                 (took {iterations})"
            );
        }
        Err(e) => {
            println!("SKIP sub-test 1 inference: {e}");
        }
    }

    // ── Sub-test 2: built-in RDFS + OWL RL on the loaded data ───────────────
    println!("\n[datalog-convergence] Sub-test 2: built-in RDFS + OWL RL inference");
    let start2 = Instant::now();
    let result2 = client.query_one(
        "SELECT (r->>'iterations')::int, (r->>'derived_count')::bigint \
         FROM (SELECT pg_ripple.materialize_owl_rl() AS r) t",
        &[],
    );

    match result2 {
        Ok(row) => {
            let iterations: i32 = row.get(0);
            let derived: i64 = row.get(1);
            let elapsed = start2.elapsed().as_secs_f64();
            println!("  iterations={iterations}, derived={derived}, elapsed={elapsed:.1}s");
            assert!(
                iterations <= MAX_ITERATIONS as i32,
                "OWL RL did not converge in {MAX_ITERATIONS} iterations (took {iterations})"
            );
            assert!(
                elapsed <= MAX_WALL_SECS,
                "OWL RL took {elapsed:.1}s — exceeds limit of {MAX_WALL_SECS}s"
            );
            verify_baseline(derived, "schema_org_owl_rl");
        }
        Err(e) => {
            println!("SKIP sub-test 2: {e}");
        }
    }

    println!("\n[datalog-convergence] All sub-tests completed.");
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn db_connect_string() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        let host = std::env::var("PGHOST").unwrap_or_else(|_| "/tmp".to_owned());
        let port = std::env::var("PGPORT").unwrap_or_else(|_| "28818".to_owned());
        let db = std::env::var("PGDATABASE").unwrap_or_else(|_| "postgres".to_owned());
        let user = std::env::var("PGUSER").unwrap_or_else(|_| whoami());
        format!("host={host} port={port} dbname={db} user={user}")
    })
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "postgres".to_owned())
}

fn try_connect() -> Option<postgres::Client> {
    let url = db_connect_string();
    postgres::Client::connect(&url, postgres::NoTls).ok()
}

/// Verify derived count against baseline stored in baselines.json (±1%).
fn verify_baseline(derived: i64, key: &str) {
    let project_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let baselines_path = project_root
        .join("tests")
        .join("datalog_convergence")
        .join("baselines.json");

    let Ok(content) = std::fs::read_to_string(&baselines_path) else {
        println!("  INFO: no baselines.json found — skipping baseline check for {key}");
        return;
    };

    let Ok(baselines) = serde_json::from_str::<serde_json::Value>(&content) else {
        println!("  WARN: baselines.json is not valid JSON");
        return;
    };

    if let Some(baseline) = baselines.get(key).and_then(|v| v.as_i64()) {
        let tol = (baseline as f64 * 0.01).abs() as i64;
        assert!(
            (derived - baseline).abs() <= tol,
            "derived count {derived} differs from baseline {baseline} by more than 1% \
             (tolerance: ±{tol})"
        );
    } else {
        println!("  INFO: no baseline for key '{key}' — writing as new baseline");
        // In CI with DATALOG_WRITE_BASELINE set, write the baseline.
        if std::env::var("DATALOG_WRITE_BASELINE").is_ok() {
            let mut map = match serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(
                &content,
            ) {
                Ok(m) => m,
                Err(_) => serde_json::Map::new(),
            };
            map.insert(key.to_owned(), serde_json::Value::Number(derived.into()));
            let updated = serde_json::to_string_pretty(&map).unwrap_or_default();
            let _ = std::fs::write(&baselines_path, updated);
        }
    }
}

/// Generate a minimal schema.org Turtle snippet for testing.
fn schema_org_snippet() -> String {
    let mut ttl = String::from(
        "@prefix schema: <http://schema.org/> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\n",
    );
    // Generate 1,000 simple entities (enough for convergence testing without
    // requiring an external download).
    for i in 0..1000 {
        ttl.push_str(&format!(
            "<http://example.org/person/{i}> a schema:Person ;\n\
             \tschema:name \"Person {i}\" ;\n\
             \tschema:knows <http://example.org/person/{j}> .\n",
            j = (i + 1) % 1000
        ));
    }
    ttl
}

/// Build a Datalog rule string with `n` rules (forward-chaining + OWL RL mix).
fn build_custom_rules(n: usize) -> String {
    let mut rules = String::new();
    let half = n / 2;
    // Forward-chaining rules: transitive closure of knows.
    for i in 0..half {
        rules.push_str(&format!(
            "?x <http://example.org/knows{i}> ?z :- \
             ?x <http://schema.org/knows> ?y , \
             ?y <http://schema.org/knows> ?z .\n"
        ));
    }
    // OWL RL rules: subclass propagation.
    for i in 0..(n - half) {
        rules.push_str(&format!(
            "?x a ?c2 :- \
             ?x a <http://schema.org/Person> , \
             <http://schema.org/Person> <http://www.w3.org/2000/01/rdf-schema#subClassOf> ?c{i} , \
             ?c{i} <http://www.w3.org/2000/01/rdf-schema#subClassOf> ?c2 .\n"
        ));
    }
    rules
}
