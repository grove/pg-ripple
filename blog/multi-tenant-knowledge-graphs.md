[← Back to Blog Index](README.md)

# Multi-Tenant Knowledge Graphs with Quotas

## Per-tenant isolation, triple limits, and row-level security — on a single PostgreSQL instance

---

You're building a SaaS platform where each customer gets their own knowledge graph. You have 200 tenants. Running 200 PostgreSQL instances is expensive. Running 200 pg_ripple installations is absurd.

pg_ripple's multi-tenancy model gives each tenant an isolated named graph with quota enforcement and row-level security — all on a single PostgreSQL instance.

---

## Named Graphs as Tenant Boundaries

Each tenant's data lives in a dedicated named graph:

```sql
-- Create a tenant
SELECT pg_ripple.create_tenant(
  tenant_id    => 'acme-corp',
  graph        => 'http://example.org/tenant/acme-corp',
  triple_limit => 1000000   -- 1M triple quota
);

-- Load data into the tenant's graph
SELECT pg_ripple.load_turtle('
  @prefix ex: <http://example.org/> .
  ex:alice foaf:name "Alice" .
  ex:alice rdf:type ex:Employee .
',
  graph => 'http://example.org/tenant/acme-corp'
);
```

The graph parameter isolates data: triples loaded with one graph ID are invisible to queries scoped to a different graph.

---

## Quota Enforcement

Each tenant has a configurable triple limit:

```sql
SELECT pg_ripple.set_tenant_quota(
  tenant_id    => 'acme-corp',
  triple_limit => 5000000  -- Upgrade to 5M
);
```

The quota is enforced by an AFTER INSERT trigger on VP tables. When a bulk load would exceed the limit:

```sql
SELECT pg_ripple.load_turtle('... 2M triples ...',
  graph => 'http://example.org/tenant/acme-corp'
);
-- ERROR: tenant 'acme-corp' triple quota exceeded (5,000,000 limit, 4,800,000 current, 2,000,000 attempted)
```

The entire load is rejected — no partial inserts. The tenant sees a clear error explaining the limit.

Quota checks are fast: a counter is maintained in the tenant registry table, updated by the trigger. The check is an integer comparison, not a COUNT(*) over VP tables.

---

## Row-Level Security

Named graph isolation prevents queries from seeing other tenants' data through SPARQL's graph scope. But VP tables are shared — all tenants' triples are in the same physical tables. Row-level security (RLS) adds a second layer of protection.

```sql
-- Grant access to a PostgreSQL role
SELECT pg_ripple.grant_graph(
  role  => 'acme_role',
  graph => 'http://example.org/tenant/acme-corp'
);
```

Under the hood, this creates RLS policies on VP tables:

```sql
-- Generated RLS policy (simplified)
CREATE POLICY tenant_acme ON _pg_ripple.vp_42
  FOR SELECT
  USING (g = (SELECT id FROM _pg_ripple.dictionary WHERE value = 'http://example.org/tenant/acme-corp'));
```

When `acme_role` queries the database, RLS filters automatically limit results to their graph. Even a `SELECT * FROM _pg_ripple.vp_42` returns only the tenant's rows.

```sql
SET ROLE acme_role;
SELECT * FROM pg_ripple.sparql('SELECT ?s ?p ?o WHERE { ?s ?p ?o }');
-- Only returns triples from acme-corp's graph
```

---

## Cross-Tenant Queries

Some use cases require querying across tenants — an admin dashboard, a platform-wide search, compliance reporting. pg_ripple handles this with a superuser role that bypasses RLS:

```sql
SET ROLE platform_admin;  -- superuser or has BYPASSRLS
SELECT * FROM pg_ripple.sparql('
  SELECT ?tenant (COUNT(*) AS ?triples) WHERE {
    GRAPH ?tenant { ?s ?p ?o }
  }
  GROUP BY ?tenant
');
```

This returns triple counts per tenant — only visible to the admin role.

---

## Tenant-Scoped Inference

Datalog inference can be scoped to a single tenant's graph:

```sql
SELECT pg_ripple.datalog_infer(
  graph => 'http://example.org/tenant/acme-corp'
);
```

Inference rules only consider triples in the tenant's graph. Inferred triples are stored in the same graph. This prevents one tenant's ontology from interfering with another's inference.

Shared ontology rules (RDFS, OWL RL) can be applied to all tenants by running inference without a graph scope:

```sql
SELECT pg_ripple.datalog_infer();  -- All graphs
```

---

## Tenant Lifecycle

```sql
-- Create
SELECT pg_ripple.create_tenant('acme-corp', ...);

-- Usage stats
SELECT * FROM pg_ripple.tenant_stats('acme-corp');
-- triple_count: 847291, quota_pct: 16.9%, graph_count: 1

-- Suspend (disallow writes, allow reads)
SELECT pg_ripple.suspend_tenant('acme-corp');

-- Delete (remove all data — uses erase_graph internally)
SELECT pg_ripple.delete_tenant('acme-corp');
```

`delete_tenant()` removes all triples in the tenant's graph across every VP table, cleans up dictionary entries, removes embeddings, retracts inferences, and drops the RLS policies. It's `erase_subject()` at the graph level.

---

## The Economics

Running 200 tenants on a single PostgreSQL instance vs. 200 separate instances:

| Metric | Shared instance | 200 instances |
|--------|----------------|---------------|
| Memory | 1 × shared_buffers | 200 × shared_buffers |
| Connections | 1 pool, RLS-isolated | 200 pools |
| Backup | 1 pg_dump | 200 pg_dumps |
| Monitoring | 1 dashboard | 200 dashboards |
| Dictionary cache | Shared (common terms cached once) | 200 caches (duplicated) |
| Inference | Shared OWL RL rules (loaded once) | 200 copies of the same rules |

The dictionary cache sharing is significant: common terms like `rdf:type`, `rdfs:label`, `owl:sameAs` appear in every tenant's data. With a shared instance, they're cached once. With separate instances, each caches its own copy.

---

## Trade-offs

- **Noisy neighbor risk.** A tenant running a complex SPARQL query can consume CPU that affects other tenants. Mitigation: `statement_timeout` per role, resource groups (PostgreSQL 18), or query complexity limits.
- **Schema collision.** All tenants share the same VP tables. If tenant A uses `ex:name` and tenant B uses `ex:name` with different semantics, the graph column separates them. But predicate statistics are global, which can affect query planning. This is rarely a problem in practice.
- **Migration complexity.** Moving a tenant to a dedicated instance requires exporting their graph, which is straightforward (`pg_ripple.export_ntriples(graph => ...)`) but requires downtime for the tenant.

For most SaaS knowledge graph use cases — where tenants have moderate-sized graphs (< 10M triples each) and the platform operator wants to minimize infrastructure — multi-tenancy on a single instance is the right choice.
