//! Multi-tenant named-graph isolation for pg_ripple v0.57.0.
//!
//! Provides tenant management functions that layer on top of the existing
//! `grant_graph_access()` / `revoke_graph_access()` RLS infrastructure
//! from v0.55.0.
//!
//! # Functions
//!
//! - `create_tenant(name, graph_iri, quota_triples)` — create a PostgreSQL role
//!   with RLS access to a named graph and optional triple quota enforcement.
//! - `drop_tenant(name)` — revoke access and remove the trigger.
//! - `tenant_stats()` — SRF returning per-tenant stats.

use pgrx::prelude::*;

// ─── create_tenant ────────────────────────────────────────────────────────────

/// Create a new pg_ripple tenant.
///
/// Creates a PostgreSQL role `pg_ripple_tenant_{tenant_name}`, maps it to
/// a named graph via `grant_graph_access()`, creates a row in the
/// `_pg_ripple.tenants` catalog table, and installs a quota-enforcing trigger
/// if `quota_triples > 0`.
///
/// Requires superuser privileges.
#[pg_extern(schema = "pg_ripple", name = "create_tenant")]
pub fn create_tenant(tenant_name: &str, graph_iri: &str, quota_triples: default!(i64, "0")) {
    // Validate tenant name: only lowercase alphanumeric + underscore.
    if !tenant_name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        || tenant_name.is_empty()
    {
        pgrx::error!(
            "PT701: create_tenant: invalid tenant name '{}'; \
             use only lowercase letters, digits, and underscores",
            tenant_name
        );
    }

    if graph_iri.is_empty() {
        pgrx::error!("PT702: create_tenant: graph_iri must not be empty");
    }

    let role_name = format!("pg_ripple_tenant_{tenant_name}");

    // Create the role if it doesn't exist.
    let create_role_sql = format!(
        "DO $$ BEGIN \
           IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{role_name}') THEN \
             CREATE ROLE {role_name} NOLOGIN; \
           END IF; \
         END $$"
    );
    Spi::run_with_args(&create_role_sql, &[]).unwrap_or_else(|e| {
        pgrx::error!("PT703: create_tenant: failed to create role {role_name}: {e}");
    });

    // Grant graph access via the existing security API.
    crate::security_api::do_grant_graph_access_pub(graph_iri, &role_name, "ALL");

    // Register in tenants catalog.
    let _ = Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.tenants ( \
           tenant_name TEXT PRIMARY KEY, \
           graph_iri TEXT NOT NULL, \
           quota_triples BIGINT NOT NULL DEFAULT 0, \
           created_at TIMESTAMPTZ NOT NULL DEFAULT now() \
         )",
        &[],
    );

    Spi::run_with_args(
        "INSERT INTO _pg_ripple.tenants (tenant_name, graph_iri, quota_triples) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (tenant_name) DO UPDATE SET \
           graph_iri = EXCLUDED.graph_iri, \
           quota_triples = EXCLUDED.quota_triples",
        &[
            pgrx::datum::DatumWithOid::from(tenant_name),
            pgrx::datum::DatumWithOid::from(graph_iri),
            pgrx::datum::DatumWithOid::from(quota_triples),
        ],
    )
    .unwrap_or_else(|e| {
        pgrx::warning!("create_tenant: catalog insert failed: {e}");
    });

    // Install quota trigger if quota_triples > 0.
    if quota_triples > 0 {
        install_quota_trigger(tenant_name, graph_iri, quota_triples);
    }
}

// ─── drop_tenant ──────────────────────────────────────────────────────────────

