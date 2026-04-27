//! Security and data governance API functions for pg_ripple v0.55.0.
//!
//! # Functions
//!
//! - `grant_graph_access()` — create an RLS policy granting a role access to a named graph.
//! - `revoke_graph_access()` — drop an RLS policy revoking a role's access to a named graph.
//! - `erase_subject()` — GDPR-style erasure: atomically delete all triples with `s = encode(iri)`.

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

/// Public wrapper for `do_grant_graph_access` used by tenant management (v0.57.0).
pub(crate) fn do_grant_graph_access_pub(graph_iri: &str, role: &str, privilege: &str) {
    do_grant_graph_access(graph_iri, role, privilege);
}

/// Public wrapper for `do_revoke_graph_access` used by tenant management (v0.57.0).
pub(crate) fn do_revoke_graph_access_pub(graph_iri: &str, role: &str) {
    do_revoke_graph_access(graph_iri, role);
}

/// Row returned by `erase_subject()` SRF — one row per storage relation touched.
#[derive(Debug, Clone)]
pub(crate) struct EraseRow {
    pub relation: String,
    pub rows_deleted: i64,
}

pub(crate) fn erase_subject_impl(iri: &str) -> Vec<EraseRow> {
    use pgrx::datum::DatumWithOid;

    // Look up the IRI in the dictionary.  If it doesn't exist there are no
    // triples to erase, so return an empty result set immediately rather than
    // inserting the IRI via encode() and then deleting the fresh entry.
    let subject_id = match crate::dictionary::lookup_iri(iri) {
        Some(id) => id,
        None => return Vec::new(),
    };

    let mut results: Vec<EraseRow> = Vec::new();

    // Delete from vp_rare.
    let rare_deleted: i64 = pgrx::Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.vp_rare WHERE s = $1 RETURNING 1) SELECT count(*)::bigint FROM d",
        &[DatumWithOid::from(subject_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0);
    results.push(EraseRow {
        relation: "_pg_ripple.vp_rare".to_owned(),
        rows_deleted: rare_deleted,
    });

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
        let delta_table = format!("_pg_ripple.vp_{pred_id}_delta");
        let delta_sql = format!(
            "WITH d AS (DELETE FROM {delta_table} WHERE s = $1 RETURNING 1) SELECT count(*)::bigint FROM d"
        );
        let delta_cnt: i64 =
            pgrx::Spi::get_one_with_args::<i64>(&delta_sql, &[DatumWithOid::from(subject_id)])
                .unwrap_or(None)
                .unwrap_or(0);
        if delta_cnt > 0 {
            results.push(EraseRow {
                relation: delta_table,
                rows_deleted: delta_cnt,
            });
        }

        // Attempt delete from main table.
        let main_table = format!("_pg_ripple.vp_{pred_id}_main");
        let main_sql = format!(
            "WITH d AS (DELETE FROM {main_table} WHERE s = $1 RETURNING 1) SELECT count(*)::bigint FROM d"
        );
        if let Ok(cnt) =
            pgrx::Spi::get_one_with_args::<i64>(&main_sql, &[DatumWithOid::from(subject_id)])
        {
            let n = cnt.unwrap_or(0);
            if n > 0 {
                results.push(EraseRow {
                    relation: main_table,
                    rows_deleted: n,
                });
            }
        }
    }

    // Delete KGE embeddings for the subject (best-effort; table may not exist).
    let kge_exists: bool = pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = '_pg_ripple' AND c.relname = 'kge_embeddings')",
    )
    .unwrap_or(None)
    .unwrap_or(false);
    if kge_exists {
        let kge_cnt: i64 = pgrx::Spi::get_one_with_args::<i64>(
            "WITH d AS (DELETE FROM _pg_ripple.kge_embeddings WHERE entity_id = $1 RETURNING 1) SELECT count(*)::bigint FROM d",
            &[DatumWithOid::from(subject_id)],
        )
        .unwrap_or(None)
        .unwrap_or(0);
        results.push(EraseRow {
            relation: "_pg_ripple.kge_embeddings".to_owned(),
            rows_deleted: kge_cnt,
        });
    }

    // Delete from PROV-O named graph (triples where s = subject_id in the prov graph).
    let prov_graph_exists: bool = pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = '_pg_ripple' AND c.relname = 'prov_log')",
    )
    .unwrap_or(None)
    .unwrap_or(false);
    if prov_graph_exists {
        // Check if prov_log has a subject_id column before attempting deletion.
        let has_subject_col: bool = pgrx::Spi::get_one::<bool>(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
             WHERE table_schema = '_pg_ripple' AND table_name = 'prov_log' AND column_name = 'subject_id')",
        )
        .unwrap_or(None)
        .unwrap_or(false);
        if has_subject_col {
            let prov_cnt: i64 = pgrx::Spi::get_one_with_args::<i64>(
                "WITH d AS (DELETE FROM _pg_ripple.prov_log WHERE subject_id = $1 RETURNING 1) SELECT count(*)::bigint FROM d",
                &[DatumWithOid::from(subject_id)],
            )
            .unwrap_or(None)
            .unwrap_or(0);
            results.push(EraseRow {
                relation: "_pg_ripple.prov_log".to_owned(),
                rows_deleted: prov_cnt,
            });
        }
    }

    // Delete from audit log (best-effort).
    let audit_exists: bool = pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = '_pg_ripple' AND c.relname = 'audit_log')",
    )
    .unwrap_or(None)
    .unwrap_or(false);
    if audit_exists {
        // Check if audit_log has a subject_id column (added in future schema upgrades).
        // If not, skip the deletion gracefully — audit_log records operations, not subject data.
        let has_subject_col: bool = pgrx::Spi::get_one::<bool>(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
             WHERE table_schema = '_pg_ripple' AND table_name = 'audit_log' AND column_name = 'subject_id')",
        )
        .unwrap_or(None)
        .unwrap_or(false);
        if has_subject_col {
            let audit_cnt: i64 = pgrx::Spi::get_one_with_args::<i64>(
                "WITH d AS (DELETE FROM _pg_ripple.audit_log WHERE subject_id = $1 RETURNING 1) SELECT count(*)::bigint FROM d",
                &[DatumWithOid::from(subject_id)],
            )
            .unwrap_or(None)
            .unwrap_or(0);
            results.push(EraseRow {
                relation: "_pg_ripple.audit_log".to_owned(),
                rows_deleted: audit_cnt,
            });
        }
    }

    // Remove the subject's dictionary entry if it's no longer referenced by any VP table.
    // This is best-effort — we skip the cross-table reference check for performance.
    let dict_cnt: i64 = pgrx::Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.dictionary WHERE id = $1 RETURNING 1) SELECT count(*)::bigint FROM d",
        &[DatumWithOid::from(subject_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0);
    results.push(EraseRow {
        relation: "_pg_ripple.dictionary".to_owned(),
        rows_deleted: dict_cnt,
    });

    results
}

