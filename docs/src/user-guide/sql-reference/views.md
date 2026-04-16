# Materialized Views

pg_ripple v0.11.0 integrates with [pg_trickle](https://github.com/grove/pg-trickle) to provide always-fresh, incrementally-maintained stream tables for SPARQL queries, Datalog goals, and predicate semi-joins. All three features are soft-gated — pg_ripple loads and operates normally without pg_trickle; the new functions detect its absence at call time and return a clear error with an install hint.

---

## Checking pg_trickle availability

```sql
SELECT pg_ripple.pg_trickle_available();
-- true  (pg_trickle is installed)
-- false (pg_trickle not installed; view functions will error)
```

---

## SPARQL views

A SPARQL view compiles a SPARQL SELECT query into a pg_trickle stream table that stays up to date automatically as triples change.

### create_sparql_view

```sql
pg_ripple.create_sparql_view(
    name     TEXT,
    sparql   TEXT,
    schedule TEXT    DEFAULT '1s',
    decode   BOOLEAN DEFAULT false
) → BIGINT
```

Compiles the SPARQL SELECT to SQL and registers a pg_trickle stream table. Returns the number of projected columns.

- **name** — unique identifier for the view (becomes the stream table name in `pg_ripple` schema)
- **sparql** — a valid SPARQL SELECT query
- **schedule** — pg_trickle refresh interval (e.g. `'1s'`, `'10s'`, `'1m'`)
- **decode** — when `true`, dictionary IDs are decoded to human-readable strings in the stream table; when `false` (default), columns contain raw `BIGINT` IDs for maximum performance

```sql
-- Create a view of all people and their names
SELECT pg_ripple.create_sparql_view(
    'people_names',
    'SELECT ?person ?name WHERE {
       ?person <http://xmlns.com/foaf/0.1/name> ?name
     }',
    '5s',
    true
);

-- Query the materialized view like a regular table
SELECT * FROM pg_ripple.people_names;
```

### drop_sparql_view

```sql
pg_ripple.drop_sparql_view(name TEXT) → BOOLEAN
```

Drops the stream table and removes the catalog entry.

### list_sparql_views

```sql
pg_ripple.list_sparql_views() → JSONB
```

Returns a JSONB array of all registered SPARQL views, including name, original query, schedule, and decode mode.

---

## Datalog views

A Datalog view bundles a rule set with a goal pattern into a self-refreshing stream table.

### create_datalog_view

```sql
pg_ripple.create_datalog_view(
    name          TEXT,
    rules         TEXT,
    goal          TEXT,
    rule_set_name TEXT    DEFAULT 'custom',
    schedule      TEXT    DEFAULT '10s',
    decode        BOOLEAN DEFAULT false
) → BIGINT
```

Parses inline Datalog rules, compiles the goal query to SQL, and registers a pg_trickle stream table. Returns the number of projected columns.

```sql
-- View all inferred grandparent relationships, refreshing every 10 seconds
SELECT pg_ripple.create_datalog_view(
    'grandparents',
    '?x <http://example.org/grandparent> ?z :-
       ?x <http://example.org/parent> ?y ,
       ?y <http://example.org/parent> ?z .',
    '?x <http://example.org/grandparent> ?z',
    'family',
    '10s',
    true
);

SELECT * FROM pg_ripple.grandparents;
```

### create_datalog_view_from_rule_set

```sql
pg_ripple.create_datalog_view_from_rule_set(
    name      TEXT,
    rule_set  TEXT,
    goal      TEXT,
    schedule  TEXT    DEFAULT '10s',
    decode    BOOLEAN DEFAULT false
) → BIGINT
```

References an existing named rule set (loaded earlier via `load_rules()` or `load_rules_builtin()`) instead of providing inline rules.

```sql
-- Load rules once
SELECT pg_ripple.load_rules_builtin('rdfs');

-- Create a view using those rules
SELECT pg_ripple.create_datalog_view_from_rule_set(
    'all_types',
    'rdfs',
    '?x <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ?t',
    '30s',
    true
);
```

### drop_datalog_view / list_datalog_views

```sql
pg_ripple.drop_datalog_view(name TEXT) → BOOLEAN
pg_ripple.list_datalog_views() → JSONB
```

Same lifecycle management as SPARQL views.

---

## Extended Vertical Partitioning (ExtVP)

ExtVP pre-computes the semi-join between two frequently co-joined predicate pairs. The SPARQL query engine detects and uses ExtVP tables automatically when they exist, giving 2–10× speedups on star patterns.

### create_extvp

```sql
pg_ripple.create_extvp(
    name      TEXT,
    pred1_iri TEXT,
    pred2_iri TEXT,
    schedule  TEXT DEFAULT '10s'
) → BIGINT
```

Creates a pg_trickle stream table containing the pre-computed semi-join between two predicate VP tables. Returns the column count.

```sql
-- Pre-compute the join between foaf:name and foaf:knows
SELECT pg_ripple.create_extvp(
    'name_knows',
    '<http://xmlns.com/foaf/0.1/name>',
    '<http://xmlns.com/foaf/0.1/knows>',
    '10s'
);
```

When the SPARQL engine encounters a star pattern joining these two predicates, it will use the ExtVP table instead of joining the two VP tables at query time.

### drop_extvp / list_extvp

```sql
pg_ripple.drop_extvp(name TEXT) → BOOLEAN
pg_ripple.list_extvp() → JSONB
```

---

## Catalog tables

| Table | Description |
|-------|-------------|
| `_pg_ripple.sparql_views` | Name, original SPARQL, generated SQL, schedule, decode mode, stream table name, variables |
| `_pg_ripple.datalog_views` | Name, rules, rule set, goal, generated SQL, schedule, decode mode, stream table name, variables |
| `_pg_ripple.extvp_tables` | Name, predicate IRIs, predicate IDs, generated SQL, schedule, stream table name |

---

## When to use views

| Use case | Recommendation |
|----------|----------------|
| Dashboard with a few key metrics | SPARQL view with `decode = true`, schedule `'5s'` |
| Incremental RDFS/OWL materialization | Datalog view from built-in rule set |
| Star-pattern heavy workload | ExtVP on the top 5–10 predicate pairs |
| Ad-hoc exploration | Use `sparql()` directly — no view needed |
| Write-heavy with rare reads | Avoid views (refresh cost outweighs read savings) |
