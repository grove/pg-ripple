//! Apache Arrow Flight bulk-export API (v0.62.0, signed tickets v0.66.0 FLIGHT-01,
//! security hardening v0.67.0 FLIGHT-SEC-01/02).
//!
//! Provides `pg_ripple.export_arrow_flight(graph_iri TEXT)` which returns an
//! opaque Flight ticket (BYTEA) that can be redeemed against the
//! `pg_ripple_http` Arrow Flight endpoint (`POST /flight/do_get`).
//!
//! # v0.66.0 changes (FLIGHT-01)
//!
//! Tickets are now HMAC-SHA256 signed and include:
//! - `iat`   — issued-at UNIX timestamp
//! - `exp`   — expiry UNIX timestamp (iat + `pg_ripple.arrow_flight_expiry_secs`)
//! - `aud`   — audience (fixed value `"pg_ripple_http"`)
//! - `nonce` — replay guard (random UUID from `gen_random_uuid()` via SPI)
//! - `sig`   — HMAC-SHA256(canonical_payload, `pg_ripple.arrow_flight_secret`),
//!   hex-encoded
//!
//! # v0.67.0 changes (FLIGHT-SEC-01)
//!
//! - Unsigned tickets (`sig = "unsigned"`) are rejected by default.
//!   Set GUC `pg_ripple.arrow_unsigned_tickets_allowed = on` (or env var
//!   `ARROW_UNSIGNED_TICKETS_ALLOWED=true`) for local development only.
//! - `build_signed_ticket` errors when the secret is empty and unsigned
//!   tickets are not explicitly allowed.
//!
//! The `pg_ripple_http` service validates the HMAC, expiry, and audience
//! before serving any data.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

// ─── Implementation ───────────────────────────────────────────────────────────

/// Generate a signed Arrow Flight ticket for bulk export of a named graph.
///
/// # Arguments
///
/// - `graph_iri`  — the named graph IRI, or `"DEFAULT"` for the default graph.
/// - `secret`     — HMAC signing secret; use `pg_ripple.arrow_flight_secret`.
/// - `expiry_secs`— ticket validity in seconds (default from GUC).
/// - `nonce`      — replay-guard value (random UUID from SPI, or a test value).
/// - `allow_unsigned` — when `true`, an empty secret produces an unsigned ticket
///   (for local development only).  When `false` (default in production), an
///   empty secret is a hard error.
///
/// # Returns
///
/// UTF-8 JSON bytes suitable for returning as PostgreSQL BYTEA.
pub fn build_signed_ticket(
    graph_iri: &str,
    graph_id: i64,
    secret: &str,
    expiry_secs: u64,
    nonce: &str,
    allow_unsigned: bool,
) -> Vec<u8> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let issued_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let expiry = issued_at + expiry_secs;

    // Canonical payload for HMAC: deterministic JSON, sorted keys.
    // We sign only the fields that cannot change after ticket issuance.
    let canonical = format!(
        "aud=pg_ripple_http,exp={expiry},graph_id={graph_id},graph_iri={graph_iri},iat={issued_at},nonce={nonce},type=arrow_flight_v2"
    );

    let sig = if secret.is_empty() {
        if !allow_unsigned {
            // FLIGHT-SEC-01: reject unsigned tickets unless explicitly enabled.
            pgrx::error!(
                "Arrow Flight ticket cannot be issued: pg_ripple.arrow_flight_secret is not set \
                 and pg_ripple.arrow_unsigned_tickets_allowed is off. \
                 Set a signing secret or enable unsigned tickets for development."
            );
        }
        // Local development mode: produce an unsigned ticket.
        "unsigned".to_owned()
    } else {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .unwrap_or_else(|e| pgrx::error!("HMAC init failed: {e}"));
        mac.update(canonical.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    };

    let ticket = serde_json::json!({
        "graph_iri":  graph_iri,
        "graph_id":   graph_id,
        "iat":        issued_at,
        "exp":        expiry,
        "aud":        "pg_ripple_http",
        "nonce":      nonce,
        "type":       "arrow_flight_v2",
        "sig":        sig
    });

    ticket.to_string().into_bytes()
}

