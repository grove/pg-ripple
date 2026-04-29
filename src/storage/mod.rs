//! Storage engine — VP table management and triple CRUD (v0.6.0 HTAP).
//!
//! # VP table layout (v0.6.0+)
//!
//! Each predicate is split into three physical tables plus a read view:
//!
//! ```sql
//! -- Write inbox (all INSERTs go here)
//! CREATE TABLE _pg_ripple.vp_{id}_delta (s, o, g, i, source);
//! -- Read-optimised archive (BRIN-indexed, populated by merge worker)
//! CREATE TABLE _pg_ripple.vp_{id}_main  (s, o, g, i, source);
//! -- Pending deletes from main
//! CREATE TABLE _pg_ripple.vp_{id}_tombstones (s, o, g);
//! -- Read view: (main − tombstones) UNION ALL delta
//! CREATE VIEW  _pg_ripple.vp_{id} AS ...;
//! ```
//!
//! The view `_pg_ripple.vp_{id}` maintains backward compatibility with
//! the SPARQL query engine.  All new predicates are HTAP-split on creation.
//!
//! Predicates with fewer than `pg_ripple.vp_promotion_threshold` (default 1 000)
//! triples are initially stored in `_pg_ripple.vp_rare (p, s, o, g, i, source)`.
//! vp_rare is not split (HTAP exemption) — see ROADMAP v0.6.0.
//!
//! # Named graphs
//!
//! The default graph has identifier `0`.  Named graphs have positive `i64` ids.

pub mod catalog;
pub mod cdc_bridge;
pub mod index_advisor;
pub mod merge;
pub mod mutation_journal;
pub(crate) mod promote;

pub(crate) use promote::{promote_rare_predicates, recover_interrupted_promotions};

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

// v0.72.0 MOD-01: extracted modules
pub mod dictionary_io;
pub mod ops;
pub mod vp_rare_io;

// Re-export all public API from extracted modules.
#[allow(unused_imports)]
pub use dictionary_io::{encode_rdf_term, parse_rdf_term, strip_angle_brackets_pub};
#[allow(unused_imports)]
pub use ops::*;

