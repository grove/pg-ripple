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
pub mod merge;

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

use crate::dictionary;

// ─── In-update deduplication tracking ────────────────────────────────────────

// Thread-local set tracking (p, s, o, g) quads inserted during the current

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
    let s = s.trim();
    // v0.48.0: Handle RDF-star quoted triple syntax `<< s p o >>`.
    if s.starts_with("<<") && s.ends_with(">>") {
        let inner = s[2..s.len() - 2].trim();
        let tokens = tokenize_rdf_terms(inner);
        if tokens.len() >= 3 {
            let s_id = encode_rdf_term(&tokens[0]);
            let p_id = encode_rdf_term(&tokens[1]);
            // Object may span multiple tokens (e.g. typed literal with spaces)
            let o_str = if tokens.len() == 3 {
                tokens[2].clone()
            } else {
                tokens[2..].join(" ")
            };
            let o_id = encode_rdf_term(&o_str);
            return dictionary::encode_quoted_triple(s_id, p_id, o_id);
        }
    }
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

/// Tokenize a space-separated sequence of N-Triples terms, respecting IRIs,
/// quoted literals and nested `<< >>` quoted triples.
fn tokenize_rdf_terms(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_literal = false;
    let mut in_iri = false;
    let mut quoted_depth: usize = 0;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' if !in_iri => {
                in_literal = !in_literal;
                current.push(c);
            }
            '<' if !in_literal => {
                if i + 1 < chars.len() && chars[i + 1] == '<' {
                    quoted_depth += 1;
                    current.push(c);
                    current.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                in_iri = true;
                current.push(c);
            }
            '>' if !in_literal && quoted_depth > 0 => {
                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    quoted_depth -= 1;
                    current.push(c);
                    current.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                current.push(c);
            }
            '>' if !in_literal && in_iri => {
                in_iri = false;
                current.push(c);
            }
            ' ' | '\t' | '\n' if !in_literal && !in_iri && quoted_depth == 0 => {
                if !current.is_empty() {
                    tokens.push(current.trim().to_owned());
                    current.clear();
                }
            }
            _ => current.push(c),
        }
        i += 1;
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_owned());
    }
    tokens
}


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

    // Note: the predicate_stats view is created via extension_sql in lib.rs,
    // not here, to avoid deadlocks when initialize_schema() is called from
    // concurrent test transactions.
}

/// Get the current VP promotion threshold from the GUC.
fn vp_promotion_threshold() -> i64 {
    crate::VPP_THRESHOLD.get() as i64
}

