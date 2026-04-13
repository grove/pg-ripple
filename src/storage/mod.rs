//! Storage engine — VP table management and triple CRUD.
//!
//! # VP table layout
//!
//! Each unique predicate gets its own table:
//! ```sql
//! CREATE TABLE _pg_ripple.vp_{predicate_id} (
//!     s      BIGINT NOT NULL,
//!     o      BIGINT NOT NULL,
//!     g      BIGINT NOT NULL DEFAULT 0,
//!     i      BIGINT NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq'),
//!     source SMALLINT NOT NULL DEFAULT 0     -- 0 = explicit, 1 = inferred
//! );
//! CREATE INDEX ON _pg_ripple.vp_{id} (s, o);
//! CREATE INDEX ON _pg_ripple.vp_{id} (o, s);
//! ```
//!
//! Predicates with fewer than `pg_ripple.vp_promotion_threshold` (default 1 000)
//! triples are stored in `_pg_ripple.vp_rare (p, s, o, g, i, source)` instead.
//!
//! # Default graph
//!
//! The default graph has identifier `0`.  Named graphs have positive `i64` ids.

use pgrx::prelude::*;

use crate::dictionary;

// ─── Internal helpers ────────────────────────────────────────────────────────

/// Look up the VP table name for a predicate id, or `None` if it lives in vp_rare.
fn vp_table_name(predicate_id: i64) -> Option<String> {
    Spi::get_one_with_args::<pg_sys::Oid>(
        "SELECT table_oid FROM _pg_ripple.predicates WHERE id = $1",
        vec![(PgBuiltInOids::INT8OID.oid(), predicate_id.into_datum())],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate catalog SPI error: {e}"))
    .map(|oid| {
        Spi::get_one_with_args::<String>(
            "SELECT relname::text FROM pg_catalog.pg_class WHERE oid = $1",
            vec![(PgBuiltInOids::OIDOID.oid(), oid.into_datum())],
        )
        .unwrap_or_else(|e| pgrx::error!("pg_class lookup SPI error: {e}"))
    })
    .flatten()
}

/// Ensure a dedicated VP table exists for `predicate_id`.
///
/// If the predicate has fewer than `vp_promotion_threshold` rows it stays in
/// `vp_rare`; once it crosses the threshold this function creates the dedicated
/// table and migrates the rows (promotion logic added in v0.2.0).
fn ensure_vp_table(predicate_id: i64) -> String {
    // Check whether a dedicated table already exists.
    let existing = Spi::get_one_with_args::<String>(
        "SELECT '_pg_ripple.vp_' || id::text \
         FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL",
        vec![(PgBuiltInOids::INT8OID.oid(), predicate_id.into_datum())],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate lookup SPI error: {e}"));

    if let Some(table) = existing {
        return table;
    }

    // Create a new VP table for this predicate.
    let table = format!("_pg_ripple.vp_{}", predicate_id);
    Spi::run_with_args(
        &format!(
            "CREATE TABLE IF NOT EXISTS {table} ( \
                 s      BIGINT NOT NULL, \
                 o      BIGINT NOT NULL, \
                 g      BIGINT NOT NULL DEFAULT 0, \
                 i      BIGINT NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq'), \
                 source SMALLINT NOT NULL DEFAULT 0 \
             ); \
             CREATE INDEX IF NOT EXISTS ON {table} (s, o); \
             CREATE INDEX IF NOT EXISTS ON {table} (o, s)",
        ),
        None,
    )
    .unwrap_or_else(|e| pgrx::error!("VP table creation SPI error: {e}"));

    // Register in the predicate catalog.
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count) \
         VALUES ($1, $2::regclass::oid, 0) \
         ON CONFLICT (id) DO UPDATE SET table_oid = EXCLUDED.table_oid",
        vec![
            (PgBuiltInOids::INT8OID.oid(), predicate_id.into_datum()),
            (PgBuiltInOids::TEXTOID.oid(), table.clone().into_datum()),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate catalog insert SPI error: {e}"));

    table
}

/// Parse a bare IRI `<…>` or return the term as-is for encode dispatch.
fn strip_angle_brackets(term: &str) -> &str {
    let t = term.trim();
    if t.starts_with('<') && t.ends_with('>') {
        &t[1..t.len() - 1]
    } else {
        t
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Insert a triple `(s, p, o)` into graph `g`.
///
/// Returns the globally-unique statement identifier (SID) assigned to the row.
pub fn insert_triple(s: &str, p: &str, o: &str, g: i64) -> i64 {
    let s_id = dictionary::encode(strip_angle_brackets(s), dictionary::KIND_IRI);
    let p_id = dictionary::encode(strip_angle_brackets(p), dictionary::KIND_IRI);
    let o_id = dictionary::encode(strip_angle_brackets(o), dictionary::KIND_IRI);

    let table = ensure_vp_table(p_id);

    let sid = Spi::get_one_with_args::<i64>(
        &format!(
            "INSERT INTO {table} (s, o, g) VALUES ($1, $2, $3) RETURNING i"
        ),
        vec![
            (PgBuiltInOids::INT8OID.oid(), s_id.into_datum()),
            (PgBuiltInOids::INT8OID.oid(), o_id.into_datum()),
            (PgBuiltInOids::INT8OID.oid(), g.into_datum()),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("triple insert SPI error: {e}"))
    .unwrap_or(0);

    // Increment the predicate's triple count in the catalog.
    Spi::run_with_args(
        "UPDATE _pg_ripple.predicates SET triple_count = triple_count + 1 WHERE id = $1",
        vec![(PgBuiltInOids::INT8OID.oid(), p_id.into_datum())],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));

    sid
}

/// Delete a triple.  Returns the number of rows removed.
pub fn delete_triple(s: &str, p: &str, o: &str, g: i64) -> i64 {
    let s_id = dictionary::encode(strip_angle_brackets(s), dictionary::KIND_IRI);
    let p_id = dictionary::encode(strip_angle_brackets(p), dictionary::KIND_IRI);
    let o_id = dictionary::encode(strip_angle_brackets(o), dictionary::KIND_IRI);

    // If there's no VP table for this predicate, there's nothing to delete.
    let existing = Spi::get_one_with_args::<String>(
        "SELECT '_pg_ripple.vp_' || id::text \
         FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL",
        vec![(PgBuiltInOids::INT8OID.oid(), p_id.into_datum())],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate lookup SPI error: {e}"));

    let Some(table) = existing else {
        return 0;
    };

    let deleted = Spi::get_one_with_args::<i64>(
        &format!(
            "WITH d AS (DELETE FROM {table} WHERE s=$1 AND o=$2 AND g=$3 RETURNING 1) \
             SELECT count(*)::bigint FROM d"
        ),
        vec![
            (PgBuiltInOids::INT8OID.oid(), s_id.into_datum()),
            (PgBuiltInOids::INT8OID.oid(), o_id.into_datum()),
            (PgBuiltInOids::INT8OID.oid(), g.into_datum()),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("triple delete SPI error: {e}"))
    .unwrap_or(0);

    if deleted > 0 {
        Spi::run_with_args(
            "UPDATE _pg_ripple.predicates SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
            vec![
                (PgBuiltInOids::INT8OID.oid(), p_id.into_datum()),
                (PgBuiltInOids::INT8OID.oid(), deleted.into_datum()),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
    }

    deleted
}

/// Return the sum of `triple_count` across all predicate catalog entries.
pub fn total_triple_count() -> i64 {
    Spi::get_one::<i64>(
        "SELECT COALESCE(SUM(triple_count), 0)::bigint FROM _pg_ripple.predicates",
    )
    .unwrap_or_else(|e| pgrx::error!("triple_count SPI error: {e}"))
    .unwrap_or(0)
}

/// Find triples matching the supplied pattern.
///
/// Any argument may be `None` to act as a wildcard.  Returns decoded text tuples
/// `(s, p, o, g)` in the default graph unless `graph` is supplied.
pub fn find_triples(
    s: Option<&str>,
    p: Option<&str>,
    o: Option<&str>,
    graph: Option<i64>,
) -> Vec<(String, String, String, String)> {
    let g = graph.unwrap_or(0);

    // If the predicate is bound, we can limit the scan to one VP table.
    if let Some(p_str) = p {
        let p_id = dictionary::encode(strip_angle_brackets(p_str), dictionary::KIND_IRI);
        let table_opt = Spi::get_one_with_args::<String>(
            "SELECT '_pg_ripple.vp_' || id::text \
             FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL",
            vec![(PgBuiltInOids::INT8OID.oid(), p_id.into_datum())],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate lookup SPI error: {e}"));

        let Some(table) = table_opt else {
            return vec![];
        };

        let s_id = s.map(|v| dictionary::encode(strip_angle_brackets(v), dictionary::KIND_IRI));
        let o_id = o.map(|v| dictionary::encode(strip_angle_brackets(v), dictionary::KIND_IRI));

        scan_vp_table(&table, p_id, s_id, o_id, g)
    } else {
        // No predicate bound: scan all VP tables.
        let pred_ids: Vec<i64> = Spi::connect(|c| {
            let tup_table = c
                .select("SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL", None, None)
                .unwrap_or_else(|e| pgrx::error!("predicates scan SPI error: {e}"));
            tup_table
                .filter_map(|row| row.get::<i64>(1).ok().flatten())
                .collect()
        });

        let s_id = s.map(|v| dictionary::encode(strip_angle_brackets(v), dictionary::KIND_IRI));
        let o_id = o.map(|v| dictionary::encode(strip_angle_brackets(v), dictionary::KIND_IRI));

        pred_ids
            .into_iter()
            .flat_map(|pid| {
                let table = format!("_pg_ripple.vp_{pid}");
                scan_vp_table(&table, pid, s_id, o_id, g)
            })
            .collect()
    }
}

/// Scan a single VP table and decode results to text tuples.
fn scan_vp_table(
    table: &str,
    p_id: i64,
    s_id: Option<i64>,
    o_id: Option<i64>,
    g: i64,
) -> Vec<(String, String, String, String)> {
    // Build WHERE clause dynamically based on which axes are bound.
    let mut conditions = vec!["g = $3".to_string()];
    if s_id.is_some() {
        conditions.push("s = $1".to_string());
    }
    if o_id.is_some() {
        conditions.push("o = $2".to_string());
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!("SELECT s, o FROM {table} {where_clause}");
    let args = vec![
        (PgBuiltInOids::INT8OID.oid(), s_id.unwrap_or(0).into_datum()),
        (PgBuiltInOids::INT8OID.oid(), o_id.unwrap_or(0).into_datum()),
        (PgBuiltInOids::INT8OID.oid(), g.into_datum()),
    ];

    let p_str = dictionary::decode(p_id).unwrap_or_default();
    let g_str = if g == 0 {
        String::from("")
    } else {
        dictionary::decode(g).unwrap_or_default()
    };

    Spi::connect(|c| {
        let tup_table = c
            .select(&sql, None, Some(args))
            .unwrap_or_else(|e| pgrx::error!("VP table scan SPI error: {e}"));
        tup_table
            .filter_map(|row| {
                let s_val: Option<i64> = row.get(1).ok().flatten();
                let o_val: Option<i64> = row.get(2).ok().flatten();
                if let (Some(s_enc), Some(o_enc)) = (s_val, o_val) {
                    let s_str = dictionary::decode(s_enc).unwrap_or_default();
                    let o_str = dictionary::decode(o_enc).unwrap_or_default();
                    Some((s_str, p_str.clone(), o_str, g_str.clone()))
                } else {
                    None
                }
            })
            .collect()
    })
}