/// Initialize the extension's base schemas and tables.
/// Called once from _PG_init to ensure all base infrastructure exists.
#[allow(dead_code)]
pub fn initialize_schema() {
    // Create the user-visible schema if it doesn't exist.
    Spi::run_with_args(
        "DO $$ BEGIN \
             IF NOT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = 'pg_ripple') THEN \
                 SET LOCAL allow_system_table_mods = on; \
                 CREATE SCHEMA pg_ripple; \
             END IF; \
         END $$",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("pg_ripple schema creation error: {e}"));

    // Create the internal schema if it doesn't exist.
    Spi::run_with_args("CREATE SCHEMA IF NOT EXISTS _pg_ripple", &[])
        .unwrap_or_else(|e| pgrx::error!("_pg_ripple schema creation error: {e}"));

    // Create the dictionary table.
    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.dictionary ( \
             id       BIGINT   GENERATED ALWAYS AS IDENTITY PRIMARY KEY, \
             hash     BYTEA    NOT NULL, \
             value    TEXT     NOT NULL, \
             kind     SMALLINT NOT NULL DEFAULT 0, \
             datatype TEXT, \
             lang     TEXT \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary table creation error: {e}"));

    // v0.4.0: Add quoted-triple component columns to dictionary (idempotent).
    // These columns are only populated for kind = 5 (KIND_QUOTED_TRIPLE).
    Spi::run_with_args(
        "ALTER TABLE _pg_ripple.dictionary \
             ADD COLUMN IF NOT EXISTS qt_s BIGINT, \
             ADD COLUMN IF NOT EXISTS qt_p BIGINT, \
             ADD COLUMN IF NOT EXISTS qt_o BIGINT",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary qt columns migration error: {e}"));

    // Unique index on the full 128-bit hash (collision-free lookup key).
    Spi::run_with_args(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_dictionary_hash \
         ON _pg_ripple.dictionary (hash)",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary hash index creation error: {e}"));

    // Create indexes on dictionary table
    Spi::run_with_args(
        "CREATE INDEX IF NOT EXISTS idx_dictionary_value_kind \
         ON _pg_ripple.dictionary (value, kind)",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary index creation error: {e}"));

    // Create the statement ID sequence.
    Spi::run_with_args(
        "CREATE SEQUENCE IF NOT EXISTS _pg_ripple.statement_id_seq \
         START 1 INCREMENT 1 CACHE 64 NO CYCLE",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("statement sequence creation error: {e}"));

    // Create the load generation sequence (for blank node document-scoping).
    Spi::run_with_args(
        "CREATE SEQUENCE IF NOT EXISTS _pg_ripple.load_generation_seq \
         START 1 INCREMENT 1 NO CYCLE",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("load_generation sequence creation error: {e}"));

    // Create the predicates catalog.
    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.predicates ( \
             id           BIGINT      NOT NULL PRIMARY KEY, \
             table_oid    OID, \
             triple_count BIGINT      NOT NULL DEFAULT 0 \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("predicates catalog creation error: {e}"));

    // v0.25.0 A-5: Add schema_name and table_name columns (idempotent).
    Spi::run_with_args(
        "ALTER TABLE _pg_ripple.predicates \
             ADD COLUMN IF NOT EXISTS schema_name TEXT, \
             ADD COLUMN IF NOT EXISTS table_name  TEXT",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("predicates schema_name/table_name migration error: {e}"));

    // v0.55.0 F-2: Add tombstones_cleared_at for tombstone GC tracking.
    Spi::run_with_args(
        "ALTER TABLE _pg_ripple.predicates \
             ADD COLUMN IF NOT EXISTS tombstones_cleared_at TIMESTAMPTZ",
        &[],
    )
    .unwrap_or_else(|e| pgrx::warning!("predicates tombstones_cleared_at migration: {e}"));

    // v0.68.0 PROMO-01: Add promotion_status for nonblocking shadow-table promotion.
    // Values: NULL/'promoted' = fully promoted; 'promoting' = copy in progress.
    Spi::run_with_args(
        "ALTER TABLE _pg_ripple.predicates \
             ADD COLUMN IF NOT EXISTS promotion_status TEXT",
        &[],
    )
    .unwrap_or_else(|e| pgrx::warning!("predicates promotion_status migration: {e}"));

    // Create the rare predicates consolidation table.
    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.vp_rare ( \
             p      BIGINT      NOT NULL, \
             s      BIGINT      NOT NULL, \
             o      BIGINT      NOT NULL, \
             g      BIGINT      NOT NULL DEFAULT 0, \
             i      BIGINT      NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq'), \
             source SMALLINT    NOT NULL DEFAULT 0 \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("vp_rare table creation error: {e}"));

    // Create indexes on vp_rare table
    Spi::run_with_args(
        "CREATE INDEX IF NOT EXISTS idx_vp_rare_p_s_o \
         ON _pg_ripple.vp_rare (p, s, o)",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("vp_rare (p,s,o) index creation error: {e}"));

    Spi::run_with_args(
        "CREATE INDEX IF NOT EXISTS idx_vp_rare_s_p \
         ON _pg_ripple.vp_rare (s, p)",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("vp_rare (s,p) index creation error: {e}"));

    Spi::run_with_args(
        "CREATE INDEX IF NOT EXISTS idx_vp_rare_g_p_s_o \
         ON _pg_ripple.vp_rare (g, p, s, o)",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("vp_rare (g,p,s,o) index creation error: {e}"));

    // Create the statements range-mapping catalog (v0.2.0, used by RDF-star in v0.4.0).
    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.statements ( \
             sid_min      BIGINT NOT NULL, \
             sid_max      BIGINT NOT NULL, \
             predicate_id BIGINT NOT NULL, \
             table_oid    OID    NOT NULL, \
             PRIMARY KEY  (sid_min) \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("statements catalog creation error: {e}"));

    // Create the IRI prefix registry.
    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.prefixes ( \
             prefix     TEXT NOT NULL PRIMARY KEY, \
             expansion  TEXT NOT NULL \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("prefixes table creation error: {e}"));

    // v0.6.0: HTAP pattern tables + CDC schema + predicates.htap column.
    merge::initialize_pattern_tables();
    crate::cdc::initialize_cdc_schema();

    // v0.52.0: CDC bridge triggers catalog.
    cdc_bridge::initialize_cdc_bridge_schema();

    // v0.58.0: Temporal RDF timeline table.
    crate::temporal::initialize_timeline_schema();

    // v0.58.0: PROV-O provenance catalog.
    crate::prov::initialize_prov_schema();

    // Note: the predicate_stats view is created via extension_sql in lib.rs,
    // not here, to avoid deadlocks when initialize_schema() is called from
    // concurrent test transactions.
}

/// Ensure a dedicated VP table (HTAP split) exists for `predicate_id`.
///
/// Returns the fully-qualified view name `_pg_ripple.vp_{id}`.
/// In v0.6.0+, this creates delta + main + tombstones + view.
pub(super) fn ensure_vp_table(predicate_id: i64) -> String {
    // Check whether a dedicated table/view already exists.
    let existing = match Spi::get_one_with_args::<String>(
        "SELECT '_pg_ripple.vp_' || id::text \
         FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL",
        &[DatumWithOid::from(predicate_id)],
    ) {
        Ok(Some(table)) => Some(table),
        Ok(None) => None,
        Err(_) => None,
    };

    if let Some(table) = existing {
        return table;
    }

    // Create the HTAP split (delta + main + tombstones + view).
    let view = merge::ensure_htap_tables(predicate_id);

    // RLS-01: apply graph RLS policies to the new delta and main tables.
    let delta = format!("_pg_ripple.vp_{predicate_id}_delta");
    let main = format!("_pg_ripple.vp_{predicate_id}_main");
    crate::security_api::apply_rls_to_vp_table(&delta);
    crate::security_api::apply_rls_to_vp_table(&main);

    // Install CDC trigger on the new delta table.
    crate::cdc::install_trigger(predicate_id);

    view
}
