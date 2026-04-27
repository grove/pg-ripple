# Multi-Tenant Graphs

Multi-tenant graph storage is one of the trickiest patterns in any database. You have one logical product but many customers, each of whose data must be **invisible** to the others, **bounded** in size, and **measurable** for billing and capacity planning. pg_ripple solves this with three composable building blocks:

| Building block | What it gives you |
|---|---|
| **Named graphs** | Logical partitioning — each tenant lives in their own graph IRI |
| **Row-level security on graphs** | A tenant role sees only their own graph(s); enforced by PostgreSQL itself |
| **Tenant quotas** | A configurable triple-count cap per tenant, enforced on insert |

Together these give you *true* tenant isolation backed by PostgreSQL's permission system — no application-level filtering, no risk of an SQL injection bypassing the wall.

---

## When to use this

- A SaaS product where every customer has their own knowledge graph.
- A research platform where each project is sandboxed.
- A multi-team data platform where you want to share an instance without sharing data.

If you only need *labelled* partitioning without isolation (e.g. "this graph is from PubMed, this one from Crossref"), plain named graphs are enough. The features on this page are about **enforced** isolation.

---

## The model

```
   ┌──────────────────────────────────────────────────────────────┐
   │                    pg_ripple instance                         │
   │                                                               │
   │   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
   │   │  graph A     │  │  graph B     │  │  graph C     │      │
   │   │  (acme.com)  │  │  (globex)    │  │  (shared)    │      │
   │   │  500 K trip. │  │  120 K trip. │  │  20 K trip.  │      │
   │   │  quota: 1M   │  │  quota: 250K │  │  quota: ∞    │      │
   │   └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
   │          │                 │                 │              │
   │          └────► role tenant_acme   ◄─────────┤              │
   │                  role tenant_globex ◄────────┤              │
   │                                              │              │
   └──────────────────────────────────────────────┴──────────────┘
                                                  │
                                          everyone reads "shared"
```

- A **graph IRI** is the unit of isolation: triples are tagged with their graph at insert time.
- A **role** is the unit of access control: PostgreSQL roles own grants on graphs.
- A **tenant** is the unit of quota: every named graph that has been registered as a tenant carries a triple-count cap.

---

## Step 1 — Register tenants

`create_tenant()` registers a named graph as a tenant and assigns it a triple-count quota. After registration, every insert into that graph is checked against the quota; an over-quota insert raises `PT530`.

```sql
SELECT pg_ripple.create_tenant(
    graph_iri    := 'https://example.org/tenants/acme',
    triple_quota := 1_000_000
);

SELECT pg_ripple.create_tenant(
    graph_iri    := 'https://example.org/tenants/globex',
    triple_quota := 250_000
);
```

You can list current usage at any time:

```sql
SELECT graph_iri, triple_count, triple_quota,
       round(100.0 * triple_count / triple_quota, 1) AS pct_used
FROM pg_ripple.tenant_stats()
ORDER BY pct_used DESC;
```

The view is built on a trigger-maintained counter, so it is O(1) — safe to call from a billing job.

---

## Step 2 — Grant graphs to roles

`grant_graph(role, graph_iri)` is the analog of `GRANT … ON … TO …` for graphs. It registers a per-role visibility rule that pg_ripple enforces in every SPARQL query.

```sql
CREATE ROLE tenant_acme   LOGIN PASSWORD 'pw_a';
CREATE ROLE tenant_globex LOGIN PASSWORD 'pw_b';

GRANT USAGE ON SCHEMA pg_ripple, _pg_ripple TO tenant_acme, tenant_globex;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA pg_ripple TO tenant_acme, tenant_globex;

SELECT pg_ripple.grant_graph('tenant_acme',   'https://example.org/tenants/acme');
SELECT pg_ripple.grant_graph('tenant_globex', 'https://example.org/tenants/globex');

-- Both tenants share access to a public reference graph.
SELECT pg_ripple.grant_graph('tenant_acme',   'https://example.org/shared');
SELECT pg_ripple.grant_graph('tenant_globex', 'https://example.org/shared');
```

A SPARQL query run by `tenant_acme` now sees only triples in `acme` and `shared`. There is no application-level filter to forget; the rule is enforced at the storage layer.

---

## Step 3 — (Optional) attribute every change

Pair tenants with the [audit log](temporal-and-provenance.md#audit-log) for billing-grade attribution:

```sql
SET pg_ripple.audit_log_enabled = on;

-- Per-tenant write volume in the last 24h:
SELECT role, count(*) AS updates
FROM _pg_ripple.audit_log
WHERE ts > now() - interval '24 hours'
GROUP BY role
ORDER BY updates DESC;
```

For per-tenant *read* volume, capture `pg_stat_statements` data; pg_ripple integrates with it transparently.

---

## Operational concerns

### Quota enforcement

The trigger that enforces quotas runs in the same transaction as the insert. If a bulk-load would exceed the quota, **the entire load is rolled back** — there is no partial commit. Plan your loaders accordingly: chunk loads if you expect to flirt with the quota, or pre-check via `tenant_stats()`.

### Eviction

Quotas are caps, not LRU. pg_ripple does not automatically evict old triples when a tenant fills up. To shrink a tenant: delete triples (or call `clear_graph()`) and the counter updates immediately.

### Renaming or splitting a tenant

`rename_tenant(old_iri, new_iri)` updates the registration and re-tags every triple in a single transaction. Use it sparingly — it touches every triple in the graph.

### Backup and restore

Tenants are pure PostgreSQL objects. `pg_dump --schema=pg_ripple --schema=_pg_ripple` captures everything. To export a single tenant for legal-hold or migration:

```sql
COPY (SELECT * FROM pg_ripple.export_quads_for_graph('https://example.org/tenants/acme'))
TO '/tmp/acme.nq';
```

---

## Failure modes and pitfalls

1. **Forgetting to grant the shared graph.** A tenant without read access to your reference vocabularies will see types as opaque IRIs. Always grant `https://example.org/shared` (or your equivalent) to every tenant.
2. **Granting the default graph.** The default graph (graph ID `0`) is *not* tenant-scoped. Do not put tenant data into the default graph; it will be visible to everyone.
3. **Using a single role for all tenants.** Quota and RLS attach to the role. Sharing a role across tenants defeats both.
4. **Pre-emptive quotas vs reactive quotas.** PT530 is raised *before* commit, not afterwards. Long-running bulk loads should split into batches and check `tenant_stats()` after each batch.

---

## See also

- [Operations → Security](../operations/security.md) — the deeper PostgreSQL RLS background.
- [Temporal & Provenance](temporal-and-provenance.md) — audit-log capture for tenant attribution.
- [Cookbook: Audit trail with PROV-O and temporal queries](../cookbook/audit-trail.md)
