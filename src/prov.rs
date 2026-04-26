//! PROV-O graph provenance tracking (v0.58.0, Feature L-8.4).
//!
//! When `pg_ripple.prov_enabled = on`, every bulk-load operation emits a set of
//! PROV-O provenance triples into the default named graph
//! `<urn:pg_ripple:prov>`:
//!
//! ```turtle
//! @prefix prov: <http://www.w3.org/ns/prov#> .
//! @prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
//!
//! <urn:pg_ripple:activity:{uuid}>
//!     a prov:Activity ;
//!     prov:startedAtTime "..."^^xsd:dateTime ;
//!     prov:endedAtTime   "..."^^xsd:dateTime ;
//!     prov:generated     <urn:pg_ripple:entity:{source_md5}> .
//!
//! <urn:pg_ripple:entity:{source_md5}>
//!     a prov:Entity ;
//!     prov:wasAttributedTo <urn:pg_ripple:agent:pg_user> ;
//!     pg_ripple:tripleCount {count}^^xsd:integer .
//! ```
//!
//! The `_pg_ripple.prov_catalog` table stores a summary view for fast access
//! without parsing the provenance graph.

use pgrx::prelude::*;

// ─── Provenance graph IRI ─────────────────────────────────────────────────────

const PROV_GRAPH_IRI: &str = "urn:pg_ripple:prov";
const PROV_NS: &str = "http://www.w3.org/ns/prov#";
const XSD_DATE_TIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const PG_RIPPLE_NS: &str = "urn:pg_ripple:vocab#";

// ─── Catalog initialisation ───────────────────────────────────────────────────

/// Create `_pg_ripple.prov_catalog` (idempotent).
pub fn initialize_prov_schema() {
    Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.prov_catalog ( \
             source        TEXT        NOT NULL PRIMARY KEY, \
             activity_iri  TEXT        NOT NULL, \
             triple_count  BIGINT      NOT NULL DEFAULT 0, \
             last_updated  TIMESTAMPTZ NOT NULL DEFAULT now() \
         )",
        &[],
    )
    .unwrap_or_else(|e| pgrx::warning!("prov_catalog creation: {e}"));
}

// ─── Provenance emission ──────────────────────────────────────────────────────

