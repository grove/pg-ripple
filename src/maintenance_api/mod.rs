//! pg_ripple SQL API — Administrative functions, Graph-level RLS, Schema summary

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    include!("vacuum.rs");
    include!("stats.rs");
    include!("migrate.rs");

    // ── Graph-level Row-Level Security (v0.14.0) ─────────────────────────────

    /// Enable graph-level Row-Level Security on the current database.
    ///
    /// Creates RLS policies on `_pg_ripple.vp_rare` using the `g` column and
    /// the `_pg_ripple.graph_access` mapping table.  Dedicated VP tables
    /// created after this call also receive RLS policies.
    ///
    /// Set `pg_ripple.rls_bypass = on` in a superuser session to bypass all
    /// policies.  Default graph (g = 0) is always accessible.
    ///
    /// Returns `true` on success.
    #[pg_extern]
    fn enable_graph_rls() -> bool {
        // Enable RLS on vp_rare — the consolidation table always exists.
        pgrx::Spi::run(
            "ALTER TABLE _pg_ripple.vp_rare ENABLE ROW LEVEL SECURITY; \
             DROP POLICY IF EXISTS pg_ripple_rls_read ON _pg_ripple.vp_rare; \
             CREATE POLICY pg_ripple_rls_read ON _pg_ripple.vp_rare \
                 AS PERMISSIVE FOR SELECT \
                 TO PUBLIC \
                 USING ( \
                     g = 0 \
                     OR current_setting('pg_ripple.rls_bypass', true) = 'on' \
                     OR EXISTS ( \
                         SELECT 1 FROM _pg_ripple.graph_access ga \
                         WHERE ga.role_name = current_user \
                           AND ga.graph_id  = vp_rare.g \
                           AND ga.permission IN ('read', 'write', 'admin') \
                     ) \
                 ); \
             DROP POLICY IF EXISTS pg_ripple_rls_write ON _pg_ripple.vp_rare; \
             CREATE POLICY pg_ripple_rls_write ON _pg_ripple.vp_rare \
                 AS PERMISSIVE FOR ALL \
                 TO PUBLIC \
                 USING ( \
                     g = 0 \
                     OR current_setting('pg_ripple.rls_bypass', true) = 'on' \
                     OR EXISTS ( \
                         SELECT 1 FROM _pg_ripple.graph_access ga \
                         WHERE ga.role_name = current_user \
                           AND ga.graph_id  = vp_rare.g \
                           AND ga.permission IN ('write', 'admin') \
                     ) \
                 )",
        )
        .unwrap_or_else(|e| pgrx::error!("enable_graph_rls: error creating policy: {e}"));

        // Record that RLS is enabled in the predicates catalog metadata.
        pgrx::Spi::run(
            "INSERT INTO _pg_ripple.graph_access (role_name, graph_id, permission) \
             VALUES ('__rls_enabled__', -1, 'admin') \
             ON CONFLICT DO NOTHING",
        )
        .unwrap_or_else(|e| pgrx::warning!("enable_graph_rls: catalog write warning: {e}"));

        true
    }

    /// Grant a permission on a named graph to a PostgreSQL role.
    ///
    /// `permission` must be `'read'`, `'write'`, or `'admin'`.
    /// The graph IRI is encoded in the dictionary automatically.
    /// Granting `'admin'` implies read and write.
    ///
    /// Note: renamed from `grant_graph` to `grant_graph_permission` in v0.61.0
    /// to avoid a symbol conflict with the new RLS-based `grant_graph()` in
    /// `pg_ripple.security_api` (`grant_graph(graph_iri, role)`).
    #[pg_extern]
    fn grant_graph_permission(role: &str, graph: &str, permission: &str) {
        let valid = matches!(permission, "read" | "write" | "admin");
        if !valid {
            pgrx::error!(
                "grant_graph_permission: permission must be 'read', 'write', or 'admin'; got '{permission}'"
            );
        }

        let graph_id = crate::dictionary::encode(
            crate::storage::strip_angle_brackets_pub(graph),
            crate::dictionary::KIND_IRI,
        );

        pgrx::Spi::run_with_args(
            "INSERT INTO _pg_ripple.graph_access (role_name, graph_id, permission) \
             VALUES ($1, $2, $3) \
             ON CONFLICT DO NOTHING",
            &[
                pgrx::datum::DatumWithOid::from(role),
                pgrx::datum::DatumWithOid::from(graph_id),
                pgrx::datum::DatumWithOid::from(permission),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("grant_graph_permission: insert error: {e}"));
    }

    /// Revoke a permission on a named graph from a PostgreSQL role.
    ///
    /// Pass NULL for `permission` to revoke all permissions for the role on that graph.
    ///
    /// Note: renamed from `revoke_graph` to `revoke_graph_permission` in v0.61.0
    /// to avoid a symbol conflict with the new RLS-based `revoke_graph()` in
    /// `pg_ripple.security_api` (`revoke_graph(graph_iri, role)`).
    #[pg_extern]
    fn revoke_graph_permission(
        role: &str,
        graph: &str,
        permission: default!(Option<&str>, "NULL"),
    ) {
        let graph_id = crate::dictionary::encode(
            crate::storage::strip_angle_brackets_pub(graph),
            crate::dictionary::KIND_IRI,
        );

        if let Some(perm) = permission {
            pgrx::Spi::run_with_args(
                "DELETE FROM _pg_ripple.graph_access \
                 WHERE role_name = $1 AND graph_id = $2 AND permission = $3",
                &[
                    pgrx::datum::DatumWithOid::from(role),
                    pgrx::datum::DatumWithOid::from(graph_id),
                    pgrx::datum::DatumWithOid::from(perm),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("revoke_graph_permission: delete error: {e}"));
        } else {
            pgrx::Spi::run_with_args(
                "DELETE FROM _pg_ripple.graph_access \
                 WHERE role_name = $1 AND graph_id = $2",
                &[
                    pgrx::datum::DatumWithOid::from(role),
                    pgrx::datum::DatumWithOid::from(graph_id),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("revoke_graph_permission: delete error: {e}"));
        }
    }

    /// List all graph access control entries as JSONB.
    ///
    /// Returns one row per (role, graph, permission) entry with decoded graph IRIs.
    #[pg_extern]
    fn list_graph_access() -> pgrx::JsonB {
        let rows: Vec<serde_json::Value> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT ga.role_name, d.value AS graph_iri, ga.permission \
                 FROM _pg_ripple.graph_access ga \
                 LEFT JOIN _pg_ripple.dictionary d ON d.id = ga.graph_id \
                 WHERE ga.role_name <> '__rls_enabled__' \
                 ORDER BY ga.role_name, ga.graph_id",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("list_graph_access: SPI error: {e}"))
            .map(|row| {
                let role: String = row.get::<String>(1).ok().flatten().unwrap_or_default();
                let graph_iri: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                let perm: String = row.get::<String>(3).ok().flatten().unwrap_or_default();
                serde_json::json!({
                    "role": role,
                    "graph": graph_iri,
                    "permission": perm
                })
            })
            .collect()
        });
        pgrx::JsonB(serde_json::Value::Array(rows))
    }

    // ── Schema summary (v0.14.0, optional pg_trickle) ────────────────────────

    /// Enable the live schema summary stream table via pg_trickle.
    ///
    /// Creates `_pg_ripple.inferred_schema` as a pg_trickle stream table that
    /// maintains a live class→property→cardinality summary.  Used by tooling
    /// and SPARQL IDE auto-completion.
    ///
    /// Returns `true` if the stream table was created; `false` if pg_trickle
    /// is not installed (no error is raised).
    #[pg_extern]
    fn enable_schema_summary() -> bool {
        if !crate::has_pg_trickle() {
            pgrx::warning!(
                "pg_trickle is not installed; schema summary is unavailable. \
                 Install pg_trickle and run SELECT pg_ripple.enable_schema_summary() to enable."
            );
            return false;
        }

        // The schema summary groups triples by predicate to give a rough
        // class→property→cardinality overview.  We use rdf:type as the
        // class link; predicates become properties; COUNT becomes cardinality.
        let rdf_type_id = crate::dictionary::encode(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            crate::dictionary::KIND_IRI,
        );

        let summary_sql = format!(
            "SELECT \
                 COALESCE(dc.value, 'unknown') AS class_iri, \
                 dp.value                       AS property_iri, \
                 COUNT(*)::bigint               AS cardinality \
             FROM _pg_ripple.vp_rare vr \
             JOIN _pg_ripple.vp_rare type_row \
                 ON type_row.s = vr.s \
                AND type_row.p = {rdf_type_id} \
             JOIN _pg_ripple.dictionary dp ON dp.id = vr.p \
             LEFT JOIN _pg_ripple.dictionary dc ON dc.id = type_row.o \
             WHERE vr.p <> {rdf_type_id} \
             GROUP BY 1, 2"
        );

        pgrx::Spi::run_with_args(
            "SELECT pg_trickle.create_stream_table($1, $2, '30s')",
            &[
                pgrx::datum::DatumWithOid::from("_pg_ripple.inferred_schema"),
                pgrx::datum::DatumWithOid::from(summary_sql.as_str()),
            ],
        )
        .unwrap_or_else(|e| {
            pgrx::warning!(
                "failed to create _pg_ripple.inferred_schema stream table: {}",
                e
            );
        });

        true
    }

    /// Return the live schema summary as JSONB.
    ///
    /// Reads from `_pg_ripple.inferred_schema` if available (requires
    /// `enable_schema_summary()` to have been called), otherwise falls back
    /// to a direct scan.  Returns an array of `{class, property, cardinality}`.
    #[pg_extern]
    fn schema_summary() -> pgrx::JsonB {
        let has_stream_table = pgrx::Spi::get_one::<bool>(
            "SELECT EXISTS( \
                 SELECT 1 FROM pg_class c \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = '_pg_ripple' AND c.relname = 'inferred_schema' \
             )",
        )
        .unwrap_or(None)
        .unwrap_or(false);

        let query = if has_stream_table {
            "SELECT class_iri, property_iri, cardinality \
             FROM _pg_ripple.inferred_schema \
             ORDER BY class_iri, property_iri"
        } else {
            "SELECT \
                 COALESCE(dc.value, 'unknown') AS class_iri, \
                 dp.value                       AS property_iri, \
                 COUNT(*)::bigint               AS cardinality \
             FROM _pg_ripple.predicates p \
             JOIN _pg_ripple.dictionary dp ON dp.id = p.id \
             CROSS JOIN LATERAL (SELECT 1 LIMIT 0) AS dummy(x) \
             GROUP BY 1, 2 \
             ORDER BY 1, 2 \
             LIMIT 0"
        };

        let rows: Vec<serde_json::Value> = pgrx::Spi::connect(|c| {
            c.select(query, None, &[])
                .unwrap_or_else(|e| pgrx::error!("schema_summary: SPI error: {e}"))
                .map(|row| {
                    let class: String = row.get::<String>(1).ok().flatten().unwrap_or_default();
                    let prop: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                    let card: i64 = row.get::<i64>(3).ok().flatten().unwrap_or(0);
                    serde_json::json!({
                        "class": class,
                        "property": prop,
                        "cardinality": card
                    })
                })
                .collect()
        });
        pgrx::JsonB(serde_json::Value::Array(rows))
    }

    /// Return build metadata as a JSON object.
    ///
    /// Fields: `version`, `profile` (`"debug"` or `"release"`), `built` (RFC-3339
    /// timestamp or `"unknown"`), `git_sha` (short SHA or `"unknown"`).
    ///
    /// All values are compile-time constants — zero runtime overhead.
    ///
    /// ```sql
    /// SELECT pg_ripple.build_info();
    /// -- {"version":"0.99.1","profile":"release","built":"2026-05-07T12:00:00Z","git_sha":"a1b2c3d"}
    /// ```
    #[pg_extern]
    fn build_info() -> pgrx::JsonB {
        const BUILD_TIME: &str = match option_env!("BUILD_TIMESTAMP") {
            Some(ts) => ts,
            None => "unknown",
        };
        const GIT_SHA: &str = match option_env!("GIT_SHA") {
            Some(sha) => sha,
            None => "unknown",
        };
        const BUILD_PROFILE: &str = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let json = serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "profile": BUILD_PROFILE,
            "built": BUILD_TIME,
            "git_sha": GIT_SHA,
        });
        pgrx::JsonB(json)
    }
}