/// Ensure a dedicated VP table (HTAP split) exists for `predicate_id`.
///
/// Returns the fully-qualified view name `_pg_ripple.vp_{id}`.
/// In v0.6.0+, this creates delta + main + tombstones + view.
fn ensure_vp_table(predicate_id: i64) -> String {
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

    // Install CDC trigger on the new delta table.
    crate::cdc::install_trigger(predicate_id);

    view
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
/// Returns the SID, or 0 if the quad already exists (duplicate no-op).
fn insert_into_vp_rare(p_id: i64, s_id: i64, o_id: i64, g: i64) -> i64 {
    // Use ON CONFLICT DO NOTHING for set semantics — vp_rare has a UNIQUE(p,s,o,g)
    // constraint (added in v0.44.0) so duplicate quads are silently skipped.
    // pgrx's get_one_with_args returns Err (not Ok(None)) when ON CONFLICT DO NOTHING
    // fires and no row is returned, so we match explicitly on Ok/Err.
    let sid = match Spi::get_one_with_args::<i64>(
        "INSERT INTO _pg_ripple.vp_rare (p, s, o, g) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (p, s, o, g) DO NOTHING RETURNING i",
        &[
            DatumWithOid::from(p_id),
            DatumWithOid::from(s_id),
            DatumWithOid::from(o_id),
            DatumWithOid::from(g),
        ],
    ) {
        Ok(Some(val)) => val,
        Ok(None) | Err(_) => 0, // ON CONFLICT fired — row already exists, skip
    };

    if sid == 0 {
        // Duplicate triple — UNIQUE constraint fired, no row inserted.
        return 0;
    }

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

/// Promote a single predicate from vp_rare to its own VP table (HTAP split).
///
/// v0.22.0 H-3/H-4: Uses a single atomic CTE to eliminate the two-statement window
/// where concurrent inserts could orphan rows in vp_rare under a predicate that now
/// has its own VP table. After the atomic move, updates triple_count to match the
/// actual row count rather than leaving it at 0 after promotion.
fn promote_predicate(p_id: i64) {
    // v0.37.0: Acquire a per-predicate advisory lock before promotion to ensure
    // exactly one backend races to promote the same predicate. CREATE TABLE IF NOT
    // EXISTS is idempotent, but the data move must not be executed twice.
    Spi::run_with_args(
        "SELECT pg_advisory_xact_lock($1)",
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("promote_predicate: advisory lock error: {e}"));

    // ensure_vp_table creates the HTAP split (delta + main + tombstones + view).
    ensure_vp_table(p_id);
    let delta = format!("_pg_ripple.vp_{p_id}_delta");

    // Atomically move all rows for this predicate from vp_rare to the dedicated
    // delta table in a single CTE — eliminates the window between SELECT and DELETE
    // where concurrent inserts could be orphaned.
    Spi::run_with_args(
        &format!(
            "WITH moved AS ( \
               DELETE FROM _pg_ripple.vp_rare WHERE p = $1 \
               RETURNING s, o, g, i, source \
             ) \
             INSERT INTO {delta} (s, o, g, i, source) \
             SELECT s, o, g, i, source FROM moved \
             ON CONFLICT (s, o, g) DO NOTHING"
        ),
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate promotion atomic CTE SPI error: {e}"));

    // Restore accurate triple_count in the predicate catalog after promotion.
    // Before this update, triple_count reflects vp_rare inserts; after the atomic
    // move the VP table is the authoritative source.
    Spi::run_with_args(
        &format!(
            "UPDATE _pg_ripple.predicates \
             SET triple_count = (SELECT count(*) FROM {delta}), \
                 table_oid   = (SELECT oid FROM pg_class \
                                WHERE relname = 'vp_{p_id}_delta' \
                                  AND relnamespace = (SELECT oid FROM pg_namespace \
                                                      WHERE nspname = '_pg_ripple')), \
                 schema_name  = '_pg_ripple', \
                 table_name   = 'vp_{p_id}_delta' \
             WHERE id = $1"
        ),
        &[DatumWithOid::from(p_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("predicate promotion count update SPI error: {e}"));
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
        // v0.13.0: Create extended statistics on (s, o) for correlation-aware planning.
        create_extended_statistics(p_id);
    }

    count
}

/// Create PG18 extended statistics on the `(s, o)` column pair of a VP table.
///
/// Extended statistics help the PostgreSQL planner understand the correlation
/// between subject and object columns, enabling better cardinality estimates
/// for multi-predicate star patterns.
///
/// The statistic object is named `_pg_ripple.ext_stats_vp_{id}` and covers
/// n-distinct and dependencies statistics on `(s, o)`.
fn create_extended_statistics(pred_id: i64) {
    let stats_name = format!("ext_stats_vp_{pred_id}");
    let delta_table = format!("_pg_ripple.vp_{pred_id}_delta");

    // Create extended statistics if not already present.
    let create_sql = format!(
        "CREATE STATISTICS IF NOT EXISTS _pg_ripple.{stats_name} \
         (ndistinct, dependencies) ON s, o FROM {delta_table}"
    );
    Spi::run_with_args(&create_sql, &[])
        .unwrap_or_else(|e| pgrx::warning!("extended stats creation for vp_{pred_id}: {e}"));
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Allocate and return the next load generation ID (for blank node scoping).
pub fn next_load_generation() -> i64 {
    let new_gen = Spi::get_one::<i64>("SELECT nextval('_pg_ripple.load_generation_seq')")
        .unwrap_or_else(|e| pgrx::error!("load_generation_seq SPI error: {e}"))
        .unwrap_or(1);
    // Update the session cache so current_load_generation() reflects the new value.
    LOAD_GEN_CACHE.store(new_gen, std::sync::atomic::Ordering::Relaxed);
    new_gen
}

/// Insert a triple `(s, p, o)` into graph `g`.
///
/// Routes to vp_rare for new/rare predicates; promotes when threshold is crossed.
/// Returns the globally-unique statement identifier (SID).
pub fn insert_triple(s: &str, p: &str, o: &str, g: i64) -> i64 {
    let s_id = encode_rdf_term(s);
    let p_id = dictionary::encode(strip_angle_brackets(p), dictionary::KIND_IRI);
    let o_id = encode_rdf_term(o);

    // Fast path: dedicated VP table (HTAP split) already exists — insert to delta.
    if let Some(_view) = get_dedicated_vp_table(p_id) {
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        // Use ON CONFLICT DO UPDATE to get the existing row's ID if it already exists.
        // This handles UNIQUE (s, o, g) constraint (v0.22.0 H-6).
        // If the triple already exists in delta, we return its existing statement ID.
        // This prevents duplicate triples across main+delta merge boundaries.
        let sid = Spi::get_one_with_args::<i64>(
            &format!(
                "INSERT INTO {delta} (s, o, g) VALUES ($1, $2, $3) \
                 ON CONFLICT (s, o, g) DO UPDATE SET i = EXCLUDED.i \
                 RETURNING i"
            ),
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

        // Update shmem delta counter for merge worker triggering.
        crate::shmem::record_delta_inserts(1);
        // Mark predicate as having delta rows in the bloom filter.
        crate::shmem::set_predicate_delta_bit(p_id);

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
    if let Some(_view) = get_dedicated_vp_table(p_id) {
        // Route insert to delta table (HTAP write inbox).
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        // Use ON CONFLICT DO UPDATE for UNIQUE (s, o, g) constraint (v0.22.0 H-6).
        let sid = Spi::get_one_with_args::<i64>(
            &format!(
                "INSERT INTO {delta} (s, o, g) VALUES ($1, $2, $3) \
                 ON CONFLICT (s, o, g) DO UPDATE SET i = EXCLUDED.i \
                 RETURNING i"
            ),
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

        crate::shmem::record_delta_inserts(1);
        // Mark predicate as having delta rows in the bloom filter.
        crate::shmem::set_predicate_delta_bit(p_id);
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

    if let Some(_view) = table_opt {
        // Route batch insert to delta table.
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        // Build a multi-row VALUES insert (all i64 integers — injection-safe).
        let values: Vec<String> = rows
            .iter()
            .map(|(s, o, g)| format!("({},{},{})", s, o, g))
            .collect();
        let sql = format!(
            "INSERT INTO {delta} (s, o, g) VALUES {} ON CONFLICT (s, o, g) DO NOTHING",
            values.join(","),
        );
        Spi::run_with_args(&sql, &[])
            .unwrap_or_else(|e| pgrx::error!("batch VP delta insert SPI error: {e}"));

        let cnt = rows.len() as i64;
        Spi::run_with_args(
            "UPDATE _pg_ripple.predicates SET triple_count = triple_count + $2 WHERE id = $1",
            &[DatumWithOid::from(p_id), DatumWithOid::from(cnt)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count batch update SPI error: {e}"));

        crate::shmem::record_delta_inserts(cnt);
        // Mark predicate as having delta rows in the bloom filter.
        crate::shmem::set_predicate_delta_bit(p_id);
    } else {
        // Insert into vp_rare in bulk.
        // Deduplicate within this batch first (set semantics within a single load).
        let mut seen = std::collections::HashSet::new();
        let unique_rows: Vec<(i64, i64, i64)> = rows
            .iter()
            .filter(|&&(s, o, g)| seen.insert((s, o, g)))
            .copied()
            .collect();
        if unique_rows.is_empty() {
            return 0;
        }
        // Insert only rows not already present — use a NOT EXISTS guard for
        // cross-statement deduplication (UNIQUE constraint enforces the rest).
        let values: Vec<String> = unique_rows
            .iter()
            .map(|(s, o, g)| format!("({},{},{},{})", p_id, s, o, g))
            .collect();
        let sql = format!(
            "INSERT INTO _pg_ripple.vp_rare (p, s, o, g) \
             SELECT p, s, o, g FROM (VALUES {}) AS v(p, s, o, g) \
             WHERE NOT EXISTS (SELECT 1 FROM _pg_ripple.vp_rare r WHERE r.p=v.p AND r.s=v.s AND r.o=v.o AND r.g=v.g)",
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

    // Try dedicated VP table (HTAP split).
    if let Some(_view) = get_dedicated_vp_table(p_id) {
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        let tombs = format!("_pg_ripple.vp_{p_id}_tombstones");

        // 1. Try to delete from delta first (fast path).
        let d = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH d AS (DELETE FROM {delta} WHERE s=$1 AND o=$2 AND g=$3 RETURNING 1) \
                 SELECT count(*)::bigint FROM d"
            ),
            &[
                DatumWithOid::from(s_id),
                DatumWithOid::from(o_id),
                DatumWithOid::from(g),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("triple delete delta SPI error: {e}"))
        .unwrap_or(0);

        if d > 0 {
            deleted += d;
        } else {
            // 2. Not in delta — add a tombstone to suppress it from main.
            // v0.37.0: Acquire the per-predicate advisory lock in shared mode before
            // inserting a tombstone. The merge worker acquires the exclusive form
            // (pg_advisory_xact_lock) so a merge and a concurrent delete never race.
            Spi::run_with_args(
                "SELECT pg_advisory_xact_lock_shared($1)",
                &[DatumWithOid::from(p_id)],
            )
            .unwrap_or_else(|e| pgrx::error!("delete_triple: advisory lock error: {e}"));

            Spi::run_with_args(
                &format!(
                    "INSERT INTO {tombs} (s, o, g) VALUES ($1, $2, $3) \
                     ON CONFLICT DO NOTHING"
                ),
                &[
                    DatumWithOid::from(s_id),
                    DatumWithOid::from(o_id),
                    DatumWithOid::from(g),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("tombstone insert SPI error: {e}"));

            // Check if the triple actually existed in main.
            let in_main = Spi::get_one_with_args::<i64>(
                &format!(
                    "SELECT count(*)::bigint FROM _pg_ripple.vp_{p_id}_main \
                     WHERE s = $1 AND o = $2 AND g = $3"
                ),
                &[
                    DatumWithOid::from(s_id),
                    DatumWithOid::from(o_id),
                    DatumWithOid::from(g),
                ],
            )
            .unwrap_or(None)
            .unwrap_or(0);
            deleted += in_main;
        }

        if deleted > 0 {
            Spi::run_with_args(
                "UPDATE _pg_ripple.predicates \
                 SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
                &[DatumWithOid::from(p_id), DatumWithOid::from(deleted)],
            )
            .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
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

/// Return the number of triples in a specific named graph.
pub fn triple_count_in_graph(g_id: i64) -> i64 {
    let mut total = 0i64;

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
        let cnt = Spi::get_one_with_args::<i64>(
            &format!("SELECT count(*)::bigint FROM {table} WHERE g = $1"),
            &[DatumWithOid::from(g_id)],
        )
        .unwrap_or(None)
        .unwrap_or(0);
        total += cnt;
    }

    let rare_cnt = Spi::get_one_with_args::<i64>(
        "SELECT count(*)::bigint FROM _pg_ripple.vp_rare WHERE g = $1",
        &[DatumWithOid::from(g_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0);
    total += rare_cnt;

    total
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
#[allow(dead_code)]
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

/// Iterate over all encoded triples in batches using cursor-based streaming.
///
/// Calls `callback` for each batch of `(s_id, p_id, o_id, g_id)` tuples.
/// The batch size is controlled by `pg_ripple.export_batch_size` (default 10 000).
///
/// This avoids loading the entire graph into a single Rust `Vec`, which can
/// consume many GiB of memory for large stores.
///
/// # Parameters
/// - `graph`: optional graph filter (None = all graphs)
/// - `callback`: called once per batch with a slice of `(s, p, o, g)` tuples
#[allow(clippy::type_complexity)]
pub fn for_each_encoded_triple_batch(
    graph: Option<i64>,
    callback: &mut dyn FnMut(&[(i64, i64, i64, i64)]), // (s, p, o, g)
) {
    let batch_size = crate::EXPORT_BATCH_SIZE.get() as usize;

    // ── Dedicated VP tables ───────────────────────────────────────────────────
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
            Some(gid) => format!(" WHERE g = {gid}"),
            None => String::new(),
        };
        // Use OFFSET-based pagination inside a single SPI::connect to avoid
        // repeated connection overhead.  Each page fetches `batch_size` rows
        // ordered by the monotonically-increasing SID column `i`.
        let mut offset = 0usize;
        loop {
            let sql = format!(
                "SELECT s, o, g FROM {table}{g_filter} ORDER BY i LIMIT {batch_size} OFFSET {offset}"
            );
            let page: Vec<(i64, i64, i64, i64)> = Spi::connect(|c| {
                c.select(&sql, None, &[])
                    .unwrap_or_else(|e| {
                        pgrx::error!("for_each_encoded_triple_batch VP scan SPI error: {e}")
                    })
                    .filter_map(|row| {
                        let s: Option<i64> = row.get(1).ok().flatten();
                        let o: Option<i64> = row.get(2).ok().flatten();
                        let g: Option<i64> = row.get(3).ok().flatten();
                        match (s, o, g) {
                            (Some(s), Some(o), Some(g)) => Some((s, p_id, o, g)),
                            _ => None,
                        }
                    })
                    .collect()
            });
            let page_len = page.len();
            if !page.is_empty() {
                callback(&page);
            }
            if page_len < batch_size {
                break;
            }
            offset += batch_size;
        }
    }

    // ── vp_rare ───────────────────────────────────────────────────────────────
    let g_filter = match graph {
        Some(gid) => format!(" WHERE g = {gid}"),
        None => String::new(),
    };
    let mut offset = 0usize;
    loop {
        let sql = format!(
            "SELECT p, s, o, g FROM _pg_ripple.vp_rare{g_filter} ORDER BY i LIMIT {batch_size} OFFSET {offset}"
        );
        let page: Vec<(i64, i64, i64, i64)> = Spi::connect(|c| {
            c.select(&sql, None, &[])
                .unwrap_or_else(|e| {
                    pgrx::error!("for_each_encoded_triple_batch vp_rare scan SPI error: {e}")
                })
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
        let page_len = page.len();
        if !page.is_empty() {
            callback(&page);
        }
        if page_len < batch_size {
            break;
        }
        offset += batch_size;
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

/// Clear all triples in a named or default graph (identified by `g_id`).
/// Like `drop_graph` but operates by numeric graph ID.  Returns triples deleted.
pub fn clear_graph_by_id(g_id: i64) -> i64 {
    let mut deleted = 0i64;

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
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        let tombs = format!("_pg_ripple.vp_{p_id}_tombstones");
        let main_t = format!("_pg_ripple.vp_{p_id}_main");

        let d_delta = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH d AS (DELETE FROM {delta} WHERE g = $1 RETURNING 1) \
                 SELECT count(*)::bigint FROM d"
            ),
            &[DatumWithOid::from(g_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("clear_graph_by_id delta delete SPI error: {e}"))
        .unwrap_or(0);

        let d_main = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH ins AS ( \
                     INSERT INTO {tombs} (s, o, g) \
                     SELECT s, o, g FROM {main_t} WHERE g = $1 \
                     ON CONFLICT DO NOTHING \
                     RETURNING 1 \
                 ) SELECT count(*)::bigint FROM ins"
            ),
            &[DatumWithOid::from(g_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("clear_graph_by_id tombstones SPI error: {e}"))
        .unwrap_or(0);

        let d = d_delta + d_main;
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

    let d = Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.vp_rare WHERE g = $1 RETURNING p) \
         SELECT count(*)::bigint FROM d",
        &[DatumWithOid::from(g_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("clear_graph_by_id vp_rare delete SPI error: {e}"))
    .unwrap_or(0);
    deleted += d;

    deleted
}

/// Collect all distinct graph IDs currently in the store (including default graph 0).
pub fn all_graph_ids() -> Vec<i64> {
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

    for p_id in &pred_ids {
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        let main_t = format!("_pg_ripple.vp_{p_id}_main");
        Spi::connect(|c| {
            for row in c
                .select(&format!("SELECT DISTINCT g FROM {delta}"), None, &[])
                .unwrap_or_else(|e| pgrx::error!("all_graph_ids delta scan: {e}"))
            {
                if let Some(g) = row.get::<i64>(1).ok().flatten() {
                    g_ids.insert(g);
                }
            }
            for row in c
                .select(&format!("SELECT DISTINCT g FROM {main_t}"), None, &[])
                .unwrap_or_else(|e| pgrx::error!("all_graph_ids main scan: {e}"))
            {
                if let Some(g) = row.get::<i64>(1).ok().flatten() {
                    g_ids.insert(g);
                }
            }
        });
    }

    Spi::connect(|c| {
        for row in c
            .select("SELECT DISTINCT g FROM _pg_ripple.vp_rare", None, &[])
            .unwrap_or_else(|e| pgrx::error!("all_graph_ids vp_rare scan: {e}"))
        {
            if let Some(g) = row.get::<i64>(1).ok().flatten() {
                g_ids.insert(g);
            }
        }
    });

    g_ids.into_iter().collect()
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
        // For HTAP split: delete from delta + add tombstones for main rows.
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        let tombs = format!("_pg_ripple.vp_{p_id}_tombstones");
        let main_t = format!("_pg_ripple.vp_{p_id}_main");

        // Delete from delta.
        let d_delta = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH d AS (DELETE FROM {delta} WHERE g = $1 RETURNING 1) \
                 SELECT count(*)::bigint FROM d"
            ),
            &[DatumWithOid::from(g_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("drop_graph delta delete SPI error: {e}"))
        .unwrap_or(0);

        // Add tombstones for main rows (to suppress them from the view).
        let d_main = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH ins AS ( \
                     INSERT INTO {tombs} (s, o, g) \
                     SELECT s, o, g FROM {main_t} WHERE g = $1 \
                     ON CONFLICT DO NOTHING \
                     RETURNING 1 \
                 ) SELECT count(*)::bigint FROM ins"
            ),
            &[DatumWithOid::from(g_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("drop_graph tombstones SPI error: {e}"))
        .unwrap_or(0);

        let d = d_delta + d_main;
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
            rows.filter_map(|row| row.get::<i64>(1).ok().flatten())
                .next()
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

// ─── v0.5.1 additions ─────────────────────────────────────────────────────────

/// Insert a triple by pre-encoded dictionary IDs.
/// Alias for `insert_encoded_triple` for use from the SPARQL Update executor.
pub fn insert_triple_by_ids(s_id: i64, p_id: i64, o_id: i64, g_id: i64) -> i64 {
    insert_encoded_triple(s_id, p_id, o_id, g_id)
}

/// Delete a triple by pre-encoded dictionary IDs.  Returns the number of deleted rows.
pub fn delete_triple_by_ids(s_id: i64, p_id: i64, o_id: i64, g_id: i64) -> i64 {
    let mut deleted = 0i64;

    // Try dedicated VP table (HTAP: delta first, then tombstone).
    if let Some(_view) = get_dedicated_vp_table(p_id) {
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        let tombs = format!("_pg_ripple.vp_{p_id}_tombstones");

        let d = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH d AS (DELETE FROM {delta} WHERE s=$1 AND o=$2 AND g=$3 RETURNING 1) \
                 SELECT count(*)::bigint FROM d"
            ),
            &[
                DatumWithOid::from(s_id),
                DatumWithOid::from(o_id),
                DatumWithOid::from(g_id),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("delete_triple_by_ids delta SPI error: {e}"))
        .unwrap_or(0);

        if d > 0 {
            deleted += d;
        } else {
            // Add tombstone to suppress from main.
            Spi::run_with_args(
                &format!(
                    "INSERT INTO {tombs} (s, o, g) VALUES ($1, $2, $3) \
                     ON CONFLICT DO NOTHING"
                ),
                &[
                    DatumWithOid::from(s_id),
                    DatumWithOid::from(o_id),
                    DatumWithOid::from(g_id),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("tombstone insert SPI error: {e}"));

            let in_main = Spi::get_one_with_args::<i64>(
                &format!(
                    "SELECT count(*)::bigint FROM _pg_ripple.vp_{p_id}_main \
                     WHERE s = $1 AND o = $2 AND g = $3"
                ),
                &[
                    DatumWithOid::from(s_id),
                    DatumWithOid::from(o_id),
                    DatumWithOid::from(g_id),
                ],
            )
            .unwrap_or(None)
            .unwrap_or(0);
            deleted += in_main;
        }

        if deleted > 0 {
            Spi::run_with_args(
                "UPDATE _pg_ripple.predicates \
                 SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
                &[DatumWithOid::from(p_id), DatumWithOid::from(deleted)],
            )
            .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
        }
    }

    // Also try vp_rare.
    let d = Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.vp_rare \
         WHERE p=$1 AND s=$2 AND o=$3 AND g=$4 RETURNING 1) \
         SELECT count(*)::bigint FROM d",
        &[
            DatumWithOid::from(p_id),
            DatumWithOid::from(s_id),
            DatumWithOid::from(o_id),
            DatumWithOid::from(g_id),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("vp_rare delete_by_ids SPI error: {e}"))
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

/// Return the current load generation counter (used for blank-node scoping).
/// Session-local cache of the current load generation value.
/// Updated by both `next_load_generation()` and on first access by `current_load_generation()`.
static LOAD_GEN_CACHE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// Wraps `next_load_generation` but does NOT advance the generation — it just
/// reads the current in-session value.
pub fn current_load_generation() -> i64 {
    let g = LOAD_GEN_CACHE.load(std::sync::atomic::Ordering::Relaxed);
    if g == 0 {
        // Fetch from DB on first call.
        let g2 = Spi::get_one::<i64>("SELECT last_value FROM _pg_ripple.load_generation_seq")
            .ok()
            .flatten()
            .unwrap_or(1);
        LOAD_GEN_CACHE.store(g2, std::sync::atomic::Ordering::Relaxed);
        g2
    } else {
        g
    }
}

/// Return all `(predicate_id, object_id)` pairs where the given `subject_id`
/// appears as the subject.  Used by the CBD DESCRIBE algorithm.
pub fn triples_for_subject(subject_id: i64) -> Vec<(i64, i64)> {
    let mut result = Vec::new();

    // Scan all dedicated VP tables.
    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("describe predicates SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        let pairs: Vec<(i64, i64)> = Spi::connect(|c| {
            c.select(
                &format!("SELECT $1, o FROM {table} WHERE s = $2"),
                None,
                &[DatumWithOid::from(p_id), DatumWithOid::from(subject_id)],
            )
            .unwrap_or_else(|e| pgrx::error!("describe vp SPI error: {e}"))
            .filter_map(|row| {
                Some((
                    row.get::<i64>(1).ok().flatten()?,
                    row.get::<i64>(2).ok().flatten()?,
                ))
            })
            .collect()
        });
        result.extend(pairs);
    }

    // Also scan vp_rare.
    let rare_pairs: Vec<(i64, i64)> = Spi::connect(|c| {
        c.select(
            "SELECT p, o FROM _pg_ripple.vp_rare WHERE s = $1",
            None,
            &[DatumWithOid::from(subject_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("describe vp_rare SPI error: {e}"))
        .filter_map(|row| {
            Some((
                row.get::<i64>(1).ok().flatten()?,
                row.get::<i64>(2).ok().flatten()?,
            ))
        })
        .collect()
    });
    result.extend(rare_pairs);

    result
}

/// Return all `(subject_id, predicate_id)` pairs where the given `object_id`
/// appears as the object.  Used by the symmetric CBD DESCRIBE algorithm.
pub fn triples_for_object(object_id: i64) -> Vec<(i64, i64)> {
    let mut result = Vec::new();

    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("describe_incoming predicates SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        let pairs: Vec<(i64, i64)> = Spi::connect(|c| {
            c.select(
                &format!("SELECT s, $1 FROM {table} WHERE o = $2"),
                None,
                &[DatumWithOid::from(p_id), DatumWithOid::from(object_id)],
            )
            .unwrap_or_else(|e| pgrx::error!("describe_incoming vp SPI error: {e}"))
            .filter_map(|row| {
                Some((
                    row.get::<i64>(1).ok().flatten()?,
                    row.get::<i64>(2).ok().flatten()?,
                ))
            })
            .collect()
        });
        result.extend(pairs);
    }

    let rare_pairs: Vec<(i64, i64)> = Spi::connect(|c| {
        c.select(
            "SELECT s, p FROM _pg_ripple.vp_rare WHERE o = $1",
            None,
            &[DatumWithOid::from(object_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("describe_incoming vp_rare SPI error: {e}"))
        .filter_map(|row| {
            Some((
                row.get::<i64>(1).ok().flatten()?,
                row.get::<i64>(2).ok().flatten()?,
            ))
        })
        .collect()
    });
    result.extend(rare_pairs);

    result
}

// ─── Deduplication functions (v0.7.0) ─────────────────────────────────────────

/// Remove duplicate `(s, o, g)` rows for the predicate identified by `p_iri`.
///
/// Strategy:
/// - **delta table**: DELETE all rows where ctid is not the minimum ctid per (s,o,g).
/// - **main table**: insert tombstone rows for all but the minimum-SID row per (s,o,g),
///   so duplicates are masked at query time and removed on the next merge.
/// - **vp_rare** (if predicate has no dedicated table): DELETE duplicate rows by
///   (p, s, o, g) keeping the minimum ctid.
///
/// Runs ANALYZE on all modified tables afterward.
/// Returns the total count of rows removed.
pub fn deduplicate_predicate(p_iri: &str) -> i64 {
    let p_clean = if p_iri.starts_with('<') && p_iri.ends_with('>') {
        &p_iri[1..p_iri.len() - 1]
    } else {
        p_iri
    };

    let p_id = match crate::dictionary::lookup_iri(p_clean) {
        Some(id) => id,
        None => {
            // Predicate not in dictionary — nothing to deduplicate.
            return 0;
        }
    };

    let mut total_removed: i64 = 0;

    if get_dedicated_vp_table(p_id).is_some() {
        // Dedicated HTAP VP table: handle delta and main separately.
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        let main = format!("_pg_ripple.vp_{p_id}_main");
        let tombs = format!("_pg_ripple.vp_{p_id}_tombstones");

        // Deduplicate delta: delete all rows keeping the minimum-i (SID) row per (s,o,g).
        // In practice the UNIQUE (s,o,g) constraint prevents duplicates in the delta table,
        // but this covers legacy data created before the constraint existed.
        let delta_removed = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH keep AS ( \
                     SELECT s, o, g, MIN(i) AS min_i \
                     FROM {delta} \
                     GROUP BY s, o, g \
                     HAVING COUNT(*) > 1 \
                 ), \
                 del AS ( \
                     DELETE FROM {delta} d \
                     USING keep k \
                     WHERE d.s = k.s AND d.o = k.o AND d.g = k.g AND d.i <> k.min_i \
                     RETURNING 1 \
                 ) \
                 SELECT COUNT(*)::BIGINT FROM del"
            ),
            &[],
        )
        .unwrap_or(None)
        .unwrap_or(0);

        total_removed += delta_removed;

        // Deduplicate main: tombstone all but the minimum-SID row per (s,o,g).
        // The rows remain in main but are hidden by the view until the next merge.
        let main_removed = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH ranked AS ( \
                     SELECT s, o, g, i, \
                            ROW_NUMBER() OVER (PARTITION BY s, o, g ORDER BY i ASC) AS rn \
                     FROM {main} \
                 ), \
                 dupes AS (SELECT DISTINCT s, o, g FROM ranked WHERE rn > 1), \
                 ins AS ( \
                     INSERT INTO {tombs} (s, o, g) \
                     SELECT s, o, g FROM dupes \
                     ON CONFLICT DO NOTHING \
                     RETURNING 1 \
                 ) \
                 SELECT COUNT(*)::BIGINT FROM ins"
            ),
            &[],
        )
        .unwrap_or(None)
        .unwrap_or(0);

        total_removed += main_removed;

        // ANALYZE both tables.
        Spi::run_with_args(&format!("ANALYZE {delta}"), &[])
            .unwrap_or_else(|e| pgrx::error!("ANALYZE delta error: {e}"));
        Spi::run_with_args(&format!("ANALYZE {main}"), &[])
            .unwrap_or_else(|e| pgrx::error!("ANALYZE main error: {e}"));
    } else {
        // vp_rare: DELETE duplicate (p, s, o, g) keeping the minimum-SID row.
        let rare_removed = Spi::get_one_with_args::<i64>(
            "WITH del AS ( \
                 DELETE FROM _pg_ripple.vp_rare r \
                 WHERE r.p = $1 \
                   AND r.i NOT IN ( \
                       SELECT MIN(i) FROM _pg_ripple.vp_rare \
                       WHERE p = $1 \
                       GROUP BY p, s, o, g \
                   ) \
                 RETURNING 1 \
             ) \
             SELECT COUNT(*)::BIGINT FROM del",
            &[DatumWithOid::from(p_id)],
        )
        .unwrap_or(None)
        .unwrap_or(0);

        total_removed += rare_removed;

        if rare_removed > 0 {
            Spi::run_with_args("ANALYZE _pg_ripple.vp_rare", &[])
                .unwrap_or_else(|e| pgrx::error!("ANALYZE vp_rare error: {e}"));
        }
    }

    total_removed
}

/// Remove duplicate `(s, o, g)` rows across all predicates and `vp_rare`.
///
/// Iterates over all predicate IRIs in `_pg_ripple.predicates` and calls
/// `deduplicate_predicate` for each. Then deduplicates `vp_rare` for any
/// predicates that remain in the rare table.
///
/// Returns the total count of rows removed.
pub fn deduplicate_all() -> i64 {
    // Collect all predicate IRIs from the catalog.
    let pred_iris: Vec<String> = Spi::connect(|c| {
        c.select(
            "SELECT d.value FROM _pg_ripple.predicates p \
             JOIN _pg_ripple.dictionary d ON d.id = p.id",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("deduplicate_all SPI error: {e}"))
        .filter_map(|row| row.get::<&str>(1).ok().flatten().map(|s| s.to_owned()))
        .collect()
    });

    let mut total: i64 = 0;
    for iri in pred_iris {
        total += deduplicate_predicate(&iri);
    }

    // Deduplicate all remaining rare triples in vp_rare
    // (predicates below promotion threshold that may not be in the catalog).
    let rare_removed = Spi::get_one_with_args::<i64>(
        "WITH del AS ( \
             DELETE FROM _pg_ripple.vp_rare r \
             WHERE r.i NOT IN ( \
                 SELECT MIN(i) FROM _pg_ripple.vp_rare \
                 GROUP BY p, s, o, g \
             ) \
             RETURNING 1 \
         ) \
         SELECT COUNT(*)::BIGINT FROM del",
        &[],
    )
    .unwrap_or(None)
    .unwrap_or(0);

    total += rare_removed;

    if rare_removed > 0 {
        Spi::run_with_args("ANALYZE _pg_ripple.vp_rare", &[])
            .unwrap_or_else(|e| pgrx::error!("ANALYZE vp_rare error: {e}"));
    }

    total
}
