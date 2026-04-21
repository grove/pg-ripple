//! W3C SPARQL 1.1 test harness — shared types and utilities.
//!
//! This module provides the core infrastructure for running the W3C SPARQL 1.1
//! conformance test suite against a live pg_ripple installation.
//!
//! # Usage
//!
//! Set environment variables before running:
//! - `W3C_TEST_DIR` — path to the downloaded W3C test data (default: `tests/w3c/data/`)
//! - `DATABASE_URL`  — PostgreSQL connection string (default: pgrx pg18 socket)
//!
//! Tests skip gracefully when either the test data directory or the database
//! connection is unavailable.

pub mod loader;
pub mod manifest;
pub mod runner;
pub mod validator;

#[allow(unused_imports)]
pub use manifest::{TestCase, TestType};
#[allow(unused_imports)]
pub use runner::{RunConfig, RunReport, TestOutcome, run_test_suite};

use std::path::PathBuf;

/// Build the PostgreSQL connection string.
///
/// Resolution order:
/// 1. `DATABASE_URL` environment variable
/// 2. `PGHOST` / `PGPORT` / `PGDATABASE` / `PGUSER` environment variables
/// 3. pgrx default (Unix socket at `~/.pgrx`, port 28818)
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
    let conn_str = db_connect_string();
    postgres::Client::connect(&conn_str, postgres::NoTls).ok()
}

/// Return the W3C SPARQL 1.1 test data directory.
///
/// Resolution order:
/// 1. `W3C_TEST_DIR` environment variable
/// 2. `tests/w3c/data/` relative to the project root
pub fn test_data_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("W3C_TEST_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }

    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let default = project_root.join("tests").join("w3c").join("data");
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
