//! Apache Jena SPARQL test harness for pg_ripple.
//!
//! Adapts the Jena test manifest vocabulary (`jt:`) to the unified conformance
//! runner.  Jena uses the same Turtle-format manifest structure as W3C but
//! adds test types specific to Jena's test categories.
//!
//! # Manifest vocabulary extensions
//!
//! Beyond the W3C `mf:` and `qt:` terms, Jena defines:
//! - `jt:QueryEvaluationTest` — run a SPARQL query and compare results
//! - `jt:UpdateEvaluationTest` — run a SPARQL UPDATE and compare resulting graph
//! - `jt:NegativeSyntaxTest` — query must fail to parse
//! - `jt:PositiveSyntaxTest` — query must parse without error
//!
//! # Sub-suites
//!
//! - `sparql-query` — SPARQL 1.1 query evaluation
//! - `sparql-update` — SPARQL 1.1 update evaluation
//! - `sparql-syntax` — positive and negative syntax tests
//! - `algebra` — algebra normalisation and equivalence tests
//!
//! # Usage
//!
//! ```sh
//! # With Jena test data in tests/jena/data/:
//! cargo test --test jena_suite
//!
//! # Or point to a custom directory:
//! JENA_TEST_DIR=/tmp/jena cargo test --test jena_suite
//! ```

pub mod manifest;

use std::path::PathBuf;

/// Build the PostgreSQL connection string (reuses W3C helper logic).
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

/// Try to open a PostgreSQL connection.  Returns `None` if the connection fails.
pub fn try_connect() -> Option<postgres::Client> {
    postgres::Client::connect(&db_connect_string(), postgres::NoTls).ok()
}

/// Return the Jena SPARQL test data directory.
///
/// Resolution order:
/// 1. `JENA_TEST_DIR` environment variable
/// 2. `tests/jena/data/` relative to the project root
pub fn test_data_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("JENA_TEST_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let default = project_root.join("tests").join("jena").join("data");
    if default.is_dir() {
        Some(default)
    } else {
        None
    }
}

/// Convert a `file://` IRI to a local filesystem path.
pub fn file_iri_to_path(iri: &str) -> Option<PathBuf> {
    iri.strip_prefix("file://").map(PathBuf::from)
}
