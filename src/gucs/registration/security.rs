//! GUC registration for security and access control (Q13-01, v0.84.0).
//! Split from registration.rs for navigability.

#[allow(unused_imports)]
use crate::gucs::*;
use pgrx::guc::{GucContext, GucFlags};
#[allow(unused_imports)]
use pgrx::prelude::*;

/// Register all GUCs for this domain.
pub fn register() {
    // ── v0.14.0 GUCs ─────────────────────────────────────────────────────────

    // v0.37.0: rls_bypass is elevated to PGC_POSTMASTER so it cannot be
    // flipped per-session (a user could otherwise bypass RLS with SET LOCAL).
    // This requires the registration to happen only during shared_preload_libraries
    // loading (where Postmaster-context GUCs are accepted).
    // When loaded outside that context (e.g. direct CREATE EXTENSION), fall back
    // to Suset context so the GUC is still registered.
    {
        let ctx = if unsafe { pgrx::pg_sys::process_shared_preload_libraries_in_progress } {
            GucContext::Postmaster
        } else {
            GucContext::Suset
        };
        pgrx::GucRegistry::define_bool_guc(
            c"pg_ripple.rls_bypass",
            c"Superuser override: when on, graph-level RLS policies are bypassed; \
          cannot be changed per-session (v0.37.0: PGC_POSTMASTER scope)",
            c"",
            &RLS_BYPASS,
            ctx,
            GucFlags::default(),
        );
    }

    // ── v0.51.0 GUCs — Security Hardening & Production Readiness ─────────────
    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.sparql_max_algebra_depth",
        c"Maximum allowed SPARQL algebra tree depth; queries deeper than this are \
      rejected with PT440 (default: 256, 0=disabled). (v0.51.0)",
        c"",
        &SPARQL_MAX_ALGEBRA_DEPTH,
        0,
        65535,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.sparql_max_triple_patterns",
        c"Maximum number of triple patterns in a single SPARQL query; queries \
      exceeding this are rejected with PT440 (default: 4096, 0=disabled). (v0.51.0)",
        c"",
        &SPARQL_MAX_TRIPLE_PATTERNS,
        0,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.tracing_otlp_endpoint",
        c"OTLP collector endpoint for OpenTelemetry span export when \
      tracing_exporter = 'otlp' (e.g. 'http://jaeger:4318/v1/traces'). \
      Falls back to OTEL_EXPORTER_OTLP_ENDPOINT env var if empty. (v0.51.0)",
        c"",
        &TRACING_OTLP_ENDPOINT,
        GucContext::Userset,
        GucFlags::default(),
    );

    // ── v0.55.0 GUCs — Security & Storage Quality ────────────────────────────

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.tombstone_retention_seconds",
        c"Seconds to retain tombstones after a merge cycle. \
      0 (default) = truncate tombstones immediately after a full merge. (v0.55.0)",
        c"",
        &TOMBSTONE_RETENTION_SECONDS,
        0,
        86400,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.normalize_iris",
        c"When on (default), normalize IRI strings to NFC before dictionary encoding. \
      Ensures that semantically equivalent IRIs with different Unicode normalization \
      map to the same dictionary entry. (v0.55.0)",
        c"",
        &NORMALIZE_IRIS,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.copy_rdf_allowed_paths",
        c"Comma-separated list of allowed path prefixes for copy_rdf_from(). \
      When set, only paths matching a listed prefix are permitted. \
      When empty (default), ALL paths are denied (PT403 default-deny policy). (v0.55.0)",
        c"",
        &COPY_RDF_ALLOWED_PATHS,
        GucContext::Sighup,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.read_replica_dsn",
        c"DSN for a read replica to route SELECT/CONSTRUCT/ASK/DESCRIBE queries to. \
      Falls back to primary on connection failure (PT530 WARNING). (v0.55.0)",
        c"",
        &READ_REPLICA_DSN,
        GucContext::Userset,
        GucFlags::default(),
    );
}
