//! WatDiv benchmark harness for pg_ripple.
//!
//! WatDiv (Waterloo SPARQL Diversity Test Suite) tests correctness and
//! performance under realistic data distributions across four query classes:
//!
//! - **Star** (S1–S7): same-subject patterns — exercises VP star-join optimisation
//! - **Chain** (C1–C3): linear path patterns — tests join ordering
//! - **Snowflake** (F1–F5): star + chain hybrid — tests mixed strategies
//! - **Complex** (B1–B12, L1–L5): multi-hop with OPTIONAL and UNION
//!
//! Correctness is validated by comparing result row counts against pre-computed
//! baselines (within ±0.1%).  Performance regressions > 20% trigger a CI warning
//! (not a failure).
//!
//! # Usage
//!
//! ```sh
//! # Run with cached 10M-triple dataset:
//! cargo test --test watdiv_suite
//!
//! # Or set custom data location:
//! WATDIV_DATA_DIR=/tmp/watdiv cargo test --test watdiv_suite
//!
//! # Regenerate the dataset:
//! bash scripts/fetch_conformance_tests.sh --watdiv
//! ```

pub mod template;

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

/// Return the WatDiv data directory.
///
/// Resolution order:
/// 1. `WATDIV_DATA_DIR` environment variable
/// 2. `tests/watdiv/data/` relative to the project root
pub fn data_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("WATDIV_DATA_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let default = project_root.join("tests").join("watdiv").join("data");
    if default.is_dir() {
        Some(default)
    } else {
        None
    }
}

/// Return the WatDiv query templates directory.
///
/// Resolution order:
/// 1. `WATDIV_TEMPLATE_DIR` environment variable
/// 2. `tests/watdiv/templates/` relative to the project root
pub fn template_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("WATDIV_TEMPLATE_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let default = project_root.join("tests").join("watdiv").join("templates");
    if default.is_dir() {
        Some(default)
    } else {
        None
    }
}

/// Return the WatDiv baseline file (expected row counts per template).
pub fn baseline_file() -> Option<PathBuf> {
    if let Ok(f) = std::env::var("WATDIV_BASELINE_FILE") {
        let p = PathBuf::from(f);
        if p.exists() {
            return Some(p);
        }
    }
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let default = project_root
        .join("tests")
        .join("watdiv")
        .join("baselines.json");
    if default.exists() {
        Some(default)
    } else {
        None
    }
}