/// Validate a signed Arrow Flight ticket payload.
///
/// Returns `Ok(graph_id)` when the ticket is valid.
/// Returns `Err(reason)` when validation fails.
/// Used by `pg_ripple_http` to verify tickets before streaming.
///
/// `allow_unsigned` should be `false` in production (FLIGHT-SEC-01).
#[allow(dead_code)]
pub fn validate_ticket(
    ticket: &serde_json::Value,
    secret: &str,
    now_secs: u64,
    allow_unsigned: bool,
) -> Result<i64, String> {
    // Type check.
    if ticket.get("type").and_then(|v| v.as_str()) != Some("arrow_flight_v2") {
        return Err("invalid ticket type (expected arrow_flight_v2)".to_owned());
    }
    // Audience check.
    if ticket.get("aud").and_then(|v| v.as_str()) != Some("pg_ripple_http") {
        return Err("invalid audience".to_owned());
    }
    // Expiry check.
    let exp = ticket
        .get("exp")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing exp field".to_owned())?;
    if now_secs > exp {
        return Err(format!("ticket expired at {exp}, now {now_secs}"));
    }
    // Signature check.
    let sig = ticket
        .get("sig")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing sig field".to_owned())?;

    if sig == "unsigned" {
        // FLIGHT-SEC-01: reject unsigned tickets in production mode.
        if !allow_unsigned {
            return Err(
                "unsigned Arrow Flight ticket rejected — set ARROW_UNSIGNED_TICKETS_ALLOWED=true \
                 for local development or configure a signing secret"
                    .to_owned(),
            );
        }
        // allow_unsigned = true: skip HMAC check for local development.
    } else if !secret.is_empty() {
        let graph_iri = ticket
            .get("graph_iri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing graph_iri".to_owned())?;
        let graph_id = ticket
            .get("graph_id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| "missing graph_id".to_owned())?;
        let iat = ticket
            .get("iat")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "missing iat".to_owned())?;
        let nonce = ticket
            .get("nonce")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing nonce".to_owned())?;

        let canonical = format!(
            "aud=pg_ripple_http,exp={exp},graph_id={graph_id},graph_iri={graph_iri},iat={iat},nonce={nonce},type=arrow_flight_v2"
        );
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| format!("HMAC init error: {e}"))?;
        mac.update(canonical.as_bytes());
        let expected = hex::encode(mac.finalize().into_bytes());
        if expected != sig {
            return Err("HMAC signature mismatch".to_owned());
        }
    }

    let graph_id = ticket
        .get("graph_id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "missing graph_id".to_owned())?;
    Ok(graph_id)
}

/// Entry point called from `export_arrow_flight` pg_extern (runs inside PG context).
pub fn export_arrow_flight_impl(graph_iri: &str) -> Vec<u8> {
    // Encode the graph IRI to its dictionary integer ID.
    let graph_id = if graph_iri.eq_ignore_ascii_case("default") || graph_iri == "0" {
        0i64
    } else {
        crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI)
    };

    // Read secret, expiry, and allow_unsigned from GUCs.
    let secret = crate::gucs::storage::ARROW_FLIGHT_SECRET
        .get()
        .as_ref()
        .and_then(|s| s.to_str().ok().map(str::to_owned))
        .unwrap_or_default();
    let expiry_secs = crate::gucs::storage::ARROW_FLIGHT_EXPIRY_SECS.get().max(1) as u64;
    let allow_unsigned = crate::gucs::storage::ARROW_UNSIGNED_TICKETS_ALLOWED.get();

    // Generate a nonce via PostgreSQL's gen_random_uuid() for replay prevention.
    let nonce: String = pgrx::Spi::get_one::<String>("SELECT gen_random_uuid()::text")
        .unwrap_or(None)
        .unwrap_or_else(uuid_fallback);

    build_signed_ticket(
        graph_iri,
        graph_id,
        &secret,
        expiry_secs,
        &nonce,
        allow_unsigned,
    )
}

/// Fallback nonce when SPI is unavailable (unit tests).
fn uuid_fallback() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(42);
    format!("fallback-nonce-{t}")
}