/// Drop a pg_ripple tenant: revoke access, remove trigger, delete catalog row.
///
/// Does NOT drop the PostgreSQL role (use `DROP ROLE` manually if desired).
#[pg_extern(schema = "pg_ripple", name = "drop_tenant")]
pub fn drop_tenant(tenant_name: &str) {
    if tenant_name.is_empty() {
        pgrx::error!("PT704: drop_tenant: tenant_name must not be empty");
    }

    let role_name = format!("pg_ripple_tenant_{tenant_name}");

    // Look up the graph IRI for this tenant.
    let graph_iri: Option<String> = Spi::get_one_with_args::<String>(
        "SELECT graph_iri FROM _pg_ripple.tenants WHERE tenant_name = $1",
        &[pgrx::datum::DatumWithOid::from(tenant_name)],
    )
    .unwrap_or(None);

    if let Some(iri) = graph_iri {
        crate::security_api::do_revoke_graph_access_pub(&iri, &role_name);
    }

    // Remove quota trigger.
    let trigger_name = format!("pg_ripple_quota_{tenant_name}");
    let drop_trigger_sql = format!("DROP TRIGGER IF EXISTS {trigger_name} ON _pg_ripple.vp_rare");
    let _ = Spi::run_with_args(&drop_trigger_sql, &[]);

    // Remove from tenants catalog.
    let _ = Spi::run_with_args(
        "DELETE FROM _pg_ripple.tenants WHERE tenant_name = $1",
        &[pgrx::datum::DatumWithOid::from(tenant_name)],
    );
}

// ─── tenant_stats ─────────────────────────────────────────────────────────────

/// Return per-tenant statistics.
///
/// Returns a table of (tenant_name, graph_iri, triple_count, quota).
#[pg_extern(schema = "pg_ripple", name = "tenant_stats")]
pub fn tenant_stats() -> TableIterator<
    'static,
    (
        name!(tenant_name, String),
        name!(graph_iri, String),
        name!(triple_count, i64),
        name!(quota, i64),
    ),
> {
    let rows = Spi::connect(|client| {
        let tenant_rows = client.select(
            "SELECT tenant_name, graph_iri, quota_triples \
             FROM _pg_ripple.tenants \
             ORDER BY tenant_name",
            None,
            &[],
        )?;

        let mut results = Vec::new();
        for row in tenant_rows {
            let name = row.get::<String>(1)?.unwrap_or_default();
            let iri = row.get::<String>(2)?.unwrap_or_default();
            let quota = row.get::<i64>(3)?.unwrap_or(0);

            // Count triples for this graph.
            let graph_id = crate::dictionary::encode(&iri, crate::dictionary::KIND_IRI);
            let count: i64 = Spi::get_one_with_args::<i64>(
                "SELECT count(*)::bigint FROM _pg_ripple.vp_rare WHERE g = $1",
                &[pgrx::datum::DatumWithOid::from(graph_id)],
            )
            .unwrap_or(None)
            .unwrap_or(0);

            results.push((name, iri, count, quota));
        }
        Ok::<_, pgrx::spi::Error>(results)
    })
    .unwrap_or_default();

    TableIterator::new(rows)
}

// ─── Quota trigger helper ─────────────────────────────────────────────────────

fn install_quota_trigger(tenant_name: &str, graph_iri: &str, quota_triples: i64) {
    let graph_id = crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI);
    let trigger_name = format!("pg_ripple_quota_{tenant_name}");
    let fn_name = format!("pg_ripple_quota_check_{tenant_name}");

    // Create the trigger function.
    let fn_sql = format!(
        "CREATE OR REPLACE FUNCTION _pg_ripple.{fn_name}() \
         RETURNS TRIGGER LANGUAGE plpgsql AS $$ \
         DECLARE \
           current_count BIGINT; \
         BEGIN \
           SELECT count(*) INTO current_count \
           FROM _pg_ripple.vp_rare WHERE g = {graph_id}; \
           IF current_count > {quota_triples} THEN \
             RAISE EXCEPTION 'PT545: quota exceeded for graph {graph_iri}: \
               % triples > quota %', current_count, {quota_triples}; \
           END IF; \
           RETURN NEW; \
         END $$"
    );

    let _ = Spi::run_with_args(&fn_sql, &[]).inspect_err(|e| {
        pgrx::warning!("create_tenant: failed to create quota function: {e}");
    });

    let trigger_sql = format!(
        "DROP TRIGGER IF EXISTS {trigger_name} ON _pg_ripple.vp_rare; \
         CREATE TRIGGER {trigger_name} \
         AFTER INSERT ON _pg_ripple.vp_rare \
         FOR EACH ROW EXECUTE FUNCTION _pg_ripple.{fn_name}()"
    );

    let _ = Spi::run_with_args(&trigger_sql, &[]).inspect_err(|e| {
        pgrx::warning!("create_tenant: failed to create quota trigger: {e}");
    });
}

// ─── Internal helpers exposed to security_api ────────────────────────────────
// These wrappers make the private security_api functions callable from tenant.rs.
