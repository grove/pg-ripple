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
//! triples are initially stored in `_pg_ripple.vp_rare (p, s, o, g, i, source)`.
//! They are automatically promoted to a dedicated VP table once the threshold
//! is crossed.
//!
//! # Named graphs
//!
//! The default graph has identifier `0`.  Named graphs have positive `i64` ids.
//! Named graph management: `create_graph`, `drop_graph`, `list_graphs`.

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

/// Public wrapper for strip_angle_brackets — used by lib.rs.
pub fn strip_angle_brackets_pub(term: &str) -> &str {
    strip_angle_brackets(term)
}

/// Parse an N-Triples–style term and return `(clean_value, kind, datatype, lang)`.
/// Supports IRIs, blank nodes, plain/typed/lang literals.
pub fn parse_rdf_term(s: &str) -> (String, i16, Option<String>, Option<String>) {
    let s = s.trim();
    if s.starts_with('<') && s.ends_with('>') {
        return (
            s[1..s.len() - 1].to_owned(),
            dictionary::KIND_IRI,
            None,
            None,
        );
    }
    if let Some(rest) = s.strip_prefix("_:") {
        return (rest.to_owned(), dictionary::KIND_BLANK, None, None);
    }
    if s.starts_with('"') {
        // Find closing quote (handling \" escapes)
        let bytes = s.as_bytes();
        let mut i = 1usize;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
            } else if bytes[i] == b'"' {
                break;
            } else {
                i += 1;
            }
        }
        let raw_value = &s[1..i];
        let rest = &s[i + 1..];
        // Unescape basic sequences
        let value = raw_value
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\n", "\n")
            .replace("\\r", "\r")
            .replace("\\t", "\t");
        if rest.starts_with("^^<") && rest.ends_with('>') {
            let dt = rest[3..rest.len() - 1].to_owned();
            return (value, dictionary::KIND_TYPED_LITERAL, Some(dt), None);
        }
        if let Some(lang_part) = rest.strip_prefix('@') {
            return (
                value,
                dictionary::KIND_LANG_LITERAL,
                None,
                Some(lang_part.to_owned()),
            );
        }
        return (value, dictionary::KIND_LITERAL, None, None);
    }
    // Fall back: treat as a bare IRI string (v0.1.0 backward-compat)
    (s.to_owned(), dictionary::KIND_IRI, None, None)
}

/// Encode an RDF term string (N-Triples format) to a dictionary id.
pub fn encode_rdf_term(s: &str) -> i64 {
    let (value, kind, datatype, lang) = parse_rdf_term(s);
    match kind {
        k if k == dictionary::KIND_TYPED_LITERAL => {
            dictionary::encode_typed_literal(&value, datatype.as_deref().unwrap_or(""))
        }
        k if k == dictionary::KIND_LANG_LITERAL => {
            dictionary::encode_lang_literal(&value, lang.as_deref().unwrap_or(""))
        }
        _ => dictionary::encode(&value, kind),
    }
}

/// Initialize the extension's base schemas and tables.
/// Called once from _PG_init to ensure all base infrastructure exists.
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

    // Note: the predicate_stats view is created via extension_sql in lib.rs,
    // not here, to avoid deadlocks when initialize_schema() is called from
    // concurrent test transactions.
}

/// Get the current VP promotion threshold from the GUC.
fn vp_promotion_threshold() -> i64 {
    crate::VPP_THRESHOLD.get() as i64
}