// ─── SQL API ─────────────────────────────────────────────────────────────────

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    /// Return a signed Arrow Flight ticket (BYTEA) for bulk export of a named graph.
    ///
    /// The ticket is HMAC-SHA256 signed using `pg_ripple.arrow_flight_secret`
    /// and expires after `pg_ripple.arrow_flight_expiry_secs` seconds.
    ///
    /// Present the returned ticket to `pg_ripple_http` at `POST /flight/do_get`
    /// to stream the graph as Arrow IPC record batches.
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
    fn test_signed_ticket_valid() {
        let ticket_bytes =
            build_signed_ticket("DEFAULT", 0, "test-secret", 3600, "test-nonce-1", false);
        let ticket: serde_json::Value = serde_json::from_slice(&ticket_bytes).unwrap();
        assert_eq!(ticket["graph_id"], 0i64);
        assert_eq!(ticket["type"], "arrow_flight_v2");
        assert_eq!(ticket["aud"], "pg_ripple_http");
        assert!(ticket["sig"].as_str().unwrap_or("") != "unsigned");
        assert!(ticket["sig"].as_str().unwrap_or("").len() == 64); // 32-byte HMAC hex
    }

    #[test]
    fn test_signed_ticket_validate_ok() {
        let ticket_bytes =
            build_signed_ticket("DEFAULT", 0, "test-secret", 3600, "test-nonce-2", false);
        let ticket: serde_json::Value = serde_json::from_slice(&ticket_bytes).unwrap();
        let iat = ticket["iat"].as_u64().unwrap_or(0);
        let result = validate_ticket(&ticket, "test-secret", iat + 1, false);
        assert!(result.is_ok(), "validation should succeed: {:?}", result);
        assert_eq!(result.unwrap(), 0i64);
    }

    #[test]
    fn test_signed_ticket_expired() {
        let ticket_bytes =
            build_signed_ticket("DEFAULT", 0, "test-secret", 1, "test-nonce-3", false);
        let ticket: serde_json::Value = serde_json::from_slice(&ticket_bytes).unwrap();
        let exp = ticket["exp"].as_u64().unwrap_or(0);
        let result = validate_ticket(&ticket, "test-secret", exp + 100, false);
        assert!(result.is_err(), "should reject expired ticket");
        assert!(result.unwrap_err().contains("expired"));
    }

    #[test]
    fn test_signed_ticket_wrong_secret() {
        let ticket_bytes =
            build_signed_ticket("DEFAULT", 0, "correct-secret", 3600, "test-nonce-4", false);
        let ticket: serde_json::Value = serde_json::from_slice(&ticket_bytes).unwrap();
        let iat = ticket["iat"].as_u64().unwrap_or(0);
        let result = validate_ticket(&ticket, "wrong-secret", iat + 1, false);
        assert!(result.is_err(), "should reject tampered signature");
        assert!(result.unwrap_err().contains("HMAC"));
    }

    #[test]
    fn test_unsigned_ticket_allowed() {
        // Empty secret + allow_unsigned = true produces unsigned ticket.
        let ticket_bytes = build_signed_ticket("DEFAULT", 0, "", 3600, "test-nonce-5", true);
        let ticket: serde_json::Value = serde_json::from_slice(&ticket_bytes).unwrap();
        assert_eq!(ticket["sig"], "unsigned");
        // Validate with allow_unsigned = true passes.
        let iat = ticket["iat"].as_u64().unwrap_or(0);
        let result = validate_ticket(&ticket, "any-secret", iat + 1, true);
        assert!(
            result.is_ok(),
            "unsigned ticket should be allowed in dev mode"
        );
    }

    #[test]
    fn test_unsigned_ticket_rejected_in_production() {
        // Build an unsigned ticket with allow_unsigned = true (dev mode).
        let ticket_bytes = build_signed_ticket("DEFAULT", 0, "", 3600, "test-nonce-6", true);
        let ticket: serde_json::Value = serde_json::from_slice(&ticket_bytes).unwrap();
        assert_eq!(ticket["sig"], "unsigned");
        // But validate with allow_unsigned = false (production mode) should reject.
        let iat = ticket["iat"].as_u64().unwrap_or(0);
        let result = validate_ticket(&ticket, "any-secret", iat + 1, false);
        assert!(
            result.is_err(),
            "unsigned ticket must be rejected in production mode"
        );
        assert!(result.unwrap_err().contains("unsigned"));
    }
}
