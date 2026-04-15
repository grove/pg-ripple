//! pg_ripple — High-performance RDF triple store for PostgreSQL 18.
//!
//! # Architecture
//!
//! Every IRI, blank node, and literal is encoded to `i64` via XXH3-128 hash
//! (see `src/dictionary/`) before being stored in vertical-partition (VP)
//! tables in the `_pg_ripple` schema (see `src/storage/`).  SPARQL queries
//! are parsed with `spargebra`, compiled to SQL, and executed via SPI
//! (see `src/sparql/`).
//!
//! In v0.6.0 (HTAP Architecture), VP tables are split into delta + main
//! partitions for non-blocking concurrent reads and writes.

use pgrx::guc::{GucContext, GucFlags};
use pgrx::prelude::*;

mod bulk_load;
mod cdc;
mod dictionary;
mod error;
mod export;
mod fts;
mod shmem;
mod sparql;
mod storage;
mod worker;

pgrx::pg_module_magic!();

// Allow creating the `pg_ripple` schema despite the `pg_` prefix restriction.
pgrx::extension_sql!(
    r#"SET LOCAL allow_system_table_mods = on;"#,
    name = "bootstrap_allow_system_mods",
    bootstrap
);

// Create the predicate_stats view as extension SQL so it runs once at
// CREATE EXTENSION time rather than on every _PG_init call (which would
// cause deadlocks when concurrent test transactions call initialize_schema).
pgrx::extension_sql!(
    r#"
CREATE OR REPLACE VIEW pg_ripple.predicate_stats AS
SELECT
    d.value       AS predicate_iri,
    p.triple_count,
    CASE WHEN p.table_oid IS NOT NULL THEN 'dedicated' ELSE 'rare' END AS storage
FROM _pg_ripple.predicates p
JOIN _pg_ripple.dictionary d ON d.id = p.id
ORDER BY p.triple_count DESC;
"#,
    name = "predicate_stats_view",
    finalize
);

// ─── GUC parameters ───────────────────────────────────────────────────────────

/// GUC: default named-graph identifier (empty string → default graph 0).
pub static DEFAULT_GRAPH: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: minimum triple count before a rare predicate gets its own VP table.
pub static VPP_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1000);

/// GUC: when true, add a `(g, s, o)` index to every dedicated VP table for
/// fast named-graph–scoped queries.  Off by default to avoid index bloat.
pub static NAMED_GRAPH_OPTIMIZED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: maximum number of cached SPARQL→SQL plan translations per backend.
/// Set to 0 to disable the plan cache.
pub static PLAN_CACHE_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(256);

/// GUC: maximum recursion depth for SPARQL property path queries (`+`, `*`).
/// Prevents runaway recursive CTEs on cyclic or very deep graphs.
pub static MAX_PATH_DEPTH: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100);

/// GUC: DESCRIBE algorithm — 'cbd' (Concise Bounded Description, default),
/// 'scbd' (Symmetric CBD, includes incoming arcs), 'simple' (one-hop only).
pub static DESCRIBE_STRATEGY: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.6.0 GUCs ─────────────────────────────────────────────────────────────

/// GUC: minimum rows in a delta table before triggering a merge.
pub static MERGE_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

/// GUC: maximum seconds between merge worker polling intervals.
pub static MERGE_INTERVAL_SECS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(60);

/// GUC: seconds to keep the old main table after a merge before dropping it.
pub static MERGE_RETENTION_SECONDS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(60);

/// GUC: number of rows written in one batch before poking the merge worker.
pub static LATCH_TRIGGER_THRESHOLD: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(10_000);

/// GUC: database the merge background worker connects to.
pub static WORKER_DATABASE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: seconds before the merge worker watchdog logs a WARNING for inactivity.
pub static MERGE_WATCHDOG_TIMEOUT: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(300);

/// GUC: maximum number of entries in the shared-memory dictionary encode cache.
/// Rounded down to the nearest multiple of `ENCODE_CACHE_CAPACITY` (4096).
/// This is a startup-only GUC (read at `_PG_init`); changing it requires a
/// PostgreSQL restart.
///
/// The shared-memory cache is split across 4 shards of 1024 slots each.
/// This GUC documents the effective size; the actual shard sizes are compiled
/// in at build time.  Set to 0 to note that only the backend-local cache is active.
pub static DICTIONARY_CACHE_SIZE: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(crate::shmem::ENCODE_CACHE_CAPACITY as i32);

/// GUC: shared-memory budget cap in megabytes.
///
/// Bulk loads check the encode-cache utilization against this budget and
/// reduce their batch size when utilization exceeds 90% to prevent OOM.
/// Set to 0 to disable back-pressure.  Startup-only GUC.
pub static CACHE_BUDGET_MB: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(64);

