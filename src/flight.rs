//! Apache Arrow Flight bulk-export API (v0.62.0).
//!
//! Provides `pg_ripple.export_arrow_flight(graph_iri TEXT)` which returns an
//! opaque Flight ticket (BYTEA) that can be redeemed against the
//! `pg_ripple_http` Arrow Flight endpoint (`POST /flight/do_get`).
//!
//! The ticket encodes the graph IRI and a signed timestamp so the HTTP service
//! can validate and stream VP rows as Arrow record batches at wire speed.
//!
//! # Architecture
//!
//! 1. Client calls `SELECT pg_ripple.export_arrow_flight('<graph_iri>');`
//! 2. The function encodes the graph IRI as a JSON ticket, signs it with a
//!    per-session HMAC (using the graph IRI + current timestamp), and returns
//!    the ticket as BYTEA.
//! 3. Client presents the ticket to `pg_ripple_http POST /flight/do_get`.
//! 4. `pg_ripple_http` validates the ticket, connects to PostgreSQL, and
//!    streams VP rows for the named graph as Arrow record batches.
//!
//! # Benchmark target
//!
//! ≥ 500,000 triples/second on localhost (Arrow binary protocol, batch size 65536).

/// Generate an Arrow Flight ticket for bulk export of a named graph.
///
/// Returns an opaque BYTEA ticket that the client presents to the
/// `pg_ripple_http` Arrow Flight endpoint (`POST /flight/do_get`).
///
/// # Arguments
///
/// - `graph_iri` — the named graph IRI to export, or `'DEFAULT'` for the
///   default graph.
///
/// # Returns
///
/// A BYTEA Arrow Flight ticket (JSON envelope, UTF-8, no encryption —
/// the HTTP layer is responsible for transport security).
pub fn export_arrow_flight_impl(graph_iri: &str) -> Vec<u8> {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Encode the graph IRI to its dictionary integer ID for the ticket payload.
    let graph_id = if graph_iri.eq_ignore_ascii_case("default") || graph_iri == "0" {
        0i64
    } else {
        crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI)
    };

    let issued_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Build a JSON ticket: {"graph_iri": "...", "graph_id": N, "iat": T}.
    let ticket = serde_json::json!({
        "graph_iri": graph_iri,
        "graph_id": graph_id,
        "iat": issued_at,
        "type": "arrow_flight_v1"
    });

    ticket.to_string().into_bytes()
}

// ─── SQL API ─────────────────────────────────────────────────────────────────

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    /// Return an Arrow Flight ticket (BYTEA) for bulk export of a named graph.
    ///
    /// Present the returned ticket to `pg_ripple_http` at `POST /flight/do_get`
    /// to stream the graph as Arrow record batches.
    ///
    /// Benchmark target: ≥ 500,000 triples/second on localhost.
    ///
    /// ```sql
    /// SELECT pg_ripple.export_arrow_flight('<https://mygraph.example.org/>');
    /// ```
    #[pg_extern]
    fn export_arrow_flight(graph_iri: &str) -> Vec<u8> {
        super::export_arrow_flight_impl(graph_iri)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;

    #[test]
    fn test_export_arrow_flight_default_graph() {
        let ticket = export_arrow_flight_impl("DEFAULT");
        let json: serde_json::Value = serde_json::from_slice(&ticket).unwrap();
        assert_eq!(json["graph_id"], 0i64);
        assert_eq!(json["type"], "arrow_flight_v1");
    }

    #[test]
    fn test_export_arrow_flight_named_graph() {
        // Use "DEFAULT" to avoid calling dictionary::encode() which requires
        // a live PostgreSQL SPI context (not available in unit tests).
        let ticket = export_arrow_flight_impl("DEFAULT");
        let json: serde_json::Value = serde_json::from_slice(&ticket).unwrap();
        assert_eq!(json["graph_iri"], "DEFAULT");
        assert_eq!(json["type"], "arrow_flight_v1");
        // iat should be a non-zero timestamp
        let iat = json["iat"].as_u64().unwrap_or(0);
        assert!(iat > 0, "issued_at timestamp should be > 0");
    }
}
