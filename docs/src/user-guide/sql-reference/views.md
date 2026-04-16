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

Example output (with `decode = true`):

```
          person          |        name
-------------------------+--------------------
 http://example.org/alice | Alice Smith
 http://example.org/bob   | Bob Johnson
 http://example.org/carol | Carol Williams
(3 rows)
```

With `decode = false`, columns contain raw dictionary IDs (BIGINT):

```
      person      |        name
-----------------+--------------------
 4728391847263   | 4728391847264
 4728391847265   | 4728391847266
 4728391847267   | 4728391847268
(3 rows)
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

```sql
SELECT pg_ripple.list_sparql_views();
```

Example output:

```json
[
  {
    "name": "people_names",
    "sparql": "SELECT ?person ?name WHERE { ?person <http://xmlns.com/foaf/0.1/name> ?name }",
    "schedule": "5s",
    "decode": true,
    "stream_table_name": "pg_ripple.people_names",
    "variables": ["person", "name"]
  },
  {
    "name": "all_students",
    "sparql": "SELECT ?s WHERE { ?s <http://xmlns.com/foaf/0.1/isPrimaryTopicOf> ?doc }",
    "schedule": "10s",
    "decode": false,
    "stream_table_name": "pg_ripple.all_students",
    "variables": ["s"]
  }
]
```

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

Example output (inferred from explicit parent triples):

```
      ?x       |      ?z
--------------+--------------
 john         | grandpa
 jane         | grandpa
 bob          | grandma
(3 rows)
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

SELECT * FROM pg_ripple.all_types;
```

Example output (includes explicit types + inferred RDFS subclass/domain types):

```
       ?x        |               ?t
---------------+-------------------------------
 alice         | http://example.org/Person
 bob           | http://example.org/Person
 alice         | http://xmlns.com/foaf/0.1/Agent
 bob           | http://xmlns.com/foaf/0.1/Agent
(4 rows)
```

### drop_datalog_view / list_datalog_views

```sql
pg_ripple.drop_datalog_view(name TEXT) → BOOLEAN
pg_ripple.list_datalog_views() → JSONB
```

Same lifecycle management as SPARQL views.

```sql
SELECT pg_ripple.list_datalog_views();
```

Example output:

```json
[
  {
    "name": "grandparents",
    "rule_set": "family",
    "goal": "?x <http://example.org/grandparent> ?z",
    "schedule": "10s",
    "decode": true,
    "stream_table_name": "pg_ripple.grandparents",
    "variables": ["?x", "?z"]
  },
  {
    "name": "all_types",
    "rule_set": "rdfs",
    "goal": "?x <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ?t",
    "schedule": "30s",
    "decode": true,
    "stream_table_name": "pg_ripple.all_types",
    "variables": ["?x", "?t"]
  }
]
```

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

SELECT * FROM pg_ripple.name_knows;
```

Example output (semi-join of subjects from both predicates):

```
         s
------------------
 alice
 bob
 carol
(3 rows)
```

When the SPARQL engine encounters a star pattern joining these two predicates, it will use the ExtVP table instead of joining the two VP tables at query time.

### drop_extvp / list_extvp

```sql
pg_ripple.drop_extvp(name TEXT) → BOOLEAN
pg_ripple.list_extvp() → JSONB
```

```sql
SELECT pg_ripple.list_extvp();
```

Example output:

```json
[
  {
    "name": "name_knows",
    "pred1_iri": "<http://xmlns.com/foaf/0.1/name>",
    "pred2_iri": "<http://xmlns.com/foaf/0.1/knows>",
    "pred1_id": 5632187461234,
    "pred2_id": 5632187461245,
    "schedule": "10s",
    "stream_table_name": "pg_ripple.name_knows"
  }
]
```

---

## Catalog tables

| Table | Description |
|-------|-------------|
| `_pg_ripple.sparql_views` | Name, original SPARQL, generated SQL, schedule, decode mode, stream table name, variables |
| `_pg_ripple.datalog_views` | Name, rules, rule set, goal, generated SQL, schedule, decode mode, stream table name, variables |
| `_pg_ripple.extvp_tables` | Name, predicate IRIs, predicate IDs, generated SQL, schedule, stream table name |
| `_pg_ripple.construct_views` | Name, SPARQL, generated SQL, schedule, decode mode, template count, stream table name |
| `_pg_ripple.describe_views` | Name, SPARQL, generated SQL, schedule, decode mode, CBD strategy, stream table name |
| `_pg_ripple.ask_views` | Name, SPARQL, generated SQL, schedule, stream table name |

---

## CONSTRUCT views (v0.18.0)

A CONSTRUCT view compiles a SPARQL CONSTRUCT query into a pg_trickle stream table with schema `(s BIGINT, p BIGINT, o BIGINT, g BIGINT)`. Rows reflect the CONSTRUCT output at all times — inserting or deleting triples that affect the WHERE pattern causes the stream table to update automatically.

### create_construct_view

