//! LUBM (Lehigh University Benchmark) test harness for pg_ripple.
//!
//! The LUBM benchmark defines 14 canonical SPARQL queries over a
//! university-domain OWL ontology, primarily testing OWL RL inference rules:
//! subclass/subproperty entailment, domain/range reasoning, and multi-hop
//! property chains.
//!
//! This harness uses a self-contained synthetic fixture (`fixtures/univ1.ttl`)
//! that bundles a single-university dataset with all supertype assertions
//! stated explicitly.  All 14 queries can therefore be validated without
//! requiring the Java UBA generator or a live OWL RL inference pass.
//!
//! The Datalog validation sub-suite (see `lubm_suite.rs`) separately validates
//! that running `pg_ripple.load_rules_builtin('owl-rl')` + `pg_ripple.infer()`
//! on an implicit-type-only version of the same data yields identical results.
//!
//! # Running locally
//!
//! ```sh
//! cargo pgrx start pg18
//! cargo test --test lubm_suite -- --nocapture
//! ```

use std::path::PathBuf;

/// Build the PostgreSQL connection string.
pub fn db_connect_string() -> String {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return url;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let host = std::env::var("PGHOST").unwrap_or_else(|_| format!("{home}/.pgrx"));
    let port = std::env::var("PGPORT").unwrap_or_else(|_| "28818".into());
    let dbname = std::env::var("PGDATABASE").unwrap_or_else(|_| "postgres".into());
    let user = std::env::var("PGUSER")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("USER").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "postgres".into());
    format!("host={host} port={port} dbname={dbname} user={user}")
}

/// Try to open a PostgreSQL connection.
pub fn try_connect() -> Option<postgres::Client> {
    postgres::Client::connect(&db_connect_string(), postgres::NoTls).ok()
}

/// Return the absolute path to `tests/lubm/` inside the project root.
pub fn lubm_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("lubm")
}

/// Return the path to the univ1 fixture Turtle file.
pub fn univ1_fixture() -> PathBuf {
    lubm_dir().join("fixtures").join("univ1.ttl")
}

/// Return the path to the ontology Turtle file.
pub fn ontology_file() -> PathBuf {
    lubm_dir().join("ontology").join("univ-bench-owl.ttl")
}

/// Return the path to a query file by 1-based number.
pub fn query_file(n: u8) -> PathBuf {
    lubm_dir().join("queries").join(format!("q{n:02}.sparql"))
}

/// Return the path to the univ1 baseline JSON file.
pub fn univ1_baselines() -> PathBuf {
    lubm_dir().join("baselines").join("univ1.json")
}

/// Return the path to the datalog validation baseline JSON file.
#[allow(dead_code)]
pub fn datalog_validation_baselines() -> PathBuf {
    lubm_dir().join("baselines").join("datalog_validation.json")
}

/// Load the expected query result counts from `tests/lubm/baselines/univ1.json`.
///
/// Returns a `Vec<(query_id, expected_count)>` ordered Q1..Q14.
pub fn load_univ1_baselines() -> Vec<(String, usize)> {
    let path = univ1_baselines();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[lubm] could not read baselines {}: {e}", path.display());
            return Vec::new();
        }
    };
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[lubm] baseline JSON parse error: {e}");
            return Vec::new();
        }
    };
    let mut result = Vec::new();
    if let Some(queries) = v.get("queries").and_then(|q| q.as_object()) {
        for n in 1..=14u8 {
            let key = format!("Q{n}");
            let count = queries.get(&key).and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            result.push((key, count));
        }
    }
    result
}

/// Load the ontology Turtle text.
pub fn load_ontology_text() -> Result<String, String> {
    std::fs::read_to_string(ontology_file()).map_err(|e| format!("reading ontology: {e}"))
}

/// Load the univ1 fixture Turtle text.
pub fn load_univ1_text() -> Result<String, String> {
    std::fs::read_to_string(univ1_fixture()).map_err(|e| format!("reading univ1 fixture: {e}"))
}

/// Load the SPARQL text for a query by 1-based number (1..=14).
pub fn load_query_text(n: u8) -> Result<String, String> {
    let path = query_file(n);
    let raw =
        std::fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    // Strip comment lines (lines starting with '#').
    let sparql: String = raw
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(sparql)
}

/// Run a SPARQL SELECT query against a live pg_ripple database and return the
/// row count.
///
/// Uses `SELECT COUNT(*) FROM pg_ripple.sparql($query)` which is the canonical
/// way to count results via the pg_ripple SQL API.
pub fn run_sparql_count(client: &mut postgres::Client, sparql: &str) -> Result<usize, String> {
    let row = client
        .query_one(
            "SELECT COUNT(*)::bigint FROM pg_ripple.sparql($1)",
            &[&sparql],
        )
        .map_err(|e| format!("sparql count query failed: {e}"))?;
    let count: i64 = row.get(0);
    Ok(count as usize)
}

/// Represents a single LUBM query test.
pub struct LubmQuery {
    /// 1-based query number (1..=14).
    pub number: u8,
    /// SPARQL query text (comments stripped).
    pub sparql: String,
    /// Expected result row count.
    pub expected: usize,
}

/// Build all 14 LUBM query entries from disk fixtures and baselines.
pub fn build_lubm_queries() -> Vec<LubmQuery> {
    let baselines = load_univ1_baselines();
    let mut queries = Vec::with_capacity(14);
    for (key, expected) in baselines {
        let n: u8 = key[1..].parse().unwrap_or(0);
        if n == 0 || n > 14 {
            continue;
        }
        match load_query_text(n) {
            Ok(sparql) => queries.push(LubmQuery {
                number: n,
                sparql,
                expected,
            }),
            Err(e) => eprintln!("[lubm] could not load query Q{n}: {e}"),
        }
    }
    queries.sort_by_key(|q| q.number);
    queries
}
