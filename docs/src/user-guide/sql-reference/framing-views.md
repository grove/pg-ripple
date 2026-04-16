# Framing Views (v0.17.0)

Framing views combine JSON-LD Framing with pg_trickle to create live, incrementally-maintained stream tables. A framing view translates your frame into a SPARQL CONSTRUCT query once and registers it with pg_trickle; whenever triples are inserted or deleted, only the VP tables referenced by the frame are rescanned.

> **Requires pg_trickle.** Call `pg_ripple.pg_trickle_available()` to check. All functions raise a descriptive error at call time when pg_trickle is absent; extension load never fails.

## `create_framing_view`

```sql
pg_ripple.create_framing_view(
    name          TEXT,
    frame         JSONB,
    schedule      TEXT    DEFAULT '5s',
    decode        BOOLEAN DEFAULT FALSE,
    output_format TEXT    DEFAULT 'jsonld'
) RETURNS void
```

Creates a pg_trickle stream table `pg_ripple.framing_view_{name}` with the schema:

```
subject_id   BIGINT       -- dictionary-encoded subject IRI
frame_tree   JSONB        -- fully embedded + compacted JSON-LD for this root node
refreshed_at TIMESTAMPTZ
```

When `decode = TRUE`, a companion view `pg_ripple.framing_view_{name}_decoded` is also created. It decodes `subject_id` to a human-readable IRI string.

```sql
-- Create a live company directory refreshed every 10 seconds.
SELECT pg_ripple.create_framing_view(
    'companies',
    '{
        "@context": {"schema": "https://schema.org/"},
        "@type": "https://schema.org/Organization",
        "https://schema.org/name": {},
        "https://schema.org/employee": {
            "https://schema.org/name": {}
        }
    }'::jsonb,
    '10s',
    TRUE
);

-- Query it like a regular table.
SELECT subject_id, frame_tree FROM pg_ripple.framing_view_companies;
```

## `drop_framing_view`

```sql
pg_ripple.drop_framing_view(name TEXT) RETURNS BOOLEAN
```

Drops the stream table, its optional decode view, and the catalog entry.

```sql
SELECT pg_ripple.drop_framing_view('companies');
```

## `list_framing_views`

```sql
pg_ripple.list_framing_views() RETURNS JSONB
```

Returns a JSONB array of all registered framing views, ordered by creation time. Each entry includes `name`, `frame`, `schedule`, `output_format`, `decode`, and `created_at`.

```sql
SELECT pg_ripple.list_framing_views();
```

## Refresh Mode Selection

Choose the refresh mode based on your use case:

| Refresh mode | When to use |
|---|---|
| `IMMEDIATE` | Constraint-style frames: any matched node is a violation (e.g. companies lacking a compliance officer). Fires within the same transaction as the DML. |
| `DIFFERENTIAL` + schedule | Dashboard / API use cases: only changed subjects are reprocessed. Suitable for a company directory refreshed every 10 s. |
| `FULL` + long schedule | Large full-graph framed exports for data warehouses. Safe for deep nesting or `@always` embedding. |

## Decode Option

The `decode = TRUE` option creates a thin view that calls `pg_ripple.decode_iri(subject_id)` to expose the subject IRI as a human-readable string. The stream table itself stores integer IDs to minimise change data capture (CDC) surface.

```sql
-- Query the decoded view (requires decode = TRUE at creation time).
SELECT subject_iri, frame_tree FROM pg_ripple.framing_view_companies_decoded;
```

## Catalog Table

All framing views are recorded in `_pg_ripple.framing_views`:

| Column | Type | Description |
|---|---|---|
| `name` | TEXT | View name (primary key) |
| `frame` | JSONB | Original frame document |
| `generated_construct` | TEXT | SPARQL CONSTRUCT string used by pg_trickle |
| `schedule` | TEXT | pg_trickle refresh schedule |
| `output_format` | TEXT | `jsonld`, `ndjson`, or `turtle` |
| `decode` | BOOLEAN | Whether the decode view was created |
| `created_at` | TIMESTAMPTZ | Creation timestamp |

## pg_trickle Dependency

`create_framing_view()` and `drop_framing_view()` check for pg_trickle at call time. If absent, they raise:

```
ERROR: pg_trickle is required for framing views — install pg_trickle and add it to
shared_preload_libraries, then retry
```

Extension load never fails due to a missing pg_trickle. See [pg_trickle](https://github.com/grove/pg-trickle) for installation instructions.
