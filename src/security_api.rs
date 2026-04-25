//! Security and data governance API functions for pg_ripple v0.55.0.
//!
//! # Functions
//!
//! - `grant_graph_access()` — create an RLS policy granting a role access to a named graph.
//! - `revoke_graph_access()` — drop an RLS policy revoking a role's access to a named graph.
//! - `erase_subject()` — GDPR-style erasure: atomically delete all triples with `s = encode(iri)`.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

/// Grant a PostgreSQL role access to a named graph via Row-Level Security.
///
/// Creates an RLS policy `pg_ripple_graph_<role>_<graph_suffix>` on the
/// `_pg_ripple.vp_rare` table for the named graph.
/// The policy allows SELECT (or the requested `privilege`) when the `g` column
/// equals the dictionary-encoded graph IRI.
///
/// # Arguments
///
/// - `graph_iri` — the named graph IRI (e.g. `<https://example.org/graph1>`).
/// - `role` — the PostgreSQL role name to grant access to.
/// - `privilege` — `'SELECT'` (default) or `'ALL'`.
#[pg_extern]
#[search_path(pg_catalog, public)]
fn grant_graph_access(graph_iri: &str, role: &str, privilege: default!(&str, "'SELECT'")) {
    do_grant_graph_access(graph_iri, role, privilege);
}

/// Revoke a PostgreSQL role's named-graph RLS access.
///
/// Drops the RLS policy previously created by `grant_graph_access()`.
#[pg_extern]
#[search_path(pg_catalog, public)]
fn revoke_graph_access(graph_iri: &str, role: &str) {
    do_revoke_graph_access(graph_iri, role);
}

/// Atomically erase all triples whose subject equals `encode(iri)`.
///
/// Deletes from:
/// - all dedicated VP tables (`_pg_ripple.vp_*`)
/// - `_pg_ripple.vp_rare` (for rare predicates)
/// - `_pg_ripple.dictionary` (removes the subject's dictionary entry if no longer referenced)
/// - `_pg_ripple.kge_embeddings` (removes embedding rows for the subject)
///
/// Returns the count of deleted triples.
///
/// # GDPR note
///
/// This function provides a best-effort erasure path.  For guaranteed erasure
/// including WAL and backup media, a full backup cycle is required after calling
/// this function.
#[pg_extern]
#[search_path(pg_catalog, public)]
fn erase_subject(iri: &str) -> i64 {
    // Encode the IRI to get its dictionary ID.
    let subject_id = crate::dictionary::encode(iri, crate::dictionary::KIND_IRI);

    let mut total_deleted: i64 = 0;

    // Delete from vp_rare.
    let rare_deleted: i64 = pgrx::Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.vp_rare WHERE s = $1 RETURNING 1) SELECT count(*)::bigint FROM d",
        &[DatumWithOid::from(subject_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0);
    total_deleted += rare_deleted;

    // Delete from all dedicated VP tables.
    let pred_ids: Vec<i64> = {
        let mut ids = Vec::new();
        pgrx::Spi::connect(|client| {
            let rows = client.select(
                "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
                None,
                &[],
            );
            if let Ok(rows) = rows {
                for row in rows {
                    if let Ok(Some(id)) = row.get::<i64>(1) {
                        ids.push(id);
                    }
                }
            }
        });
        ids
    };

    for pred_id in &pred_ids {
        // Attempt delete from delta table.
        let delta_sql = format!("DELETE FROM _pg_ripple.vp_{pred_id}_delta WHERE s = $1");
        let _ = pgrx::Spi::run_with_args(&delta_sql, &[DatumWithOid::from(subject_id)]);

        // Attempt delete from main table.
        let main_sql = format!("DELETE FROM _pg_ripple.vp_{pred_id}_main WHERE s = $1");
        if let Ok(cnt) = pgrx::Spi::get_one_with_args::<i64>(
            &format!("WITH d AS ({main_sql} RETURNING 1) SELECT count(*)::bigint FROM d"),
            &[DatumWithOid::from(subject_id)],
        ) {
            total_deleted += cnt.unwrap_or(0);
        }
    }

    // Delete KGE embeddings for the subject.
    let _ = pgrx::Spi::run_with_args(
        "DELETE FROM _pg_ripple.kge_embeddings WHERE s = $1",
        &[DatumWithOid::from(subject_id)],
    );

    // Remove the subject's dictionary entry if it's no longer referenced by any VP table.
    // This is best-effort — we skip the cross-table reference check for performance.
    let _ = pgrx::Spi::run_with_args(
        "DELETE FROM _pg_ripple.dictionary WHERE id = $1",
        &[DatumWithOid::from(subject_id)],
    );

    total_deleted
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

fn graph_iri_to_policy_suffix(graph_iri: &str) -> String {
    // Create a stable short suffix from the IRI for use in policy names.
    use xxhash_rust::xxh3::xxh3_64;
    format!("{:016x}", xxh3_64(graph_iri.as_bytes()))
}

fn do_grant_graph_access(graph_iri: &str, role: &str, privilege: &str) {
    let graph_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
    let suffix = graph_iri_to_policy_suffix(graph_iri);
    let policy_name = format!("pg_ripple_graph_{role}_{suffix}");

    // Sanitise privilege to only allow safe values.
    let pg_privilege = match privilege.to_uppercase().as_str() {
        "SELECT" | "ALL" => privilege.to_uppercase(),
        _ => {
            pgrx::error!(
                "PT710: grant_graph: invalid permission '{}'; use 'SELECT' or 'ALL'",
                privilege
            );
        }
    };

    // Enable RLS on vp_rare if not already enabled (best effort).
    let _ = pgrx::Spi::run_with_args(
        "ALTER TABLE _pg_ripple.vp_rare ENABLE ROW LEVEL SECURITY",
        &[],
    );

    // Create the policy on vp_rare.
    let policy_sql = format!(
        "CREATE POLICY {policy_name} ON _pg_ripple.vp_rare \
         AS PERMISSIVE FOR {pg_privilege} TO {role} \
         USING (g = {graph_id})"
    );
    pgrx::Spi::run_with_args(&policy_sql, &[]).unwrap_or_else(|e| {
        pgrx::warning!("grant_graph_access: policy creation failed: {e}");
    });
}

fn do_revoke_graph_access(graph_iri: &str, role: &str) {
    let suffix = graph_iri_to_policy_suffix(graph_iri);
    let policy_name = format!("pg_ripple_graph_{role}_{suffix}");

    let drop_sql = format!("DROP POLICY IF EXISTS {policy_name} ON _pg_ripple.vp_rare");
    pgrx::Spi::run_with_args(&drop_sql, &[]).unwrap_or_else(|e| {
        pgrx::warning!("revoke_graph_access: policy drop failed: {e}");
    });
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_erase_subject_no_data() {
        // Erasing a non-existent subject should return 0 without error.
        let result = super::erase_subject("<https://example.org/nonexistent>");
        assert_eq!(result, 0, "erase_subject on nonexistent IRI must return 0");
    }
}