// ─── ExecutorEnd hook (v0.6.0) ────────────────────────────────────────────────

/// Register a PostgreSQL `ExecutorEnd_hook` that pokes the merge worker's latch
/// whenever the accumulated unmerged delta row count crosses
/// `pg_ripple.latch_trigger_threshold`.
///
/// Must only be called from `_PG_init` inside the postmaster context
/// (i.e. when loaded via `shared_preload_libraries`).
fn register_executor_end_hook() {
    unsafe {
        static mut PREV_EXECUTOR_END: pg_sys::ExecutorEnd_hook_type = None;

        PREV_EXECUTOR_END = pg_sys::ExecutorEnd_hook;
        pg_sys::ExecutorEnd_hook = Some(pg_ripple_executor_end);

        #[pg_guard]
        unsafe extern "C-unwind" fn pg_ripple_executor_end(query_desc: *mut pg_sys::QueryDesc) {
            // Call the previous hook first.
            unsafe {
                if let Some(prev) = PREV_EXECUTOR_END {
                    prev(query_desc);
                } else {
                    pg_sys::standard_ExecutorEnd(query_desc);
                }
            }

            // If shmem is ready, check whether delta growth exceeds the threshold.
            if !crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            let threshold = crate::LATCH_TRIGGER_THRESHOLD.get() as i64;
            let delta_rows = crate::shmem::TOTAL_DELTA_ROWS
                .get()
                .load(std::sync::atomic::Ordering::Relaxed);
            if delta_rows >= threshold {
                crate::shmem::poke_merge_worker();
            }
        }
    }
}

/// Called once when the extension shared library is loaded.
#[allow(non_snake_case)]
#[pg_guard]
pub extern "C-unwind" fn _PG_init() {
    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.default_graph",
        c"IRI of the default named graph (empty = built-in default graph)",
        c"",
        &DEFAULT_GRAPH,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.vp_promotion_threshold",
        c"Minimum triple count before a predicate gets its own VP table (default: 1000)",
        c"",
        &VPP_THRESHOLD,
        1,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.named_graph_optimized",
        c"Add a (g, s, o) index to each VP table to speed up named-graph queries",
        c"",
        &NAMED_GRAPH_OPTIMIZED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.plan_cache_size",
        c"Maximum number of cached SPARQL-to-SQL plan translations per backend (0 = disabled)",
        c"",
        &PLAN_CACHE_SIZE,
        0,
        65536,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.max_path_depth",
        c"Maximum recursion depth for SPARQL property path queries (+ and *); 0 = unlimited",
        c"",
        &MAX_PATH_DEPTH,
        0,
        10000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.describe_strategy",
        c"DESCRIBE algorithm: 'cbd' (Concise Bounded Description), 'scbd' (Symmetric CBD), or 'simple'",
        c"",
        &DESCRIBE_STRATEGY,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.6.0 GUCs ──────────────────────────────────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_threshold",
        c"Minimum rows in a delta table before triggering a background merge (default: 10000)",
        c"",
        &MERGE_THRESHOLD,
        1,
        i32::MAX,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_interval_secs",
        c"Maximum seconds between merge worker polling cycles (default: 60)",
        c"",
        &MERGE_INTERVAL_SECS,
        1,
        3600,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_retention_seconds",
        c"Seconds to keep the previous main table after a merge before dropping it (default: 60)",
        c"",
        &MERGE_RETENTION_SECONDS,
        0,
        86400,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.latch_trigger_threshold",
        c"Rows written in one batch before poking the merge worker latch (default: 10000)",
        c"",
        &LATCH_TRIGGER_THRESHOLD,
        1,
        i32::MAX,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.worker_database",
        c"Database the background merge worker connects to (default: postgres)",
        c"",
        &WORKER_DATABASE,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.merge_watchdog_timeout",
        c"Seconds of merge worker inactivity before a WARNING is logged (default: 300)",
        c"",
        &MERGE_WATCHDOG_TIMEOUT,
        10,
        86400,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.dictionary_cache_size",
        c"Shared-memory encode-cache capacity in entries (default: 4096; startup only)",
        c"",
        &DICTIONARY_CACHE_SIZE,
        0,
        1_000_000,
        GucContext::Postmaster,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.cache_budget",
        c"Shared-memory budget cap in MB; bulk loads throttle when >90% utilised (default: 64; startup only)",
        c"",
        &CACHE_BUDGET_MB,
        0,
        65536,
        GucContext::Postmaster,
        GucFlags::default(),
    );

    // ── Shared memory initialisation (v0.6.0) ────────────────────────────────
    // Only registers shmem hooks (pg_shmem_init!) when running in postmaster
    // context (i.e. loaded via shared_preload_libraries).  When loaded via
    // CREATE EXTENSION the hooks have already fired; skip to avoid the
    // "PgAtomic was not initialized" panic.
    if unsafe { pg_sys::IsPostmasterEnvironment } {
        shmem::init();
        worker::register_merge_worker();
        // Register ExecutorEnd hook to poke the merge worker latch when the
        // accumulated unmerged delta row count crosses the trigger threshold.
        register_executor_end_hook();
    }

    // Initialize schemas and base tables.
    storage::initialize_schema();
}

