# SQL API Reference

Selected pg_ripple SQL functions with full parameter descriptions, return
schemas, and copy-pasteable examples. For the complete alphabetical list of all
157 functions see [sql-functions.md](sql-functions.md).

---

## Compatibility & diagnostics

### `compat_check`

Return a JSON object describing the installed extension version and its
compatibility with the HTTP companion.

```sql
pg_ripple.compat_check() → TEXT
```

**Returns** — a JSON string with the following keys:

| Key | Type | Description |
|-----|------|-------------|
| `extension_version` | `STRING` | Installed pg_ripple extension version, e.g. `"0.136.0"` |
| `http_min_version` | `STRING` | Minimum pg_ripple_http version required by this extension, e.g. `"0.135.0"` |
| `compatible` | `BOOL` | `true` when the running HTTP companion satisfies `http_min_version` |

**Example return value:**

```json
{
  "extension_version": "0.136.0",
  "http_min_version": "0.135.0",
  "compatible": true
}
```

**Example usage:**

```sql
SELECT pg_ripple.compat_check();
-- {"extension_version":"0.136.0","http_min_version":"0.135.0","compatible":true}

-- Parse as JSON:
SELECT
  (pg_ripple.compat_check()::jsonb) ->> 'extension_version' AS ext_version,
  (pg_ripple.compat_check()::jsonb) ->> 'compatible'        AS compatible;
```

---

## Benchmarking

### `bench_workload`

Run a benchmark workload profile against the local triple store and record the
results in `_pg_ripple.bench_history`.

```sql
pg_ripple.bench_workload(
    profile TEXT DEFAULT 'bsbm'
) → BIGINT
```

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `profile` | `TEXT` | `'bsbm'` | Benchmark profile to run. One of: `'bsbm'`, `'watdiv'`, `'pagerank'`, `'pprl'` |

**Returns** — the `run_id` of the newly inserted `_pg_ripple.bench_history` row.

```sql
-- Run BSBM benchmark:
SELECT pg_ripple.bench_workload('bsbm');

-- Run WatDiv benchmark:
SELECT pg_ripple.bench_workload('watdiv');
```

### `bench_workload_result`

Return the most recent benchmark run for the given profile.  Convenience
wrapper over `_pg_ripple.bench_history` — no raw SQL required.

```sql
pg_ripple.bench_workload_result(
    profile TEXT DEFAULT 'bsbm'
) → TABLE (
    run_id              BIGINT,
    profile             TEXT,
    started_at          TIMESTAMPTZ,
    duration_ms         BIGINT,
    queries_per_second  FLOAT8,
    triples_processed   BIGINT
)
```

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `profile` | `TEXT` | `'bsbm'` | Profile name to filter; must match a value previously passed to `bench_workload()` |

**Example usage:**

```sql
-- Run a benchmark and immediately retrieve the result:
SELECT pg_ripple.bench_workload('bsbm');
SELECT * FROM pg_ripple.bench_workload_result('bsbm');

-- Output:
--  run_id | profile | started_at               | duration_ms | queries_per_second | triples_processed
--  -------+---------+--------------------------+-------------+--------------------+------------------
--       1 | bsbm    | 2026-05-19 12:00:00+00   |         342 |             175.44 |             50000
```

---

## Rule library federation

### `publish_rule_library`

Publish an installed rule library so it can be subscribed to from remote
pg_ripple instances through the HTTP companion's rule-library stream endpoint.

```sql
pg_ripple.publish_rule_library(
    name         TEXT,
    endpoint_uri TEXT
) → VOID
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `TEXT` | Name of an installed rule library (1–64 alphanumeric/hyphen/underscore chars) |
| `endpoint_uri` | `TEXT` | Full HTTP URI at which the rule stream will be served, e.g. `'https://host/rule-libraries/my-lib/stream'` |

**Errors** — raises `PT046x` error codes for invalid name/URI, missing library,
or catalog write failure.

```sql
-- Publish the "rdfs-base" library on this instance:
SELECT pg_ripple.publish_rule_library(
    'rdfs-base',
    'https://instance-a.example.com/rule-libraries/rdfs-base/stream'
);
```

### `subscribe_rule_library`

Record the intent to subscribe to a remote rule-library stream. The source URI
must pass the SSRF policy check. The SQL function does not perform outbound
network I/O; fetching and installing the remote rules is performed by the HTTP
companion's `POST /rule-libraries/{name}/subscribe` endpoint.

```sql
pg_ripple.subscribe_rule_library(
    source_uri TEXT,
    name       TEXT
) → VOID
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `source_uri` | `TEXT` | Full HTTP URI of the remote stream endpoint |
| `name` | `TEXT` | Local name to use for the subscribed rule library |

**Errors** — raises `PT046x` error codes for SSRF-blocked URIs, invalid names,
or catalog write failure.

