//! GUC registration — extracted from lib.rs (MOD-01, v0.72.0).
//! Q13-01 (v0.84.0): split into per-domain submodules for navigability.
//!
//! `register_all_gucs()` is called once from `_PG_init`.

pub mod datalog;
pub mod federation;
pub mod observability;
pub mod security;
pub mod sparql;
pub mod storage;

/// Register all GUCs with PostgreSQL.
/// Delegates to per-domain submodule registration functions.
pub fn register_all_gucs() {
    sparql::register();
    storage::register();
    federation::register();
    datalog::register();
    security::register();
    observability::register();
}