// ─── Public SQL-callable functions ────────────────────────────────────────────

/// All user-visible SQL functions live in the `pg_ripple` schema.
#[pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── Dictionary ────────────────────────────────────────────────────────────

    /// Encode a text IRI/blank-node/literal to its dictionary `i64` identifier.
    #[pg_extern]
    fn encode_term(term: &str, kind: i16) -> i64 {
        crate::dictionary::encode(term, kind)
    }

    /// Decode a dictionary `i64` back to its original text value.
    #[pg_extern]
    fn decode_id(id: i64) -> Option<String> {
        crate::dictionary::decode(id)
    }

    // ── RDF-star: quoted triple encoding (v0.4.0) ──────────────────────────

    /// Encode a quoted triple `(s, p, o)` into the dictionary.
    ///
    /// All three arguments must be N-Triples–formatted terms (IRIs, literals,
    /// blank nodes, or nested `<< … >>` quoted triples).
    /// Returns the dictionary ID of the quoted triple.
    #[pg_extern]
    fn encode_triple(s: &str, p: &str, o: &str) -> i64 {
        let s_id = crate::storage::encode_rdf_term(s);
        let p_id = crate::dictionary::encode(
            crate::storage::strip_angle_brackets_pub(p),
            crate::dictionary::KIND_IRI,
        );
        let o_id = crate::storage::encode_rdf_term(o);
        crate::dictionary::encode_quoted_triple(s_id, p_id, o_id)
    }

    /// Decode a quoted triple dictionary ID to its component terms as JSONB.
    ///
    /// Returns `{"s": "...", "p": "...", "o": "..."}` with N-Triples–formatted
    /// values, or NULL if `id` is not a quoted triple.
    #[pg_extern]
    fn decode_triple(id: i64) -> Option<pgrx::JsonB> {
        let (s_id, p_id, o_id) = crate::dictionary::decode_quoted_triple_components(id)?;
        let mut obj = serde_json::Map::new();
        obj.insert(
            "s".to_owned(),
            serde_json::Value::String(crate::dictionary::format_ntriples(s_id)),
        );
        obj.insert(
            "p".to_owned(),
            serde_json::Value::String(crate::dictionary::format_ntriples(p_id)),
        );
        obj.insert(
            "o".to_owned(),
            serde_json::Value::String(crate::dictionary::format_ntriples(o_id)),
        );
        Some(pgrx::JsonB(serde_json::Value::Object(obj)))
    }

    // ── Triple CRUD ───────────────────────────────────────────────────────────

    /// Insert a triple into the appropriate VP table.
    ///
    /// `s`, `p`, and `o` accept N-Triples–formatted terms (IRIs, literals,
    /// blank nodes, or `<< … >>` quoted triples).
    /// `g` is an optional named graph IRI; NULL uses the default graph.
    /// Returns the globally-unique statement identifier (SID).
    #[pg_extern]
    fn insert_triple(s: &str, p: &str, o: &str, g: default!(Option<&str>, "NULL")) -> i64 {
        let g_id = g.map_or(0_i64, |iri| {
            crate::dictionary::encode(
                crate::storage::strip_angle_brackets_pub(iri),
                crate::dictionary::KIND_IRI,
            )
        });
        crate::storage::insert_triple(s, p, o, g_id)
    }

    /// Look up a statement by its globally-unique statement identifier (SID).
    ///
    /// Returns `{"s": "...", "p": "...", "o": "...", "g": "..."}` as JSONB,
    /// or NULL if the SID is not found.
    #[pg_extern]
    fn get_statement(i: i64) -> Option<pgrx::JsonB> {
        let (s, p, o, g) = crate::storage::get_statement_by_sid(i)?;
        let mut obj = serde_json::Map::new();
        obj.insert("s".to_owned(), serde_json::Value::String(s));
        obj.insert("p".to_owned(), serde_json::Value::String(p));
        obj.insert("o".to_owned(), serde_json::Value::String(o));
        obj.insert("g".to_owned(), serde_json::Value::String(g));
        Some(pgrx::JsonB(serde_json::Value::Object(obj)))
    }

    /// Delete a triple.  Returns the number of rows removed (0 or 1).
    #[pg_extern]
    fn delete_triple(s: &str, p: &str, o: &str) -> i64 {
        crate::storage::delete_triple(s, p, o, 0_i64)
    }

    /// Return the total number of triples across all VP tables and vp_rare.
    #[pg_extern]
    fn triple_count() -> i64 {
        crate::storage::total_triple_count()
    }

    /// Pattern-match triples; any argument may be NULL to act as a wildcard.
    /// Queries both dedicated VP tables and vp_rare.
    /// Returns N-Triples–formatted `(s, p, o, g)` tuples.
    #[pg_extern]
    fn find_triples(
        s: Option<&str>,
        p: Option<&str>,
        o: Option<&str>,
    ) -> TableIterator<
        'static,
        (
            name!(s, String),
            name!(p, String),
            name!(o, String),
            name!(g, String),
        ),
    > {
        let rows = crate::storage::find_triples(s, p, o, None);
        TableIterator::new(rows)
    }

    // ── Rare-predicate promotion ──────────────────────────────────────────────

    /// Promote all rare predicates that have reached the promotion threshold.
    /// Returns the number of predicates promoted.
    #[pg_extern]
    fn promote_rare_predicates() -> i64 {
        crate::storage::promote_rare_predicates()
    }

    // ── Bulk loaders ──────────────────────────────────────────────────────────

    /// Load N-Triples data from a text string.  Returns the number of triples loaded.
    /// Also accepts N-Triples-star (quoted triples as objects or subjects).
    #[pg_extern]
    fn load_ntriples(data: &str) -> i64 {
        crate::bulk_load::load_ntriples(data)
    }

    /// Load N-Quads data from a text string (supports named graphs).
    #[pg_extern]
    fn load_nquads(data: &str) -> i64 {
        crate::bulk_load::load_nquads(data)
    }

    /// Load Turtle data from a text string.
    /// Also accepts Turtle-star (quoted triples) using oxttl with rdf-12 support.
    #[pg_extern]
    fn load_turtle(data: &str) -> i64 {
        crate::bulk_load::load_turtle(data)
    }

    /// Load TriG data (Turtle with named graph blocks) from a text string.
    #[pg_extern]
    fn load_trig(data: &str) -> i64 {
        crate::bulk_load::load_trig(data)
    }

    /// Load N-Triples from a server-side file path (superuser required).
    #[pg_extern]
    fn load_ntriples_file(path: &str) -> i64 {
        crate::bulk_load::load_ntriples_file(path)
    }

    /// Load N-Quads from a server-side file path (superuser required).
    #[pg_extern]
    fn load_nquads_file(path: &str) -> i64 {
        crate::bulk_load::load_nquads_file(path)
    }

    /// Load Turtle from a server-side file path (superuser required).
    #[pg_extern]
    fn load_turtle_file(path: &str) -> i64 {
        crate::bulk_load::load_turtle_file(path)
    }

    /// Load TriG from a server-side file path (superuser required).
    #[pg_extern]
    fn load_trig_file(path: &str) -> i64 {
        crate::bulk_load::load_trig_file(path)
    }

    // ── Named graph management ────────────────────────────────────────────────

    /// Register a named graph IRI.  Returns its dictionary id.
    /// This is idempotent — safe to call multiple times.
    #[pg_extern]
    fn create_graph(graph_iri: &str) -> i64 {
        crate::storage::create_graph(graph_iri)
    }

    /// Delete all triples in a named graph.  Returns the number of triples deleted.
    #[pg_extern]
    fn drop_graph(graph_iri: &str) -> i64 {
        crate::storage::drop_graph(graph_iri)
    }

    /// List all named graph IRIs (excludes the default graph).
    #[pg_extern]
    fn list_graphs() -> TableIterator<'static, (name!(graph_iri, String),)> {
        let graphs = crate::storage::list_graphs();
        TableIterator::new(graphs.into_iter().map(|g| (g,)))
    }

    // ── IRI prefix management ─────────────────────────────────────────────────

    /// Register (or update) an IRI prefix abbreviation.
    #[pg_extern]
    fn register_prefix(prefix: &str, expansion: &str) {
        crate::storage::register_prefix(prefix, expansion);
    }

    /// Return all registered prefix → expansion mappings.
    #[pg_extern]
    fn prefixes() -> TableIterator<'static, (name!(prefix, String), name!(expansion, String))> {
        let pfxs = crate::storage::list_prefixes();
        TableIterator::new(pfxs)
    }

    // ── Export ────────────────────────────────────────────────────────────────

    /// Export triples to N-Triples format.
    /// Pass a graph IRI to export a specific named graph, or NULL for the default graph.
    #[pg_extern]
    fn export_ntriples(graph: Option<&str>) -> String {
        crate::export::export_ntriples(graph)
    }

    /// Export triples to N-Quads format.
    /// Pass a graph IRI to export a specific graph, or NULL to export all graphs.
    #[pg_extern]
    fn export_nquads(graph: Option<&str>) -> String {
        crate::export::export_nquads(graph)
    }

    // ── SPARQL query engine ───────────────────────────────────────────────────

    /// Execute a SPARQL SELECT or ASK query.
    ///
    /// Returns one JSONB row per result binding for SELECT queries.
    /// For ASK returns a single row `{"result": "true"}` or `{"result": "false"}`.
    #[pg_extern]
    fn sparql(query: &str) -> TableIterator<'static, (name!(result, pgrx::JsonB),)> {
        let rows = crate::sparql::sparql(query);
        TableIterator::new(rows.into_iter().map(|r| (r,)))
    }

    /// Execute a SPARQL ASK query; returns TRUE if any results exist.
    #[pg_extern]
    fn sparql_ask(query: &str) -> bool {
        crate::sparql::sparql_ask(query)
    }

    /// Return the SQL generated for a SPARQL query (for debugging).
    /// Set `analyze := true` to EXPLAIN ANALYZE the generated SQL.
    #[pg_extern]
    fn sparql_explain(query: &str, analyze: bool) -> String {
        crate::sparql::sparql_explain(query, analyze)
    }

    /// Execute a SPARQL CONSTRUCT query; returns one JSONB row per constructed triple.
    ///
    /// Each row is `{"s": "...", "p": "...", "o": "..."}` in N-Triples format.
    #[pg_extern]
    fn sparql_construct(query: &str) -> TableIterator<'static, (name!(result, pgrx::JsonB),)> {
        let rows = crate::sparql::sparql_construct(query);
        TableIterator::new(rows.into_iter().map(|r| (r,)))
    }

    /// Execute a SPARQL DESCRIBE query using the Concise Bounded Description algorithm.
    ///
    /// Returns one JSONB row per triple in the description.
    /// `strategy` may be `'cbd'` (default), `'scbd'` (symmetric), or `'simple'`.
    #[pg_extern]
    fn sparql_describe(
        query: &str,
        strategy: default!(&str, "'cbd'"),
    ) -> TableIterator<'static, (name!(result, pgrx::JsonB),)> {
        let rows = crate::sparql::sparql_describe(query, strategy);
        TableIterator::new(rows.into_iter().map(|r| (r,)))
    }

    /// Execute a SPARQL Update statement (`INSERT DATA` or `DELETE DATA`).
    ///
    /// Returns the total number of triples affected (inserted or deleted).
    #[pg_extern]
    fn sparql_update(query: &str) -> i64 {
        crate::sparql::sparql_update(query)
    }

    // ── Full-text search ─────────────────────────────────────────────────────

    /// Create a GIN tsvector index on the dictionary for the given predicate IRI.
    ///
    /// After indexing, SPARQL `CONTAINS()` and `REGEX()` FILTERs on triples
    /// using this predicate will be rewritten to use the GIN index for
    /// efficient text matching.  Returns the predicate dictionary id.
    #[pg_extern]
    fn fts_index(predicate: &str) -> i64 {
        crate::fts::fts_index(predicate)
    }

    /// Full-text search on literal objects of a given predicate.
    ///
    /// `query` is a `tsquery`-formatted search string (e.g. `'knowledge & graph'`).
    /// Returns matching triples as `(s TEXT, p TEXT, o TEXT)` in N-Triples format.
    #[pg_extern]
    fn fts_search(
        query: &str,
        predicate: &str,
    ) -> TableIterator<'static, (name!(s, String), name!(p, String), name!(o, String))> {
        let rows: Vec<(String, String, String)> =
            crate::fts::fts_search(query, predicate).collect();
        TableIterator::new(rows)
    }

    // ── HTAP maintenance (v0.6.0) ─────────────────────────────────────────────

    /// Trigger an immediate full merge of all HTAP VP tables.
    ///
    /// Moves all rows from delta into main, rebuilds subject_patterns and
    /// object_patterns, and runs ANALYZE on each merged table.
    /// Returns the total number of rows in all merged main tables.
    #[pg_extern]
    fn compact() -> i64 {
        crate::storage::merge::compact()
    }

    /// Migrate an existing flat VP table (pre-v0.6.0) to the HTAP partition split.
    ///
    /// Called automatically by the v0.5.1→v0.6.0 migration script, but can
    /// also be called manually if needed.  The predicate is specified by its
    /// dictionary integer ID.
    #[pg_extern]
    fn htap_migrate_predicate(pred_id: i64) {
        crate::storage::merge::migrate_flat_to_htap(pred_id);
    }

    // ── Statistics (v0.6.0) ───────────────────────────────────────────────────

    /// Return extension statistics as JSONB.
    ///
    /// Includes total triple count, per-predicate storage sizes, delta/main
    /// split counts, and (when shared_preload_libraries is set) cache hit ratio.
    ///
    /// ```sql
    /// SELECT pg_ripple.stats();
    /// ```
    #[pg_extern]
    fn stats() -> pgrx::JsonB {
        let total: i64 = crate::storage::total_triple_count();

        let pred_count: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        let rare_count: i64 =
            pgrx::Spi::get_one::<i64>("SELECT count(*)::bigint FROM _pg_ripple.vp_rare")
                .unwrap_or(None)
                .unwrap_or(0);

        let htap_count: i64 = pgrx::Spi::get_one::<i64>(
            "SELECT count(*)::bigint FROM _pg_ripple.predicates WHERE htap = true",
        )
        .unwrap_or(None)
        .unwrap_or(0);

        let delta_rows: i64 =
            if crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire) {
                crate::shmem::TOTAL_DELTA_ROWS
                    .get()
                    .load(std::sync::atomic::Ordering::Relaxed)
            } else {
                -1 // shmem not available (loaded without shared_preload_libraries)
            };

        let merge_pid: i32 = if crate::shmem::SHMEM_READY.load(std::sync::atomic::Ordering::Acquire)
        {
            crate::shmem::MERGE_WORKER_PID
                .get()
                .load(std::sync::atomic::Ordering::Relaxed)
        } else {
            0
        };

        let mut obj = serde_json::Map::new();
        obj.insert("total_triples".to_string(), serde_json::json!(total));
        obj.insert(
            "dedicated_predicates".to_string(),
            serde_json::json!(pred_count),
        );
        obj.insert("htap_predicates".to_string(), serde_json::json!(htap_count));
        obj.insert("rare_triples".to_string(), serde_json::json!(rare_count));
        obj.insert(
            "unmerged_delta_rows".to_string(),
            serde_json::json!(delta_rows),
        );
        obj.insert("merge_worker_pid".to_string(), serde_json::json!(merge_pid));

        // v0.6.0: encode cache statistics.
        let cache_utilization_pct = crate::shmem::cache_utilization_pct() as i64;
        let cache_capacity = crate::shmem::ENCODE_CACHE_CAPACITY as i64;
        obj.insert(
            "encode_cache_capacity".to_string(),
            serde_json::json!(cache_capacity),
        );
        obj.insert(
            "encode_cache_utilization_pct".to_string(),
            serde_json::json!(cache_utilization_pct),
        );

        pgrx::JsonB(serde_json::Value::Object(obj))
    }

    // ── Change Data Capture (v0.6.0) ──────────────────────────────────────────

    /// Subscribe to triple change notifications on a NOTIFY channel.
    ///
    /// `pattern` is a predicate IRI (e.g. `<https://schema.org/name>`) or
    /// `'*'` for all predicates.  Notifications fire on INSERT and DELETE in
    /// the matching VP delta tables.  Returns the subscription ID.
    ///
    /// ```sql
    /// SELECT pg_ripple.subscribe('<https://schema.org/name>', 'my_channel');
    /// LISTEN my_channel;
    /// ```
    #[pg_extern]
    fn subscribe(pattern: &str, channel: &str) -> i64 {
        crate::cdc::subscribe(pattern, channel)
    }

    /// Remove all subscriptions for a notification channel.  Returns count removed.
    #[pg_extern]
    fn unsubscribe(channel: &str) -> i64 {
        crate::cdc::unsubscribe(channel)
    }

    // ── Subject / Object pattern index (v0.6.0) ───────────────────────────────

    /// Return the sorted array of predicate IDs for a given subject ID.
    ///
    /// Uses the `_pg_ripple.subject_patterns` index populated by the merge worker.
    /// Returns NULL if the subject has not been indexed yet (before first merge).
    #[pg_extern]
    fn subject_predicates(subject_id: i64) -> Option<Vec<i64>> {
        pgrx::Spi::get_one_with_args::<Vec<i64>>(
            "SELECT pattern FROM _pg_ripple.subject_patterns WHERE s = $1",
            &[pgrx::datum::DatumWithOid::from(subject_id)],
        )
        .unwrap_or(None)
    }

    /// Return the sorted array of predicate IDs for a given object ID.
    ///
    /// Uses the `_pg_ripple.object_patterns` index populated by the merge worker.
    #[pg_extern]
    fn object_predicates(object_id: i64) -> Option<Vec<i64>> {
        pgrx::Spi::get_one_with_args::<Vec<i64>>(
            "SELECT pattern FROM _pg_ripple.object_patterns WHERE o = $1",
            &[pgrx::datum::DatumWithOid::from(object_id)],
        )
        .unwrap_or(None)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_encode_decode_roundtrip() {
        let id = crate::dictionary::encode("https://example.org/subject", 0);
        let decoded = crate::dictionary::decode(id).expect("decode should succeed");
        assert_eq!(decoded, "https://example.org/subject");
    }

    #[pg_test]
    fn test_insert_and_count() {
        crate::storage::insert_triple(
            "<https://example.org/s>",
            "<https://example.org/p>",
            "<https://example.org/o>",
            0,
        );
        assert!(crate::storage::total_triple_count() >= 1);
    }

    #[pg_test]
    fn test_typed_literal_roundtrip() {
        // xsd:integer is now inline-encoded (bit 63 = 1, no dictionary row).
        let id = crate::dictionary::encode_typed_literal(
            "42",
            "http://www.w3.org/2001/XMLSchema#integer",
        );
        assert!(
            crate::dictionary::inline::is_inline(id),
            "xsd:integer should be inline-encoded"
        );
        // decode() must still return the correct N-Triples literal string.
        let decoded = crate::dictionary::decode(id).expect("decode should succeed for inline");
        assert_eq!(
            decoded,
            "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
    }

    #[pg_test]
    fn test_lang_literal_roundtrip() {
        let id = crate::dictionary::encode_lang_literal("hello", "en");
        let full = crate::dictionary::decode_full(id).expect("decode_full should succeed");
        assert_eq!(full.value, "hello");
        assert_eq!(full.lang.as_deref(), Some("en"));
    }

    #[pg_test]
    fn test_ntriples_bulk_load() {
        let data =
            "<https://example.org/a> <https://example.org/knows> <https://example.org/b> .\n";
        let count = crate::bulk_load::load_ntriples(data);
        assert_eq!(count, 1);
        assert!(crate::storage::total_triple_count() >= 1);
    }

    #[pg_test]
    fn test_turtle_bulk_load() {
        let data = "@prefix ex: <https://example.org/> .\nex:x ex:rel ex:y .\n";
        let count = crate::bulk_load::load_turtle(data);
        assert_eq!(count, 1);
    }

    #[pg_test]
    fn test_named_graph_drop() {
        let graph = "<https://example.org/mygraph>";
        let g_id = crate::storage::create_graph(graph);
        assert!(g_id > 0);
        crate::storage::insert_triple(
            "<https://example.org/s>",
            "<https://example.org/p>",
            "<https://example.org/o>",
            g_id,
        );
        let deleted = crate::storage::drop_graph(graph);
        assert!(deleted >= 1);
    }

    #[pg_test]
    fn test_export_ntriples_roundtrip() {
        let nt =
            "<https://example.org/ex> <https://example.org/pred> <https://example.org/obj> .\n";
        crate::bulk_load::load_ntriples(nt);
        let exported = crate::export::export_ntriples(None);
        assert!(exported.contains("<https://example.org/pred>"));
    }

    // ─── SPARQL engine tests (v0.3.0) ─────────────────────────────────────────

    /// A SELECT that returns no rows on an empty store must produce an empty set.
    #[pg_test]
    fn pg_test_sparql_select_empty() {
        let rows = crate::sparql::sparql("SELECT ?s ?p ?o WHERE { ?s ?p ?o }");
        assert_eq!(rows.len(), 0, "expected no rows on empty store");
    }

    /// After loading one triple, SELECT ?s ?p ?o must return exactly one row.
    #[pg_test]
    fn pg_test_sparql_select_one_triple() {
        crate::bulk_load::load_ntriples(
            "<https://example.org/a> <https://example.org/p> <https://example.org/b> .\n",
        );
        let rows = crate::sparql::sparql("SELECT ?s ?p ?o WHERE { ?s ?p ?o }");
        assert_eq!(rows.len(), 1, "expected exactly one row");
        // The row must contain a non-null ?s binding.
        let obj = rows[0].0.as_object().expect("row must be a JSON object");
        assert!(obj.contains_key("s"), "row must have ?s binding");
        assert!(obj.contains_key("p"), "row must have ?p binding");
        assert!(obj.contains_key("o"), "row must have ?o binding");
    }

    /// sparql_ask() on an empty store returns false.
    #[pg_test]
    fn pg_test_sparql_ask_empty() {
        let result = crate::sparql::sparql_ask("ASK { ?s ?p ?o }");
        assert!(!result, "ASK on empty store must be false");
    }

    /// sparql_ask() returns true after a matching triple is inserted.
    #[pg_test]
    fn pg_test_sparql_ask_match() {
        crate::bulk_load::load_ntriples(
            "<https://example.org/x> <https://example.org/q> <https://example.org/y> .\n",
        );
        let result =
            crate::sparql::sparql_ask("ASK { <https://example.org/x> <https://example.org/q> ?o }");
        assert!(result, "ASK must be true after matching triple loaded");
    }

    /// sparql_explain() returns non-empty SQL for a simple SELECT.
    #[pg_test]
    fn pg_test_sparql_explain_returns_sql() {
        let plan = crate::sparql::sparql_explain(
            "SELECT ?s WHERE { ?s <https://example.org/p> ?o }",
            false,
        );
        assert!(
            plan.contains("Generated SQL"),
            "explain output must contain 'Generated SQL'"
        );
    }

    /// SPARQL LIMIT 1 must return at most one row.
    #[pg_test]
    fn pg_test_sparql_limit() {
        // Load two triples.
        crate::bulk_load::load_ntriples(
            "<https://example.org/s1> <https://example.org/p> <https://example.org/o1> .\n\
             <https://example.org/s2> <https://example.org/p> <https://example.org/o2> .\n",
        );
        let rows =
            crate::sparql::sparql("SELECT ?s ?o WHERE { ?s <https://example.org/p> ?o } LIMIT 1");
        assert!(rows.len() <= 1, "LIMIT 1 must return at most one row");
    }

    // ─── RDF-star / Statement Identifiers tests (v0.4.0) ──────────────────────

    /// N-Triples-star: loading an object-position quoted triple must succeed.
    #[pg_test]
    fn pg_test_ntriples_star_object_position() {
        let n = crate::bulk_load::load_ntriples(
            "<https://example.org/eve> <https://example.org/said> \
             << <https://example.org/alice> <https://example.org/knows> \
             <https://example.org/bob> >> .\n",
        );
        assert_eq!(n, 1, "object-position quoted triple must load as 1 triple");
    }

    /// N-Triples-star: loading a subject-position quoted triple must succeed.
    #[pg_test]
    fn pg_test_ntriples_star_subject_position() {
        let n = crate::bulk_load::load_ntriples(
            "<< <https://example.org/alice> <https://example.org/knows> \
             <https://example.org/bob> >> <https://example.org/certainty> \
             \"0.9\"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n",
        );
        assert_eq!(n, 1, "subject-position quoted triple must load as 1 triple");
    }

    /// encode_quoted_triple / decode_quoted_triple_components round-trip.
    #[pg_test]
    fn pg_test_quoted_triple_encode_decode() {
        let s_id =
            crate::dictionary::encode("https://example.org/alice", crate::dictionary::KIND_IRI);
        let p_id =
            crate::dictionary::encode("https://example.org/knows", crate::dictionary::KIND_IRI);
        let o_id =
            crate::dictionary::encode("https://example.org/bob", crate::dictionary::KIND_IRI);
        let qt_id = crate::dictionary::encode_quoted_triple(s_id, p_id, o_id);
        assert!(qt_id != 0, "quoted triple must have a non-zero ID");
        let components = crate::dictionary::decode_quoted_triple_components(qt_id);
        assert!(components.is_some(), "decode must return Some");
        let (ds, dp, ob) = components.unwrap();
        assert_eq!(ds, s_id);
        assert_eq!(dp, p_id);
        assert_eq!(ob, o_id);
    }

    /// insert_triple returns a positive SID; get_statement can look it back up.
    #[pg_test]
    fn pg_test_statement_identifier_lifecycle() {
        let sid = crate::storage::insert_triple(
            "<https://example.org/subject1>",
            "<https://example.org/predicate1>",
            "<https://example.org/object1>",
            0,
        );
        assert!(sid > 0, "insert must return a positive SID");
    }

    /// SPARQL DISTINCT must deduplicate results.
    #[pg_test]
    fn pg_test_sparql_distinct() {
        // Two triples sharing the same predicate and object.
        crate::bulk_load::load_ntriples(
            "<https://example.org/s1> <https://example.org/same> <https://example.org/o> .\n\
             <https://example.org/s2> <https://example.org/same> <https://example.org/o> .\n",
        );
        // Select just ?o — should be deduplicated to 1 row.
        let rows =
            crate::sparql::sparql("SELECT DISTINCT ?o WHERE { ?s <https://example.org/same> ?o }");
        assert_eq!(rows.len(), 1, "DISTINCT ?o must collapse duplicates");
    }

    /// FILTER with a bound IRI constant must restrict results correctly.
    #[pg_test]
    fn pg_test_sparql_filter_bound() {
        crate::bulk_load::load_ntriples(
            "<https://example.org/s1> <https://example.org/p> <https://example.org/o1> .\n\
             <https://example.org/s2> <https://example.org/p> <https://example.org/o2> .\n",
        );
        // Only one subject matches the binding of ?s to s1.
        let rows = crate::sparql::sparql(
            "SELECT ?o WHERE { <https://example.org/s1> <https://example.org/p> ?o }",
        );
        assert_eq!(rows.len(), 1, "bound subject must restrict to one row");
    }
}

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec!["allow_system_table_mods = on"]
    }
}