```sql
-- Record subscription intent for a rule library published on instance A:
SELECT pg_ripple.subscribe_rule_library(
    'https://instance-a.example.com/rule-libraries/rdfs-base/stream',
    'rdfs-base'
);
```

---

## Allen's interval relations

All seven Allen's interval relations are available as both SQL functions and
SPARQL extension functions (`pg:before`, `pg:meets`, etc.).

Each function accepts four `TIMESTAMPTZ` arguments representing the start and
end of two intervals: `(a_start, a_end, b_start, b_end)`.

| SQL function | SPARQL IRI | Relation |
|---|---|---|
| `pg_ripple.allen_before` | `pg:before` | A ends strictly before B starts |
| `pg_ripple.allen_meets` | `pg:meets` | A ends exactly when B starts |
| `pg_ripple.allen_overlaps` | `pg:overlaps` | A starts before B and they overlap |
| `pg_ripple.allen_during` | `pg:during` | A is entirely within B |
| `pg_ripple.allen_finishes` | `pg:finishes` | A ends at the same time as B |
| `pg_ripple.allen_starts` | `pg:starts` | A starts at the same time as B |
| `pg_ripple.allen_equals` | `pg:equals` | A and B have identical bounds |

### `allen_before`

```sql
pg_ripple.allen_before(
    a_start TIMESTAMPTZ,
    a_end   TIMESTAMPTZ,
    b_start TIMESTAMPTZ,
    b_end   TIMESTAMPTZ
) → BOOLEAN
```

Returns `true` when interval A ends strictly before interval B starts
(`a_end < b_start`).

```sql
SELECT pg_ripple.allen_before(
    '2026-01-01'::timestamptz, '2026-01-15'::timestamptz,
    '2026-02-01'::timestamptz, '2026-02-28'::timestamptz
);  -- true
```

### `allen_meets`

```sql
pg_ripple.allen_meets(
    a_start TIMESTAMPTZ,
    a_end   TIMESTAMPTZ,
    b_start TIMESTAMPTZ,
    b_end   TIMESTAMPTZ
) → BOOLEAN
```

Returns `true` when interval A ends at exactly the same instant B starts
(`a_end = b_start`).

### `allen_overlaps`

```sql
pg_ripple.allen_overlaps(
    a_start TIMESTAMPTZ,
    a_end   TIMESTAMPTZ,
    b_start TIMESTAMPTZ,
    b_end   TIMESTAMPTZ
) → BOOLEAN
```

Returns `true` when A starts before B and the intervals overlap but neither
contains the other (`a_start < b_start AND a_end > b_start AND a_end < b_end`).

### `allen_during`

```sql
pg_ripple.allen_during(
    a_start TIMESTAMPTZ,
    a_end   TIMESTAMPTZ,
    b_start TIMESTAMPTZ,
    b_end   TIMESTAMPTZ
) → BOOLEAN
```

Returns `true` when interval A is entirely within interval B
(`b_start < a_start AND a_end < b_end`).

### `allen_finishes`

```sql
pg_ripple.allen_finishes(
    a_start TIMESTAMPTZ,
    a_end   TIMESTAMPTZ,
    b_start TIMESTAMPTZ,
    b_end   TIMESTAMPTZ
) → BOOLEAN
```

Returns `true` when A ends at the same time as B but starts later
(`a_start > b_start AND a_end = b_end`).

### `allen_starts`

```sql
pg_ripple.allen_starts(
    a_start TIMESTAMPTZ,
    a_end   TIMESTAMPTZ,
    b_start TIMESTAMPTZ,
    b_end   TIMESTAMPTZ
) → BOOLEAN
```

Returns `true` when A starts at the same time as B but ends earlier
(`a_start = b_start AND a_end < b_end`).

### `allen_equals`

```sql
pg_ripple.allen_equals(
    a_start TIMESTAMPTZ,
    a_end   TIMESTAMPTZ,
    b_start TIMESTAMPTZ,
    b_end   TIMESTAMPTZ
) → BOOLEAN
```

Returns `true` when both intervals are identical
(`a_start = b_start AND a_end = b_end`).

**SPARQL example using interval relations:**

```sparql
PREFIX pg: <https://pgrdf.io/fn/>
PREFIX ex: <https://example.org/>

SELECT ?event ?label WHERE {
  ?event ex:startTime ?s ;
         ex:endTime   ?e ;
         ex:label     ?label .
  FILTER(pg:before(?s, ?e,
                   "2026-06-01T00:00:00Z"^^xsd:dateTime,
                   "2026-12-31T23:59:59Z"^^xsd:dateTime))
}
```

---

## See also

- [Full SQL function list](sql-functions.md)
- [GUC reference](guc-reference.md)
- [HTTP API reference](http-api.md)