/// Emit PROV-O provenance triples for a completed bulk load.
///
/// Inserts activity and entity triples into the provenance named graph and
/// updates `_pg_ripple.prov_catalog`.
///
/// # Arguments
/// - `source` — a label identifying the data source (file path, URL, or label)
/// - `triple_count` — the number of triples loaded
///
/// # Behaviour when disabled
/// Does nothing if `pg_ripple.prov_enabled = off`.
pub fn emit_load_provenance(source: &str, triple_count: i64) {
    if !crate::gucs::storage::PROV_ENABLED.get() {
        return;
    }

    // Generate a UUID-like activity IRI using the current timestamp + source hash.
    let activity_iri = format!(
        "urn:pg_ripple:activity:{:016x}",
        crate::dictionary::encode(source, crate::dictionary::KIND_LITERAL).unsigned_abs()
            ^ triple_count.unsigned_abs()
    );

    let entity_iri = format!(
        "urn:pg_ripple:entity:{:016x}",
        crate::dictionary::encode(source, crate::dictionary::KIND_LITERAL).unsigned_abs()
    );

    let agent_iri = "urn:pg_ripple:agent:pg_user";
    let prov_activity = format!("{PROV_NS}Activity");
    let prov_entity = format!("{PROV_NS}Entity");
    let prov_generated = format!("{PROV_NS}generated");
    let prov_was_attributed = format!("{PROV_NS}wasAttributedTo");
    let prov_started = format!("{PROV_NS}startedAtTime");
    let prov_ended = format!("{PROV_NS}endedAtTime");
    let triple_count_pred = format!("{PG_RIPPLE_NS}tripleCount");

    // Get current timestamp as xsd:dateTime string.
    let ts: String = Spi::get_one::<String>(
        "SELECT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS') || 'Z'",
    )
    .unwrap_or(None)
    .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

    // Determine provenance graph ID.
    let g_id = crate::dictionary::encode(PROV_GRAPH_IRI, crate::dictionary::KIND_IRI);

    // Insert provenance triples using batch_insert_encoded.
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    let triples: &[(&str, &str, &str, i64)] = &[
        // Activity rdf:type prov:Activity
        (&activity_iri, rdf_type, &prov_activity, g_id),
        // Activity prov:startedAtTime ts
        (&activity_iri, &prov_started, &ts, g_id),
        // Activity prov:endedAtTime ts
        (&activity_iri, &prov_ended, &ts, g_id),
        // Activity prov:generated entity
        (&activity_iri, &prov_generated, &entity_iri, g_id),
        // Entity rdf:type prov:Entity
        (&entity_iri, rdf_type, &prov_entity, g_id),
        // Entity prov:wasAttributedTo agent
        (&entity_iri, &prov_was_attributed, agent_iri, g_id),
    ];

    for (s_iri, p_iri, o_iri, g) in triples {
        let s = crate::dictionary::encode(s_iri, crate::dictionary::KIND_IRI);
        let p = crate::dictionary::encode(p_iri, crate::dictionary::KIND_IRI);
        let o = if p_iri.starts_with(&format!("{PROV_NS}startedAt"))
            || p_iri.starts_with(&format!("{PROV_NS}endedAt"))
        {
            crate::dictionary::encode_typed_literal(o_iri, XSD_DATE_TIME)
        } else if p_iri == &triple_count_pred {
            crate::dictionary::encode_typed_literal(o_iri, XSD_INTEGER)
        } else {
            crate::dictionary::encode(o_iri, crate::dictionary::KIND_IRI)
        };
        crate::storage::insert_encoded_triple(s, p, o, *g);
    }

    // Also insert the triple count fact.
    let tc_s = crate::dictionary::encode(&entity_iri, crate::dictionary::KIND_IRI);
    let tc_p = crate::dictionary::encode(&triple_count_pred, crate::dictionary::KIND_IRI);
    let tc_o = crate::dictionary::encode_typed_literal(&triple_count.to_string(), XSD_INTEGER);
    crate::storage::insert_encoded_triple(tc_s, tc_p, tc_o, g_id);

    // Update the prov_catalog summary.
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.prov_catalog (source, activity_iri, triple_count, last_updated) \
         VALUES ($1, $2, $3, now()) \
         ON CONFLICT (source) DO UPDATE \
         SET activity_iri  = EXCLUDED.activity_iri, \
             triple_count  = EXCLUDED.triple_count, \
             last_updated  = EXCLUDED.last_updated",
        &[
            pgrx::datum::DatumWithOid::from(source),
            pgrx::datum::DatumWithOid::from(activity_iri.as_str()),
            pgrx::datum::DatumWithOid::from(triple_count),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("prov_catalog upsert: {e}"));
}

// ─── SQL API ──────────────────────────────────────────────────────────────────

/// Return provenance statistics from the catalog.
///
/// Columns: `source TEXT, activity_iri TEXT, triple_count BIGINT, last_updated TIMESTAMPTZ`.
#[pg_extern(schema = "pg_ripple")]
pub fn prov_stats() -> TableIterator<
    'static,
    (
        name!(source, String),
        name!(activity_iri, String),
        name!(triple_count, i64),
        name!(last_updated, pgrx::datum::TimestampWithTimeZone),
    ),
> {
    let rows = Spi::connect(|c| {
        c.select(
            "SELECT source, activity_iri, triple_count, last_updated \
             FROM _pg_ripple.prov_catalog \
             ORDER BY last_updated DESC",
            None,
            &[],
        )
        .map(|rows| {
            rows.filter_map(|row| {
                let source = row.get::<String>(1).ok().flatten()?;
                let activity_iri = row.get::<String>(2).ok().flatten().unwrap_or_default();
                let triple_count = row.get::<i64>(3).ok().flatten().unwrap_or(0);
                let last_updated = row
                    .get::<pgrx::datum::TimestampWithTimeZone>(4)
                    .ok()
                    .flatten()?;
                Some((source, activity_iri, triple_count, last_updated))
            })
            .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    });

    TableIterator::new(rows)
}

/// Return `true` if PROV-O provenance tracking is enabled.
#[pg_extern(schema = "pg_ripple")]
pub fn prov_enabled() -> bool {
    crate::gucs::storage::PROV_ENABLED.get()
}
