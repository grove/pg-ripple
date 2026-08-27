[← Back to Blog Index](README.md)

# JSON Writeback, and What It Took to Make It Actually Work

## The round-trip architecture behind `writeback_json_row()`, and the enqueue-coverage model that finally closes it

---

v0.128.0 added the last leg of a full JSON round trip: `register_json_mapping()`
stores a JSON-LD context, `ingest_json()` loads a payload as RDF, `export_json_node()`
reads it back out as JSON, and `writeback_json_row()` pushes RDF graph changes
back into the relational table they came from. On paper the loop closed. In
practice, the automatic half of it — trigger-based async writeback — never
ran at all, and it took two follow-up releases (v0.128.1's containment patch,
then this one) to find out why and fix it properly. This post is about that
architecture, what was actually broken, and the coverage model that makes
`enable_json_writeback()` mean what it says.

---

## The round trip

A JSON mapping is a named, bidirectional JSON-LD context:

```sql
SELECT pg_ripple.register_json_mapping(
    'contacts',
    '{"contact_id": "http://schema.org/identifier",
      "full_name":  "http://schema.org/name",
      "email_addr": "http://schema.org/email"}'::jsonb
);
```

`ingest_json()` uses that context to turn a JSON payload into triples:

```sql
SELECT pg_ripple.ingest_json(
    '{"contact_id": "c001", "full_name": "Alice Smith", "email_addr": "alice@example.com"}'::jsonb,
    'https://example.com/contacts/c001',
    'contacts'
);
```

Three triples land in the graph, one per mapped predicate. `export_json_node()`
runs the same context in reverse — RDF in, JSON out. Writeback closes the
remaining direction: RDF changes flow back into a relational table.

```sql
UPDATE _pg_ripple.json_mappings
SET writeback_table       = 'contacts',
    writeback_key_columns = ARRAY['contact_id'],
    writeback_conflict_policy = 'replace'
WHERE name = 'contacts';

SELECT pg_ripple.writeback_json_row('contacts', 'https://example.com/contacts/c001');
```

`writeback_json_row()` runs a `CONSTRUCT` for the subject, decodes each
`(predicate, object)` pair, maps predicates back to context terms, and
`INSERT ... ON CONFLICT`s the result into the target table using the
configured conflict policy (`replace`, `skip`, `error`). Called directly,
this always worked. The point of `enable_json_writeback()` is to stop calling
it directly: install triggers so every future insert or delete for a mapped
predicate enqueues a writeback event automatically, and a background worker
drains the queue.

## What was actually broken

`enable_json_writeback()` looks up each mapped predicate's dictionary id and
installs an `AFTER INSERT OR DELETE` trigger on its VP delta table, wired to
a shared PL/pgSQL function that enqueues a row into
`_pg_ripple.json_writeback_queue`. The background worker later dequeues and
replays each event through `writeback_json_row()` /
`writeback_json_row_delete()`.

Two of those lookups queried a column that has never existed:

```sql
-- what the code said
SELECT id FROM _pg_ripple.dictionary WHERE iri = $1 LIMIT 1;
SELECT iri FROM _pg_ripple.dictionary WHERE id = $1 LIMIT 1;

-- what the table actually has
-- _pg_ripple.dictionary (id, hash_hi, hash_lo, value, kind, datatype, lang, ...)
```

The real column is `value`, disambiguated by `kind` (`KIND_IRI` for
predicates and subjects). Every lookup failed — but the failure was
swallowed by an `.unwrap_or(None)`, so a predicate that genuinely had no
dictionary entry yet and a predicate whose lookup query was simply broken
looked identical from the caller's side: "not covered." v0.128.0 then made
things worse by setting `writeback_enabled = true` regardless, so the
catalog claimed a working event path that had never fired a single row.

v0.128.1 shipped an emergency containment patch: fail closed instead of
lying. If any mapped predicate couldn't get a working trigger,
`enable_json_writeback()` now raised an error and left `writeback_enabled =
false`. That was the right call for a hotfix — a loud, honest failure beats
a silent, wrong success — but it also meant the feature could no longer
succeed *at all*, since the underlying column bug made every predicate look
uncovered. `pg_ripple.feature_status()` reported `json_mapping_writeback` as
`broken` for exactly that reason.

