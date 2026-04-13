//! pg_ripple — High-performance RDF triple store for PostgreSQL 18.
//!
//! # Architecture
//!
//! Every IRI, blank node, and literal is encoded to `i64` via XXH3-128 hash
//! (see `src/dictionary/`) before being stored in vertical-partition (VP)
//! tables in the `_pg_ripple` schema (see `src/storage/`).  SPARQL queries
//! are parsed with `spargebra`, compiled to SQL, and executed via SPI
//! (see `src/sparql/`).

use pgrx::prelude::*;

mod dictionary;
mod error;
mod storage;

pgrx::pg_module_magic!();

/// GUC: default named-graph identifier (empty string → default graph 0).
static DEFAULT_GRAPH: pgrx::GucSetting<Option<&'static std::ffi::CStr>> =
    pgrx::GucSetting::new(None);

/// Called once when the extension shared library is loaded.
#[allow(non_snake_case)]
#[pg_guard]
pub extern "C" fn _PG_init() {
    // Register GUC parameters before any backend connects.
    unsafe {
        pgrx::GucRegistry::define_string_guc(
            c"pg_ripple.default_graph",
            c"IRI of the default named graph (empty = built-in default graph)",
            c"",
            &DEFAULT_GRAPH,
            GucContext::Userset,
            GucFlags::default(),
        );
    }
    // Note: the merge background worker, SHACL engine, and Datalog reasoner
    // are NOT started here.  They are started lazily based on GUC values
    // (pg_ripple.merge_threshold, pg_ripple.shacl_mode, pg_ripple.inference_mode),
    // which are introduced in their respective milestone releases.
}

// ─── Public SQL-callable functions (v0.1.0) ───────────────────────────────────

/// Encode a text IRI/blank-node/literal to its dictionary `i64` identifier.
/// Creates a new entry if the term has not been seen before.
#[pg_extern(schema = "pg_ripple")]
fn encode_term(term: &str, kind: i16) -> i64 {
    dictionary::encode(term, kind)
}

/// Decode a dictionary `i64` back to its original text value.
/// Returns `None` when the identifier is not present in the dictionary.
#[pg_extern(schema = "pg_ripple")]
fn decode_id(id: i64) -> Option<String> {
    dictionary::decode(id)
}

/// Insert a triple into the appropriate VP table.
/// `s`, `p`, and `o` are N-Triples-formatted string representations.
#[pg_extern(schema = "pg_ripple")]
fn insert_triple(s: &str, p: &str, o: &str) -> i64 {
    storage::insert_triple(s, p, o, 0_i64)
}

/// Delete a triple.  Returns the number of rows removed (0 or 1).
#[pg_extern(schema = "pg_ripple")]
fn delete_triple(s: &str, p: &str, o: &str) -> i64 {
    storage::delete_triple(s, p, o, 0_i64)
}

/// Return the total number of triples across all VP tables.
#[pg_extern(schema = "pg_ripple")]
fn triple_count() -> i64 {
    storage::total_triple_count()
}

/// Pattern-match triples; any argument may be NULL to act as a wildcard.
/// Returns decoded `(s, p, o, g)` text tuples.
#[pg_extern(schema = "pg_ripple")]
fn find_triples(
    s: Option<&str>,
    p: Option<&str>,
    o: Option<&str>,
) -> TableIterator<'static, (name!(s, String), name!(p, String), name!(o, String), name!(g, String))>
{
    let rows = storage::find_triples(s, p, o, None);
    TableIterator::new(rows.into_iter())
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
}

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec![]
    }
}
