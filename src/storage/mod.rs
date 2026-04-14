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

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

use crate::dictionary;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Parse a bare IRI `<…>` or return the term as-is for encode dispatch.
fn strip_angle_brackets(term: &str) -> &str {
    let t = term.trim();
    if t.starts_with('<') && t.ends_with('>') {
        &t[1..t.len() - 1]
    } else {
        t
    }
}

/// Initialize the extension's base schemas and tables.
/// Called once from _PG_init to ensure all base infrastructure exists.
pub fn initialize_schema() {
    // Create the user-visible schema if it doesn't exist.
    // NOTE: pg_ripple starts with pg_ which is reserved; the extension's
    // bootstrap SQL (SET LOCAL allow_system_table_mods = on) enables creation
    // during CREATE EXTENSION.  In subsequent calls (e.g. after server restart),
    // the schema already exists so IF NOT EXISTS is a no-op.  We set
    // allow_system_table_mods locally here for that "already exists" fast-path;
    // the actual creation was done during CREATE EXTENSION.
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
         ON _pg_ripple.vp_rare (p, s, o); \
         CREATE INDEX IF NOT EXISTS idx_vp_rare_p_o_s \
         ON _pg_ripple.vp_rare (p, o, s)",
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("vp_rare indexes creation error: {e}"));
}

/// Ensure a dedicated VP table exists for `predicate_id`.
///
/// Returns the fully-qualified table name `_pg_ripple.vp_{id}`.
fn ensure_vp_table(predicate_id: i64) -> String {
    // Check whether a dedicated table already exists.
    let existing = match Spi::get_one_with_args::<String>(
        "SELECT '_pg_ripple.vp_' || id::text \
         FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL",
        &[DatumWithOid::from(predicate_id)],
    ) {
        Ok(Some(table)) => Some(table),
        Ok(None) => None,
        Err(_) => None, // Query returned no rows or SPI error; treat as non-existent
    };

    if let Some(table) = existing {
        return table;
    }

    let table = format!("_pg_ripple.vp_{predicate_id}");

    Spi::run_with_args(
        &format!(
            "CREATE TABLE IF NOT EXISTS {table} ( \
                 s      BIGINT NOT NULL, \
                 o      BIGINT NOT NULL, \
                 g      BIGINT NOT NULL DEFAULT 0, \
                 i      BIGINT NOT NULL DEFAULT nextval('_pg_ripple.statement_id_seq'), \
                 source SMALLINT NOT NULL DEFAULT 0 \
             )"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("VP table creation SPI error: {e}"));

    // Create indexes separately
    Spi::run_with_args(
        &format!(
            "CREATE INDEX IF NOT EXISTS idx_vp_{predicate_id}_s_o \
         ON {table} (s, o)"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("VP table index 1 SPI error: {e}"));

    Spi::run_with_args(
        &format!(
            "CREATE INDEX IF NOT EXISTS idx_vp_{predicate_id}_o_s \
         ON {table} (o, s)"
        ),
        &[],
    )
    .unwrap_or_else(|e| pgrx::error!("VP table index 2 SPI error: {e}"));

    Spi::run_with_args(
        "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count) \
         VALUES ($1, $2::regclass::oid, 0) \
         ON CONFLICT (id) DO UPDATE SET table_oid = EXCLUDED.table_oid",
        &[
            DatumWithOid::from(predicate_id),
            DatumWithOid::from(table.as_str()),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate catalog insert SPI error: {e}"));

    table
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
        &format!("INSERT INTO {table} (s, o, g) VALUES ($1, $2, $3) RETURNING i"),
        &[
            DatumWithOid::from(s_id),
            DatumWithOid::from(o_id),
            DatumWithOid::from(g),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("triple insert SPI error: {e}"))
    .unwrap_or(0);

    Spi::run_with_args(
        "UPDATE _pg_ripple.predicates SET triple_count = triple_count + 1 WHERE id = $1",
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));

    sid
}

/// Delete a triple.  Returns the number of rows removed.
pub fn delete_triple(s: &str, p: &str, o: &str, g: i64) -> i64 {
    let s_id = dictionary::encode(strip_angle_brackets(s), dictionary::KIND_IRI);
    let p_id = dictionary::encode(strip_angle_brackets(p), dictionary::KIND_IRI);
    let o_id = dictionary::encode(strip_angle_brackets(o), dictionary::KIND_IRI);

    let existing = Spi::get_one_with_args::<String>(
        "SELECT '_pg_ripple.vp_' || id::text \
         FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL",
        &[DatumWithOid::from(p_id)],
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
        &[
            DatumWithOid::from(s_id),
            DatumWithOid::from(o_id),
            DatumWithOid::from(g),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("triple delete SPI error: {e}"))
    .unwrap_or(0);

    if deleted > 0 {
        Spi::run_with_args(
            "UPDATE _pg_ripple.predicates \
             SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
            &[DatumWithOid::from(p_id), DatumWithOid::from(deleted)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
    }

    deleted
}

/// Return the sum of `triple_count` across all predicate catalog entries.
pub fn total_triple_count() -> i64 {
    Spi::get_one::<i64>("SELECT COALESCE(SUM(triple_count), 0)::bigint FROM _pg_ripple.predicates")
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

    if let Some(p_str) = p {
        let p_id = dictionary::encode(strip_angle_brackets(p_str), dictionary::KIND_IRI);
        let table_opt = Spi::get_one_with_args::<String>(
            "SELECT '_pg_ripple.vp_' || id::text \
             FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL",
            &[DatumWithOid::from(p_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate lookup SPI error: {e}"));

        let Some(table) = table_opt else {
            return vec![];
        };

        let s_id = s.map(|v| dictionary::encode(strip_angle_brackets(v), dictionary::KIND_IRI));
        let o_id = o.map(|v| dictionary::encode(strip_angle_brackets(v), dictionary::KIND_IRI));

        scan_vp_table(&table, p_id, s_id, o_id, g)
    } else {
        let pred_ids: Vec<i64> = Spi::connect(|c| {
            c.select(
                "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("predicates scan SPI error: {e}"))
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
    let mut conditions = vec!["g = $3".to_string()];
    if s_id.is_some() {
        conditions.push("s = $1".to_string());
    }
    if o_id.is_some() {
        conditions.push("o = $2".to_string());
    }
    let where_clause = format!("WHERE {}", conditions.join(" AND "));

    let sql = format!("SELECT s, o FROM {table} {where_clause}");

    let p_str = dictionary::decode(p_id).unwrap_or_default();
    let g_str = if g == 0 {
        String::new()
    } else {
        dictionary::decode(g).unwrap_or_default()
    };

    Spi::connect(|c| {
        c.select(
            &sql,
            None,
            &[
                DatumWithOid::from(s_id.unwrap_or(0)),
                DatumWithOid::from(o_id.unwrap_or(0)),
                DatumWithOid::from(g),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("VP table scan SPI error: {e}"))
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