/// Returns true if named_graph_optimized GUC is enabled.
fn named_graph_optimized() -> bool {
    crate::NAMED_GRAPH_OPTIMIZED.get()
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
        Err(_) => None,
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

    // Optional named-graph index when GUC is enabled.
    if named_graph_optimized() {
        Spi::run_with_args(
            &format!(
                "CREATE INDEX IF NOT EXISTS idx_vp_{predicate_id}_g_s_o \
                 ON {table} (g, s, o)"
            ),
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("VP table g-s-o index SPI error: {e}"));
    }

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

/// Check if a dedicated VP table exists for `predicate_id`.
fn get_dedicated_vp_table(predicate_id: i64) -> Option<String> {
    Spi::get_one_with_args::<String>(
        "SELECT '_pg_ripple.vp_' || id::text \
         FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL",
        &[DatumWithOid::from(predicate_id)],
    )
    .unwrap_or(None)
}

/// Insert a row into `_pg_ripple.vp_rare` and update the predicate count.
/// Returns the SID.
fn insert_into_vp_rare(p_id: i64, s_id: i64, o_id: i64, g: i64) -> i64 {
    let sid = Spi::get_one_with_args::<i64>(
        "INSERT INTO _pg_ripple.vp_rare (p, s, o, g) VALUES ($1, $2, $3, $4) RETURNING i",
        &[
            DatumWithOid::from(p_id),
            DatumWithOid::from(s_id),
            DatumWithOid::from(o_id),
            DatumWithOid::from(g),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("vp_rare insert SPI error: {e}"))
    .unwrap_or(0);

    Spi::run_with_args(
        "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count) \
         VALUES ($1, NULL, 1) \
         ON CONFLICT (id) DO UPDATE \
         SET triple_count = _pg_ripple.predicates.triple_count + 1",
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate count upsert SPI error: {e}"));

    sid
}

/// Promote a single predicate from vp_rare to its own VP table.
fn promote_predicate(p_id: i64) {
    let table = ensure_vp_table(p_id);

    // Move rows from vp_rare to the dedicated table.
    Spi::run_with_args(
        &format!(
            "INSERT INTO {table} (s, o, g, i, source) \
             SELECT s, o, g, i, source FROM _pg_ripple.vp_rare WHERE p = $1"
        ),
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate promotion insert SPI error: {e}"));

    Spi::run_with_args(
        "DELETE FROM _pg_ripple.vp_rare WHERE p = $1",
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate promotion delete SPI error: {e}"));
}

/// Promote all rare predicates that have reached the promotion threshold.
/// Called after bulk loads and optionally after single inserts.
pub fn promote_rare_predicates() -> i64 {
    let threshold = vp_promotion_threshold();

    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT p, count(*) as cnt FROM _pg_ripple.vp_rare GROUP BY p HAVING count(*) >= $1",
            None,
            &[DatumWithOid::from(threshold)],
        )
        .unwrap_or_else(|e| pgrx::error!("promote_rare_predicates query SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    let count = pred_ids.len() as i64;

    for p_id in pred_ids {
        promote_predicate(p_id);
    }

    count
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Allocate and return the next load generation ID (for blank node scoping).
pub fn next_load_generation() -> i64 {
    Spi::get_one::<i64>("SELECT nextval('_pg_ripple.load_generation_seq')")
        .unwrap_or_else(|e| pgrx::error!("load_generation_seq SPI error: {e}"))
        .unwrap_or(1)
}

/// Insert a triple `(s, p, o)` into graph `g`.
///
/// Routes to vp_rare for new/rare predicates; promotes when threshold is crossed.
/// Returns the globally-unique statement identifier (SID).
pub fn insert_triple(s: &str, p: &str, o: &str, g: i64) -> i64 {
    let s_id = encode_rdf_term(s);
    let p_id = dictionary::encode(strip_angle_brackets(p), dictionary::KIND_IRI);
    let o_id = encode_rdf_term(o);

    // Fast path: dedicated VP table already exists.
    if let Some(table) = get_dedicated_vp_table(p_id) {
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

        return sid;
    }

    // Slow path: insert into vp_rare, check for promotion.
    let sid = insert_into_vp_rare(p_id, s_id, o_id, g);

    // Check if threshold crossed — promote immediately for single inserts.
    let new_count: i64 = Spi::get_one_with_args::<i64>(
        "SELECT triple_count FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0);

    if new_count >= vp_promotion_threshold() {
        promote_predicate(p_id);
    }

    sid
}

/// Insert a triple that was pre-encoded (used by bulk loader for performance).
///
/// Routes to vp_rare or dedicated table based on current predicate state.
/// Does NOT check/trigger promotion (bulk load calls promote_rare_predicates at end).
#[allow(dead_code)]
pub fn insert_encoded_triple(s_id: i64, p_id: i64, o_id: i64, g: i64) -> i64 {
    if let Some(table) = get_dedicated_vp_table(p_id) {
        let sid = Spi::get_one_with_args::<i64>(
            &format!("INSERT INTO {table} (s, o, g) VALUES ($1, $2, $3) RETURNING i"),
            &[
                DatumWithOid::from(s_id),
                DatumWithOid::from(o_id),
                DatumWithOid::from(g),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("bulk insert SPI error: {e}"))
        .unwrap_or(0);

        Spi::run_with_args(
            "UPDATE _pg_ripple.predicates SET triple_count = triple_count + 1 WHERE id = $1",
            &[DatumWithOid::from(p_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));

        return sid;
    }

    insert_into_vp_rare(p_id, s_id, o_id, g)
}

/// Batch insert pre-encoded triples for a single predicate (bulk load performance).
///
/// Uses a VALUES-list INSERT to reduce SPI round-trips.
/// All values are i64 integers — no SQL injection risk.
pub fn batch_insert_encoded(p_id: i64, rows: &[(i64, i64, i64)]) -> i64 {
    if rows.is_empty() {
        return 0;
    }

    let table_opt = get_dedicated_vp_table(p_id);

    if let Some(ref table) = table_opt {
        // Build a multi-row VALUES insert (all i64 integers — injection-safe).
        let values: Vec<String> = rows
            .iter()
            .map(|(s, o, g)| format!("({},{},{})", s, o, g))
            .collect();
        let sql = format!("INSERT INTO {table} (s, o, g) VALUES {}", values.join(","));
        Spi::run_with_args(&sql, &[])
            .unwrap_or_else(|e| pgrx::error!("batch VP insert SPI error: {e}"));

        let cnt = rows.len() as i64;
        Spi::run_with_args(
            "UPDATE _pg_ripple.predicates SET triple_count = triple_count + $2 WHERE id = $1",
            &[DatumWithOid::from(p_id), DatumWithOid::from(cnt)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count batch update SPI error: {e}"));
    } else {
        // Insert into vp_rare in bulk.
        let values: Vec<String> = rows
            .iter()
            .map(|(s, o, g)| format!("({},{},{},{})", p_id, s, o, g))
            .collect();
        let sql = format!(
            "INSERT INTO _pg_ripple.vp_rare (p, s, o, g) VALUES {}",
            values.join(",")
        );
        Spi::run_with_args(&sql, &[])
            .unwrap_or_else(|e| pgrx::error!("batch vp_rare insert SPI error: {e}"));

        let cnt = rows.len() as i64;
        Spi::run_with_args(
            "INSERT INTO _pg_ripple.predicates (id, table_oid, triple_count) \
             VALUES ($1, NULL, $2) \
             ON CONFLICT (id) DO UPDATE \
             SET triple_count = _pg_ripple.predicates.triple_count + EXCLUDED.triple_count",
            &[DatumWithOid::from(p_id), DatumWithOid::from(cnt)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count batch upsert SPI error: {e}"));
    }

    rows.len() as i64
}

/// Delete a triple.  Returns the number of rows removed.
pub fn delete_triple(s: &str, p: &str, o: &str, g: i64) -> i64 {
    let s_id = encode_rdf_term(s);
    let p_id = dictionary::encode(strip_angle_brackets(p), dictionary::KIND_IRI);
    let o_id = encode_rdf_term(o);

    let mut deleted = 0i64;

    // Try dedicated VP table first.
    if let Some(table) = get_dedicated_vp_table(p_id) {
        let d = Spi::get_one_with_args::<i64>(
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

        if d > 0 {
            Spi::run_with_args(
                "UPDATE _pg_ripple.predicates \
                 SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
                &[DatumWithOid::from(p_id), DatumWithOid::from(d)],
            )
            .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
            deleted += d;
        }
    }

    // Also try vp_rare.
    let d = Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.vp_rare WHERE p=$1 AND s=$2 AND o=$3 AND g=$4 RETURNING 1) \
         SELECT count(*)::bigint FROM d",
        &[
            DatumWithOid::from(p_id),
            DatumWithOid::from(s_id),
            DatumWithOid::from(o_id),
            DatumWithOid::from(g),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("vp_rare delete SPI error: {e}"))
    .unwrap_or(0);

    if d > 0 {
        Spi::run_with_args(
            "UPDATE _pg_ripple.predicates \
             SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
            &[DatumWithOid::from(p_id), DatumWithOid::from(d)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
        deleted += d;
    }

    deleted
}

/// Return the sum of `triple_count` across all predicate catalog entries.
pub fn total_triple_count() -> i64 {
    Spi::get_one::<i64>("SELECT COALESCE(SUM(triple_count), 0)::bigint FROM _pg_ripple.predicates")
        .unwrap_or_else(|e| pgrx::error!("triple_count SPI error: {e}"))
        .unwrap_or(0)
}

/// Find triples matching the supplied pattern (includes vp_rare).
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
    let mut results = Vec::new();

    let s_id = s.map(encode_rdf_term);
    let o_id = o.map(encode_rdf_term);

    if let Some(p_str) = p {
        let p_id = dictionary::encode(strip_angle_brackets(p_str), dictionary::KIND_IRI);

        // Check dedicated VP table.
        if let Some(table) = get_dedicated_vp_table(p_id) {
            results.extend(scan_vp_table(&table, p_id, s_id, o_id, g));
        }
        // Also check vp_rare.
        results.extend(scan_vp_rare(Some(p_id), s_id, o_id, g));
    } else {
        // Scan all dedicated VP tables.
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

        for pid in pred_ids {
            let table = format!("_pg_ripple.vp_{pid}");
            results.extend(scan_vp_table(&table, pid, s_id, o_id, g));
        }

        // Scan vp_rare for remaining triples.
        results.extend(scan_vp_rare(None, s_id, o_id, g));
    }

    results
}

/// Collect all (s_id, p_id, o_id, g_id) from all VP tables (for export).
pub fn all_encoded_triples(graph: Option<i64>) -> Vec<(i64, i64, i64, i64)> {
    let mut results = Vec::new();

    // Dedicated VP tables.
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

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        let g_filter = match graph {
            Some(gid) => format!(" WHERE g = {}", gid),
            None => String::new(),
        };
        let sql = format!("SELECT s, o, g FROM {table}{g_filter}");
        let rows: Vec<(i64, i64, i64)> = Spi::connect(|c| {
            c.select(&sql, None, &[])
                .unwrap_or_else(|e| pgrx::error!("all_encoded_triples VP scan SPI error: {e}"))
                .filter_map(|row| {
                    let s: Option<i64> = row.get(1).ok().flatten();
                    let o: Option<i64> = row.get(2).ok().flatten();
                    let g: Option<i64> = row.get(3).ok().flatten();
                    match (s, o, g) {
                        (Some(s), Some(o), Some(g)) => Some((s, o, g)),
                        _ => None,
                    }
                })
                .collect()
        });
        for (s, o, g_val) in rows {
            results.push((s, p_id, o, g_val));
        }
    }

    // vp_rare.
    let g_filter = match graph {
        Some(gid) => format!(" WHERE g = {}", gid),
        None => String::new(),
    };
    let sql = format!("SELECT p, s, o, g FROM _pg_ripple.vp_rare{g_filter}");
    let rare_rows: Vec<(i64, i64, i64, i64)> = Spi::connect(|c| {
        c.select(&sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("all_encoded_triples vp_rare scan SPI error: {e}"))
            .filter_map(|row| {
                let p: Option<i64> = row.get(1).ok().flatten();
                let s: Option<i64> = row.get(2).ok().flatten();
                let o: Option<i64> = row.get(3).ok().flatten();
                let g: Option<i64> = row.get(4).ok().flatten();
                match (p, s, o, g) {
                    (Some(p), Some(s), Some(o), Some(g)) => Some((s, p, o, g)),
                    _ => None,
                }
            })
            .collect()
    });
    results.extend(rare_rows);

    results
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

    let p_str = dictionary::format_ntriples(p_id);
    let g_str = if g == 0 {
        String::new()
    } else {
        dictionary::format_ntriples(g)
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
                let s_str = dictionary::format_ntriples(s_enc);
                let o_str = dictionary::format_ntriples(o_enc);
                Some((s_str, p_str.clone(), o_str, g_str.clone()))
            } else {
                None
            }
        })
        .collect()
    })
}

/// Scan vp_rare with optional predicate, subject, object, graph filters.
fn scan_vp_rare(
    p_id: Option<i64>,
    s_id: Option<i64>,
    o_id: Option<i64>,
    g: i64,
) -> Vec<(String, String, String, String)> {
    let mut conditions = vec!["g = $4".to_string()];
    if p_id.is_some() {
        conditions.push("p = $1".to_string());
    }
    if s_id.is_some() {
        conditions.push("s = $2".to_string());
    }
    if o_id.is_some() {
        conditions.push("o = $3".to_string());
    }
    let where_clause = format!("WHERE {}", conditions.join(" AND "));
    let sql = format!("SELECT p, s, o FROM _pg_ripple.vp_rare {where_clause}");

    let g_str = if g == 0 {
        String::new()
    } else {
        dictionary::format_ntriples(g)
    };

    Spi::connect(|c| {
        c.select(
            &sql,
            None,
            &[
                DatumWithOid::from(p_id.unwrap_or(0)),
                DatumWithOid::from(s_id.unwrap_or(0)),
                DatumWithOid::from(o_id.unwrap_or(0)),
                DatumWithOid::from(g),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("vp_rare scan SPI error: {e}"))
        .filter_map(|row| {
            let p_val: Option<i64> = row.get(1).ok().flatten();
            let s_val: Option<i64> = row.get(2).ok().flatten();
            let o_val: Option<i64> = row.get(3).ok().flatten();
            if let (Some(pe), Some(se), Some(oe)) = (p_val, s_val, o_val) {
                Some((
                    dictionary::format_ntriples(se),
                    dictionary::format_ntriples(pe),
                    dictionary::format_ntriples(oe),
                    g_str.clone(),
                ))
            } else {
                None
            }
        })
        .collect()
    })
}

// ─── Named Graph Management ───────────────────────────────────────────────────

/// Encode a named graph IRI and return its dictionary id.
/// This is idempotent — calling it again returns the same id.
pub fn create_graph(graph_iri: &str) -> i64 {
    dictionary::encode(strip_angle_brackets(graph_iri), dictionary::KIND_IRI)
}

/// Drop all triples in a named graph.  Returns the number of triples deleted.
pub fn drop_graph(graph_iri: &str) -> i64 {
    let g_id = dictionary::encode(strip_angle_brackets(graph_iri), dictionary::KIND_IRI);

    let mut deleted = 0i64;

    // Delete from all dedicated VP tables.
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

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        let d = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH d AS (DELETE FROM {table} WHERE g = $1 RETURNING 1) \
                 SELECT count(*)::bigint FROM d"
            ),
            &[DatumWithOid::from(g_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("drop_graph VP delete SPI error: {e}"))
        .unwrap_or(0);

        if d > 0 {
            Spi::run_with_args(
                "UPDATE _pg_ripple.predicates \
                 SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
                &[DatumWithOid::from(p_id), DatumWithOid::from(d)],
            )
            .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
            deleted += d;
        }
    }

    // Delete from vp_rare.
    let d = Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.vp_rare WHERE g = $1 RETURNING p) \
         SELECT count(*)::bigint FROM d",
        &[DatumWithOid::from(g_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("drop_graph vp_rare delete SPI error: {e}"))
    .unwrap_or(0);
    deleted += d;

    deleted
}

/// List all named graph IRIs (excludes the default graph 0).
pub fn list_graphs() -> Vec<String> {
    // Collect distinct g values > 0 from all VP tables and vp_rare, decode them.
    let mut g_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();

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

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        let ids: Vec<i64> = Spi::connect(|c| {
            c.select(
                &format!("SELECT DISTINCT g FROM {table} WHERE g > 0"),
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("list_graphs VP scan SPI error: {e}"))
            .filter_map(|row| row.get::<i64>(1).ok().flatten())
            .collect()
        });
        for id in ids {
            g_ids.insert(id);
        }
    }

    let rare_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT DISTINCT g FROM _pg_ripple.vp_rare WHERE g > 0",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("list_graphs vp_rare scan SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });
    for id in rare_ids {
        g_ids.insert(id);
    }

    let mut graphs: Vec<String> = g_ids
        .into_iter()
        .filter_map(dictionary::decode)
        .map(|iri| format!("<{}>", iri))
        .collect();
    graphs.sort();
    graphs
}

// ─── IRI Prefix Management ────────────────────────────────────────────────────

/// Register (or update) an IRI prefix abbreviation.
pub fn register_prefix(prefix: &str, expansion: &str) {
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.prefixes (prefix, expansion) VALUES ($1, $2) \
         ON CONFLICT (prefix) DO UPDATE SET expansion = EXCLUDED.expansion",
        &[DatumWithOid::from(prefix), DatumWithOid::from(expansion)],
    )
    .unwrap_or_else(|e| pgrx::error!("register_prefix SPI error: {e}"));
}

/// Return all registered prefix → expansion pairs.
pub fn list_prefixes() -> Vec<(String, String)> {
    Spi::connect(|c| {
        c.select(
            "SELECT prefix, expansion FROM _pg_ripple.prefixes ORDER BY prefix",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("list_prefixes SPI error: {e}"))
        .filter_map(|row| {
            let prefix: Option<String> = row.get(1).ok().flatten();
            let expansion: Option<String> = row.get(2).ok().flatten();
            match (prefix, expansion) {
                (Some(p), Some(e)) => Some((p, e)),
                _ => None,
            }
        })
        .collect()
    })
}

// ─── Statement Identifier API (v0.4.0) ────────────────────────────────────────

/// Look up a statement by its globally-unique statement identifier (SID).
///
/// Searches the `_pg_ripple.statements` range-mapping catalog first, then
/// falls back to a brute-force scan if the catalog is empty.
/// Returns decoded N-Triples–formatted `(s, p, o, g)` strings, or `None`.
pub fn get_statement_by_sid(sid: i64) -> Option<(String, String, String, String)> {
    // Try the range mapping catalog first (fast path).
    let pred_from_catalog: Option<i64> = Spi::connect(|c| {
        c.select(
            "SELECT predicate_id \
             FROM _pg_ripple.statements \
             WHERE sid_min <= $1 AND sid_max >= $1 \
             ORDER BY sid_min DESC LIMIT 1",
            Some(1),
            &[DatumWithOid::from(sid)],
        )
        .ok()
        .and_then(|rows| {
            rows.filter_map(|row| row.get::<i64>(1).ok().flatten()).next()
        })
    });

    if let Some(p_id) = pred_from_catalog {
        let table = format!("_pg_ripple.vp_{p_id}");
        if let Some((s_id, o_id, g_id)) = fetch_sog_by_sid(&table, sid) {
            return Some(decode_sog(s_id, p_id, o_id, g_id));
        }
    }

    // Fallback: scan all dedicated VP tables for the SID.
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

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        if let Some((s_id, o_id, g_id)) = fetch_sog_by_sid(&table, sid) {
            return Some(decode_sog(s_id, p_id, o_id, g_id));
        }
    }

    // Also check vp_rare.
    Spi::connect(|c| {
        c.select(
            "SELECT s, p, o, g FROM _pg_ripple.vp_rare WHERE i = $1 LIMIT 1",
            Some(1),
            &[DatumWithOid::from(sid)],
        )
        .ok()
        .and_then(|rows| {
            rows.filter_map(|row| {
                let s = row.get::<i64>(1).ok().flatten()?;
                let p = row.get::<i64>(2).ok().flatten()?;
                let o = row.get::<i64>(3).ok().flatten()?;
                let g = row.get::<i64>(4).ok().flatten()?;
                Some(decode_sog(s, p, o, g))
            })
            .next()
        })
    })
}

/// Fetch `(s_id, o_id, g_id)` from a VP table by SID.
fn fetch_sog_by_sid(table: &str, sid: i64) -> Option<(i64, i64, i64)> {
    Spi::connect(|c| {
        c.select(
            &format!("SELECT s, o, g FROM {table} WHERE i = $1 LIMIT 1"),
            Some(1),
            &[DatumWithOid::from(sid)],
        )
        .ok()
        .and_then(|rows| {
            rows.filter_map(|row| {
                let s = row.get::<i64>(1).ok().flatten()?;
                let o = row.get::<i64>(2).ok().flatten()?;
                let g = row.get::<i64>(3).ok().flatten()?;
                Some((s, o, g))
            })
            .next()
        })
    })
}

/// Decode `(s_id, p_id, o_id, g_id)` to N-Triples strings.
fn decode_sog(s_id: i64, p_id: i64, o_id: i64, g_id: i64) -> (String, String, String, String) {
    (
        dictionary::format_ntriples(s_id),
        dictionary::format_ntriples(p_id),
        dictionary::format_ntriples(o_id),
        if g_id == 0 {
            String::new()
        } else {
            dictionary::format_ntriples(g_id)
        },
    )
}