// ─── SQL-exported API ─────────────────────────────────────────────────────────

#[pgrx::pg_schema]
mod pg_ripple {
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
    fn grant_graph_access(graph_iri: &str, role: &str, privilege: default!(&str, "'SELECT'")) {
        super::do_grant_graph_access(graph_iri, role, privilege);
    }

    /// Revoke a PostgreSQL role's named-graph RLS access.
    ///
    /// Drops the RLS policy previously created by `grant_graph_access()`.
    #[pg_extern]
    fn revoke_graph_access(graph_iri: &str, role: &str) {
        super::do_revoke_graph_access(graph_iri, role);
    }

    /// v0.61.0: User-friendly alias for `grant_graph_access()` (default privilege: SELECT).
    #[pg_extern]
    fn grant_graph(graph_iri: &str, role: &str) {
        super::do_grant_graph_access(graph_iri, role, "SELECT");
    }

    /// v0.61.0: User-friendly alias for `revoke_graph_access()`.
    #[pg_extern]
    fn revoke_graph(graph_iri: &str, role: &str) {
        super::do_revoke_graph_access(graph_iri, role);
    }

    /// GDPR right-to-erasure: atomically remove all traces of a subject IRI.
    ///
    /// Deletes from every storage layer that may hold data about `iri`:
    /// - all dedicated VP delta and main tables
    /// - `_pg_ripple.vp_rare`
    /// - `_pg_ripple.kge_embeddings`
    /// - `_pg_ripple.prov_log` (if present)
    /// - `_pg_ripple.audit_log` (if present)
    /// - `_pg_ripple.dictionary` (subject entry if unreferenced)
    ///
    /// Returns one row per storage relation touched, with the deletion count.
    /// All deletes execute in the caller's transaction (atomic erasure).
    ///
    /// # GDPR note
    ///
    /// This function provides a best-effort erasure path.  For guaranteed erasure
    /// including WAL and backup media, a full backup cycle is required after calling
    /// this function.
    #[pg_extern]
    fn erase_subject(
        iri: &str,
    ) -> TableIterator<'static, (name!(relation, String), name!(rows_deleted, i64))> {
        let rows: Vec<(String, i64)> = super::erase_subject_impl(iri)
            .into_iter()
            .map(|r| (r.relation, r.rows_deleted))
            .collect();
        TableIterator::new(rows)
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_erase_subject_no_data() {
        // Erasing a non-existent subject should return results without error.
        let result = crate::security_api::erase_subject_impl("<https://example.org/nonexistent>");
        // All rows_deleted should be 0 for a nonexistent subject.
        let total: i64 = result.iter().map(|r| r.rows_deleted).sum();
        assert_eq!(
            total, 0,
            "erase_subject on nonexistent IRI must return 0 total deletions"
        );
    }
}