```sql
pg_ripple.create_construct_view(
    name     TEXT,
    sparql   TEXT,
    schedule TEXT    DEFAULT '1s',
    decode   BOOLEAN DEFAULT false
) → BIGINT
```

Returns the number of template triples registered. The stream table `pg_ripple.construct_view_{name}` is created automatically. When `decode = true`, a companion view `pg_ripple.construct_view_{name}_decoded(s TEXT, p TEXT, o TEXT, g BIGINT)` is also created.

**Error conditions:**
- `sparql` is not a CONSTRUCT query → `"sparql must be a CONSTRUCT query"`
- Template contains an unbound variable → lists the unbound variables
- Template contains a blank node → advises replacement with IRIs or skolemisation

```sql
-- Materialise inferred type triples: everything that is a foaf:Person is also a foaf:Agent
SELECT pg_ripple.create_construct_view(
    'inferred_agents',
    'CONSTRUCT { ?person <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
                          <http://xmlns.com/foaf/0.1/Agent> }
     WHERE { ?person <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
                     <http://xmlns.com/foaf/0.1/Person> }',
    '5s'
);

-- The materialized triples are stored as BIGINT IDs.
SELECT * FROM pg_ripple.construct_view_inferred_agents LIMIT 5;
```

### drop_construct_view

```sql
pg_ripple.drop_construct_view(name TEXT) → void
```

Drops the stream table and removes the catalog entry. Also drops the `_decoded` view if present.

### list_construct_views

```sql
pg_ripple.list_construct_views() → JSONB
```

Returns a JSONB array of all registered CONSTRUCT views.

```json
[
  {
    "name": "inferred_agents",
    "sparql": "CONSTRUCT { ... } WHERE { ... }",
    "schedule": "5s",
    "decode": false,
    "template_count": 1,
    "stream_table": "pg_ripple.construct_view_inferred_agents",
    "created_at": "2026-04-16T10:00:00Z"
  }
]
```

---

## DESCRIBE views (v0.18.0)

A DESCRIBE view compiles a SPARQL DESCRIBE query into a pg_trickle stream table with schema `(s BIGINT, p BIGINT, o BIGINT, g BIGINT)`, materialising the Concise Bounded Description (CBD) of the described resources.

The `pg_ripple.describe_strategy` GUC is respected: `cbd` (outgoing arcs only, default) or `scbd` (symmetric — outgoing + incoming arcs).

### create_describe_view

```sql
pg_ripple.create_describe_view(
    name     TEXT,
    sparql   TEXT,
    schedule TEXT    DEFAULT '1s',
    decode   BOOLEAN DEFAULT false
) → void
```

The stream table `pg_ripple.describe_view_{name}` is created automatically. When `decode = true`, a companion `_decoded` view is also created.

```sql
-- Materialise all triples about people with a given name
SELECT pg_ripple.create_describe_view(
    'named_people',
    'DESCRIBE ?person WHERE {
       ?person <http://xmlns.com/foaf/0.1/name> "Alice"
     }',
    '10s'
);

SELECT * FROM pg_ripple.describe_view_named_people;
```

### drop_describe_view

```sql
pg_ripple.drop_describe_view(name TEXT) → void
```

### list_describe_views

```sql
pg_ripple.list_describe_views() → JSONB
```

---

## ASK views (v0.18.0)

An ASK view compiles a SPARQL ASK query into a single-row stream table with schema `(result BOOLEAN, evaluated_at TIMESTAMPTZ)`. The `result` column flips whenever the underlying pattern's satisfiability changes — useful for live constraint monitors and dashboard indicators.

### create_ask_view

```sql
pg_ripple.create_ask_view(
    name     TEXT,
    sparql   TEXT,
    schedule TEXT DEFAULT '1s'
) → void
```

The stream table `pg_ripple.ask_view_{name}` is created automatically.

```sql
-- Monitor whether any person lacks a name (constraint violation indicator)
SELECT pg_ripple.create_ask_view(
    'person_missing_name',
    'ASK { ?person <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
                   <http://xmlns.com/foaf/0.1/Person> .
           FILTER NOT EXISTS { ?person <http://xmlns.com/foaf/0.1/name> ?name } }',
    '5s'
);

-- Check the live result.
SELECT result, evaluated_at FROM pg_ripple.ask_view_person_missing_name;
--  result | evaluated_at
-- --------+---------------------------
--  f      | 2026-04-16 10:05:00+00
```

### drop_ask_view

```sql
pg_ripple.drop_ask_view(name TEXT) → void
```

### list_ask_views

```sql
pg_ripple.list_ask_views() → JSONB
```

---

## When to use views

| Use case | Recommendation |
|----------|----------------|
| Dashboard with a few key metrics | SPARQL view with `decode = true`, schedule `'5s'` |
| Incremental RDFS/OWL materialization | Datalog view from built-in rule set |
| Star-pattern heavy workload | ExtVP on the top 5–10 predicate pairs |
| Ad-hoc exploration | Use `sparql()` directly — no view needed |
| Write-heavy with rare reads | Avoid views (refresh cost outweighs read savings) |
