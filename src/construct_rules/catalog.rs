//! Catalog bootstrap — ensure the `_pg_ripple.construct_rules` table exists.

use pgrx::prelude::*;

// ─── Catalog bootstrap ────────────────────────────────────────────────────────

/// Ensure the construct-rule catalog tables exist (idempotent).
///
/// Called lazily by every public function that touches the construct-rule
/// catalog.  Adds v0.65.0 observability columns when upgrading from v0.63.0.
pub(super) fn ensure_catalog() {
    Spi::run(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.construct_rules (
            name                    TEXT PRIMARY KEY,
            sparql                  TEXT NOT NULL,
            generated_sql           TEXT,
            target_graph_id         BIGINT NOT NULL,
            mode                    TEXT NOT NULL DEFAULT 'incremental',
            source_graphs           TEXT[],
            source_graph_ids        BIGINT[],
            rule_order              INT,
            created_at              TIMESTAMPTZ DEFAULT now(),
            last_refreshed          TIMESTAMPTZ,
            last_incremental_run    TIMESTAMPTZ,
            successful_run_count    BIGINT NOT NULL DEFAULT 0,
            failed_run_count        BIGINT NOT NULL DEFAULT 0,
            last_error              TEXT,
            derived_triple_count    BIGINT NOT NULL DEFAULT 0,
            id                      BIGINT GENERATED ALWAYS AS IDENTITY
        )",
    )
    .unwrap_or_else(|e| pgrx::warning!("construct_rules catalog creation: {e}"));

    // Add v0.65.0 observability columns if upgrading from older schema.
    for (col, def) in &[
        ("last_incremental_run", "TIMESTAMPTZ"),
        ("successful_run_count", "BIGINT NOT NULL DEFAULT 0"),
        ("failed_run_count", "BIGINT NOT NULL DEFAULT 0"),
        ("last_error", "TEXT"),
        ("derived_triple_count", "BIGINT NOT NULL DEFAULT 0"),
        // v0.74.0 SCHEMA-NORM-01/05 additions.
        ("id", "BIGINT GENERATED ALWAYS AS IDENTITY"),
        ("source_graph_ids", "BIGINT[]"),
    ] {
        let _ = Spi::run(&format!(
            "ALTER TABLE _pg_ripple.construct_rules ADD COLUMN IF NOT EXISTS {col} {def}"
        ));
    }
    // SCHEMA-NORM-04: drop target_graph TEXT if it still exists (idempotent).
    let _ = Spi::run("ALTER TABLE _pg_ripple.construct_rules DROP COLUMN IF EXISTS target_graph");

    Spi::run(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.construct_rule_triples (
            rule_name TEXT   NOT NULL,
            pred_id   BIGINT NOT NULL,
            s         BIGINT NOT NULL,
            o         BIGINT NOT NULL,
            g         BIGINT NOT NULL,
            rule_id   BIGINT,
            PRIMARY KEY (rule_name, pred_id, s, o, g)
        )",
    )
    .unwrap_or_else(|e| pgrx::warning!("construct_rule_triples catalog creation: {e}"));
    let _ = Spi::run(
        "ALTER TABLE _pg_ripple.construct_rule_triples \
         ADD COLUMN IF NOT EXISTS rule_id BIGINT",
    );
}
