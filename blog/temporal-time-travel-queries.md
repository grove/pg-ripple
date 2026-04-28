[← Back to Blog Index](README.md)

# Time-Travel Queries for Knowledge Graphs

## Point-in-time graph snapshots using statement timelines

---

Your knowledge graph changes over time. Alice was in the Engineering department last month. Now she's in Product. A compliance audit asks: "What did the graph look like on March 15th?"

Most triple stores can't answer this. They store the current state. The history is gone.

pg_ripple keeps the history. Every triple has a birth timestamp (when it was inserted) and, if deleted, a death timestamp. Point-in-time queries reconstruct the graph as it existed at any past moment.

---

## The Statement Timeline

Every VP table has a companion timeline:

```sql
_pg_ripple.statement_id_timeline (
  sid        BIGINT PRIMARY KEY,   -- references the statement ID
  created_at TIMESTAMPTZ NOT NULL,
  deleted_at TIMESTAMPTZ           -- NULL if still alive
)
```

When a triple is inserted, a row is added with `created_at = now()` and `deleted_at = NULL`. When a triple is deleted, `deleted_at` is set to `now()`.

The timeline is BRIN-indexed on `created_at` for efficient range scans.

---

## Point-in-Time Queries

```sql
-- Set the time horizon
SELECT pg_ripple.point_in_time('2026-03-15T00:00:00Z');

-- All subsequent SPARQL queries see the graph as of March 15
SELECT * FROM pg_ripple.sparql('
  SELECT ?person ?department WHERE {
    ?person ex:department ?department .
    ?person rdf:type ex:Employee .
  }
');
-- Returns Alice in Engineering (her March 15 department)

-- Reset to current time
SELECT pg_ripple.point_in_time(NULL);
```

`point_in_time()` sets a session-local GUC that the SPARQL-to-SQL translator uses to add timeline filters:

```sql
-- Generated SQL includes timeline join
SELECT ...
FROM _pg_ripple.vp_42 t
JOIN _pg_ripple.statement_id_timeline tl ON tl.sid = t.i
WHERE tl.created_at <= '2026-03-15'
  AND (tl.deleted_at IS NULL OR tl.deleted_at > '2026-03-15')
```

Only triples that existed at the specified timestamp are visible.

---

## Temporal Diff

"What changed between March 1 and March 31?"

```sql
SELECT * FROM pg_ripple.temporal_diff(
  start_time => '2026-03-01T00:00:00Z',
  end_time   => '2026-03-31T23:59:59Z',
  predicate  => 'ex:department'
);
```

Returns:

| subject | old_value | new_value | changed_at |
|---------|-----------|-----------|------------|
| ex:alice | ex:engineering | ex:product | 2026-03-18T09:15:00Z |
| ex:bob | NULL | ex:marketing | 2026-03-05T14:30:00Z |

The diff shows:
- Alice moved from Engineering to Product on March 18.
- Bob was added to Marketing on March 5.

---

## Use Cases

### Compliance Auditing

"Show me the state of the access control graph on the date of the incident."

```sql
SELECT pg_ripple.point_in_time('2026-02-14T08:00:00Z');
SELECT * FROM pg_ripple.sparql('
  SELECT ?user ?role ?resource WHERE {
    ?user ex:hasRole ?role .
    ?role ex:canAccess ?resource .
    FILTER(?resource = ex:financial_records)
  }
');
```

This reconstructs who had access to what at the moment the incident occurred — forensic evidence from the knowledge graph.

### Debugging Data Quality

"When did this triple first appear?"

```sql
SELECT tl.created_at, tl.deleted_at
FROM _pg_ripple.vp_{pred_id} t
JOIN _pg_ripple.statement_id_timeline tl ON tl.sid = t.i
WHERE t.s = pg_ripple.encode_iri('http://example.org/alice')
  AND t.o = pg_ripple.encode_iri('http://example.org/invalid_dept');
```

If a SHACL violation appeared last Tuesday, you can trace it to the exact insertion time and correlate with the data load that caused it.

### Temporal Analytics

"How has the department headcount changed over the last 12 months?"

```sql
SELECT month, department, count(*) AS headcount
FROM generate_series(
  '2025-05-01'::timestamptz,
  '2026-04-01'::timestamptz,
  '1 month'
) AS month
CROSS JOIN LATERAL (
  SELECT * FROM pg_ripple.sparql_at(
    month,
    'SELECT ?person ?dept WHERE { ?person ex:department ?dept }'
  )
) AS snapshot(person text, dept text)
GROUP BY month, department
ORDER BY month, department;
```

This generates monthly snapshots of department headcounts — a time series built from the knowledge graph's history.

---

## Storage Cost

The timeline adds one row per triple (16–24 bytes). For a graph with 10 million triples, that's ~200 MB of timeline data. With BRIN indexing, the index overhead is negligible (~0.1% of the data size).

The timeline table is append-only for inserts. Deletes update the `deleted_at` column in place. No WAL amplification from maintaining the timeline.

For very long-lived graphs, old timeline entries can be vacuumed:

```sql
-- Remove timeline entries for triples deleted more than 1 year ago
SELECT pg_ripple.vacuum_timeline(
  retention => '1 year'
);
```

After vacuuming, point-in-time queries earlier than the retention boundary may return incomplete results. The retention period is a business decision: how far back do you need to see?

---

## Not a Full Temporal Database

pg_ripple's time-travel is simpler than a full bi-temporal database:

- **Transaction time only.** The timeline records when the database learned about a triple, not when the fact was true in the real world. For real-world validity periods, use RDF-star annotations (`ex:from`, `ex:to`).
- **No temporal joins.** You can't write "find all people who were in the same department at the same time" in a single point-in-time query. You'd need to query at each timepoint and intersect.
- **Statement-level granularity.** The timeline tracks individual triples, not graph-level snapshots. Reconstructing the full graph at a point in time requires filtering all VP tables — which is efficient (BRIN indexes) but not instant.

For most compliance and debugging use cases, transaction-time history is sufficient. For temporal reasoning (Allen's interval algebra, bitemporal queries), you'd combine pg_ripple's timeline with RDF-star annotations and SPARQL FILTER expressions.