This release fixes the lookups to use `value`/`kind`, and — just as
importantly — stops treating a *query failure* the same as a *missing
dictionary entry*. A malformed SQL error now propagates immediately; only a
genuine zero-row result (the predicate has never been ingested) is treated
as "not yet coverable."

## The coverage gap underneath the bug

Fixing the column name alone would not have been enough. `enable_json_writeback()`
only ever looked for a predicate's dedicated VP delta table
(`vp_{id}_delta`). But pg_ripple doesn't give every predicate its own table
immediately — predicates start out consolidated in a shared `vp_rare` table
and only get promoted to a dedicated delta/main/tombstones split once they
cross `vp_promotion_threshold` (1,000 triples by default). A predicate you
just started ingesting has no delta table yet, by design. Before this
release, that meant `enable_json_writeback()` could never succeed for a
freshly registered mapping — not until you'd pushed a thousand triples
through it.

The fix extends the enqueue trigger function to cover both storage tiers:

```
NOT YET PROMOTED                      PROMOTED
┌─────────────┐                    ┌──────────────────┐
│  vp_rare    │  ── (p, s, o) ──▶  │ vp_{id}_delta     │
│ (shared,    │   promote_predicate │ vp_{id}_main      │
│  all preds) │                    │ vp_{id}_tombstones │
└─────────────┘                    └──────────────────┘
      │                                    │
      ▼                                    ▼
 predicate-filtered              unfiltered trigger,
   trigger (2nd arg =            one per dedicated table
   predicate id)
      │                                    │
      └──────────────┬─────────────────────┘
                      ▼
        _pg_ripple.json_writeback_queue
```

`enable_json_writeback()` now installs a trigger on `vp_rare` scoped to the
mapped predicate's id (passed as a second trigger argument, since `vp_rare`
holds every predicate's rows and the trigger function has to ignore rows
that aren't its own), *and* a trigger on the dedicated delta table when one
already exists. A predicate only needs to have been ingested once — not
promoted — for coverage to succeed.

The other half is what happens when a covered predicate *does* get
promoted later. `promote_predicate()` is the single place a predicate's
dedicated tables come into existence, so it's also the single place that
needs to hand off coverage: right after creating the new tables, it now
calls into the JSON-mapping module to install triggers on the new delta and
tombstones tables for every mapping that already covers this predicate. The
old `vp_rare` trigger becomes a no-op for this predicate (its rows moved),
and the new dedicated-table triggers take over — no gap, no manual
re-enable.

Tombstones needed their own handling too. A delete of a triple that's
already been merged into `vp_{id}_main` doesn't delete a row from a delta
table — it inserts a row into `vp_{id}_tombstones` marking it deleted. The
shared trigger function treats an `INSERT` on a `*_tombstones` table as a
`delete` event, so main-resident deletes enqueue a writeback correctly
instead of being silently missed.

## The rest of the correctness pass

Three smaller bugs came out of the same audit, all in the direct writeback
path that `enable_json_writeback()` sits on top of:

- **Row counts were hard-coded.** `writeback_json_row()` always returned
  `1`, even when the `'skip'` conflict policy hit an existing row and
  inserted nothing. Both writeback functions now wrap their statement in a
  `RETURNING`-backed CTE and report the real count.
- **Every value was cast to `::text`.** `INSERT INTO t (id) SELECT $1::text`
  fails against an `integer` or `uuid` column — Postgres doesn't implicitly
  cast a `text`-typed expression across types. Values are now cast with the
  target column's real type (`CAST($1 AS integer)`), derived from
  `pg_attribute` once per mapping and cached in a new
  `writeback_column_casts` column so it isn't re-derived on every call.
- **A missing key column could build a broken placeholder list.** The
  `'error'` conflict policy's pre-check numbered SQL placeholders from the
  full key-column list but only bound values for the ones actually present
  — if one was missing, the query referenced a parameter that was never
  bound. Missing key values are now validated up front with a descriptive
  error, before any SQL gets built.

## Takeaway

None of this was a hard bug to find once you knew where to look — a wrong
column name, a swallowed error, a coverage check that assumed the wrong
storage tier. What made it dangerous was that each individual failure
degraded gracefully into something that *looked* like success:
`enable_json_writeback()` returned normally, `writeback_enabled` was `true`,
and nothing threw until you noticed the relational table just... never
updated. The fix isn't clever; it's mostly turning silent fallbacks into
loud, specific errors, and making sure the coverage model matches how
storage actually promotes predicates rather than how it's expected to.
