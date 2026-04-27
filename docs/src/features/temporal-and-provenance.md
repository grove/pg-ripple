# Temporal Queries and Provenance

Two of the most-asked compliance questions in any data system are:

1. *"What did the graph look like at 03:14 last Tuesday?"* — a **temporal** question.
2. *"Where did this fact come from, and who introduced it?"* — a **provenance** question.

pg_ripple answers both with first-class features that need no schema changes and no extra storage on the hot path.

| Capability | Function / GUC | Page section |
|---|---|---|
| Replay the graph as of a past timestamp | `pg_ripple.point_in_time(ts)` | [Temporal queries](#temporal-queries) |
| Record a `prov:Activity` per bulk-load | `pg_ripple.prov_enabled = on` | [PROV-O provenance](#prov-o-provenance) |
| Capture every SPARQL UPDATE | `pg_ripple.audit_log_enabled = on` | [Audit log](#audit-log) |
| Per-fact confidence and source | RDF-star quoted triples | [Storing Knowledge → RDF-star](storing-knowledge.md) |

The four are designed to compose. A regulator-defensible audit trail typically uses all four.

---

## Temporal queries

Every triple in pg_ripple carries a globally-unique statement ID (`i BIGINT`) drawn from a shared sequence. The sequence value monotonically increases with insertion time, so it acts as a logical timestamp. The `_pg_ripple.statement_id_timeline` table maps wall-clock timestamps to SID ranges.

`point_in_time(ts)` sets a session GUC that restricts every subsequent SPARQL and Datalog query to the SID range that existed at `ts`.

```sql
-- See the graph as it stood last Tuesday at 03:14.
SELECT pg_ripple.point_in_time('2026-04-21 03:14:00+00'::timestamptz);

-- All queries in this session are now scoped to that point in time.
SELECT * FROM pg_ripple.sparql('
    SELECT ?paper ?title WHERE {
        ?paper <http://purl.org/dc/elements/1.1/title> ?title
    }
');

-- Reset to "now".
SELECT pg_ripple.point_in_time(NULL);
```

### What this is good for

- **Audits**: prove what your application *would have* shown a user at the time of an event.
- **Reproducible analytics**: re-run a report against the exact data the original report saw.
- **Debugging**: bisect a quality regression by walking back through time.
- **Historical change-detection**: combine with [CDC subscriptions](cdc-subscriptions.md) to see *what changed between two points in time*.

### What this is **not**

- It is **not** a full bitemporal store. You cannot query "the version of fact X that was *believed* in March but *valid* for January". For that you need RDF-star plus your own valid-time predicate.
- It is **not** safe across `VACUUM FULL` of `_pg_ripple.statement_id_timeline`. Routine `VACUUM` is fine.

---

## PROV-O provenance

[PROV-O](https://www.w3.org/TR/prov-o/) is the W3C standard for describing the origin of data. Set one GUC and every bulk-load operation is automatically annotated with `prov:Activity` and `prov:Entity` triples.

```sql
SET pg_ripple.prov_enabled = on;

-- Every load is recorded.
SELECT pg_ripple.load_turtle_file('/data/products.ttl');
SELECT pg_ripple.load_nquads_file('/data/customers.nq');

-- Inspect the recorded activities.
SELECT * FROM pg_ripple.prov_stats();
```

A typical recorded activity looks like:

```turtle
_:act_42 a prov:Activity ;
    prov:startedAtTime "2026-04-27T10:00:00Z"^^xsd:dateTime ;
    prov:endedAtTime   "2026-04-27T10:00:14Z"^^xsd:dateTime ;
    prov:wasAssociatedWith <urn:postgres-role:loader_service> ;
    pg:loadFunction       "load_turtle_file" ;
    pg:sourceFile         "/data/products.ttl" ;
    pg:tripleCount        128432 .
```

You can query provenance with SPARQL like any other data. PROV-O integrates cleanly with `point_in_time()` — *"as of the close of business yesterday, which loader had touched this graph?"* is a single query.

---

## Audit log

While PROV-O captures bulk loads, the **audit log** captures every SPARQL UPDATE — `INSERT DATA`, `DELETE DATA`, `INSERT { … } WHERE`, `MOVE`, `COPY`, `LOAD`, etc. Set the GUC and entries land in `_pg_ripple.audit_log`:

```sql
SET pg_ripple.audit_log_enabled = on;

-- Every UPDATE is captured.
SELECT pg_ripple.sparql_update('
    INSERT DATA { <https://example.org/x> <https://example.org/y> "z" }
');

-- Inspect.
SELECT ts, role, txid, operation, query
FROM _pg_ripple.audit_log
ORDER BY ts DESC
LIMIT 20;

-- Cleanup (e.g. nightly).
SELECT pg_ripple.purge_audit_log(before := now() - interval '90 days');
```

The audit log is a PostgreSQL table — you can ship it to your SIEM, partition it, or replicate it like any other table.

### Per-fact provenance with RDF-star

For granular, *per-triple* provenance (rather than per-load or per-update), use RDF-star quoted triples. See [Storing Knowledge → RDF-star](storing-knowledge.md) for the syntax. The most common pattern:

```turtle
<< <:alice> <:knows> <:bob> >>
    :assertedBy <:dataset/foaf2024> ;
    :confidence "0.95"^^xsd:decimal ;
    :timestamp  "2026-04-27"^^xsd:date .
```

These quoted triples are queryable with the same SPARQL patterns as ordinary triples.

---

## Putting them together — a worked example

A regulator asks: *"On 21 March, did your system tell the user that drug A interacts with drug B? If so, on what evidence?"*

```sql
-- 1. Replay the state of the graph.
SELECT pg_ripple.point_in_time('2026-03-21 12:00:00+00');

-- 2. Re-ask the question.
SELECT * FROM pg_ripple.sparql('
    ASK { <https://example.org/drugA> <https://example.org/interactsWith> <https://example.org/drugB> }
');
-- → true

-- 3. Find the evidence (RDF-star + PROV-O).
SELECT * FROM pg_ripple.sparql('
    SELECT ?source ?confidence ?activity WHERE {
        << <https://example.org/drugA> <https://example.org/interactsWith> <https://example.org/drugB> >>
            <https://example.org/source>     ?source ;
            <https://example.org/confidence> ?confidence .
        ?activity <http://www.w3.org/ns/prov#generated> ?source .
    }
');

-- 4. Find the operator who loaded it.
SELECT * FROM pg_ripple.sparql('
    SELECT ?role WHERE {
        ?activity <http://www.w3.org/ns/prov#wasAssociatedWith> ?role
    }
');
```

That four-line chain is the kind of evidence a regulated industry needs and a black-box system cannot produce.

---

## Performance and storage notes

- `point_in_time()` is **read-only** and zero-cost on the write path.
- The `_pg_ripple.statement_id_timeline` table is a small append-only log: ~24 bytes per timestamp boundary. A 24/7 store accumulates a few KB per day.
- `prov_enabled` adds ~5–10 triples per bulk load. Negligible for any non-trivial load.
- `audit_log_enabled` writes one row per UPDATE statement. For OLTP-heavy workloads consider partitioning the table monthly.
- All three features are off by default. Enable them per database according to your compliance posture.

---

## See also

- [CDC Subscriptions](cdc-subscriptions.md) — push *changes* in real time, complementary to point-in-time *replay*.
- [Multi-Tenant Graphs](multi-tenant-graphs.md) — pair the audit log with RLS to attribute every change to a tenant role.
- [Cookbook: Audit trail with PROV-O and temporal queries](../cookbook/audit-trail.md)
