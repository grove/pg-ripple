# Prior Art Analysis: Virtuoso, maplib, Stardog, GraphDB, RDFox

> **License notes for this batch:**
> - **maplib** (DataTreehouse/maplib): Apache License 2.0 — safe to reference directly, including code patterns.
> - **Virtuoso Open-Source Edition** (openlink/virtuoso-opensource): GPL-2.0 — architectural ideas
>   only; no code reference. The open-source edition and the commercial edition differ; this analysis
>   covers OSE.
> - **Stardog**, **GraphDB**, **RDFox**: Proprietary commercial software — analysis is based solely
>   on publicly available documentation, blog posts, and academic papers. No source code is available
>   or referenced.
>
> This document records design insights drawn from public documentation.
> No code is copied; the analysis is used solely to inform pg_ripple's architecture.

---

## 1. Virtuoso Open-Source Edition (openlink/virtuoso-opensource)

**Repository**: https://github.com/openlink/virtuoso-opensource  
**License**: GPL-2.0 (commercial edition also available from OpenLink Software)  
**What it is**: A multi-model database server (relational + RDF/SPARQL + document) that has been
one of the dominant open-source triple stores since the early 2000s. Hosts the DBpedia SPARQL
endpoint and has been deployed at web scale. Major architectural innovations include a column-store
mode added in v7, and a predicate-clustered index scheme that directly inspired many later systems.

### 1.1 Relevance

Virtuoso is the most battle-tested open-source triple store and its published performance-tuning
documentation (`VirtRDFPerformanceTuning`) contains direct evidence of which index layouts and
storage decisions actually matter at production scale. pg_ripple's VP tables and `vp_rare` fallback
table solve the same access-pattern problem via a different strategy.

### 1.2 Lessons

#### 1.2.1 Predicate-first index layout: PSOG + POGS + three partial indices

Virtuoso's default RDF index scheme (since v6) is:

```sql
-- Primary key: predicate-leading full quad index
CREATE TABLE RDF_QUAD (G IRI_ID_8, S IRI_ID_8, P IRI_ID_8, O ANY,
    PRIMARY KEY (P, S, O, G));  -- PSOG

-- Full index: predicate + object, for P+O known lookups
CREATE BITMAP INDEX RDF_QUAD_POGS ON RDF_QUAD (P, O, G, S);

-- Partial indices (DISTINCT NO PRIMARY KEY REF = no duplicates stored)
CREATE DISTINCT NO PRIMARY KEY REF BITMAP INDEX RDF_QUAD_SP ON RDF_QUAD (S, P);
CREATE DISTINCT NO PRIMARY KEY REF INDEX          RDF_QUAD_OP ON RDF_QUAD (O, P);
CREATE DISTINCT NO PRIMARY KEY REF BITMAP INDEX RDF_QUAD_GS ON RDF_QUAD (G, S);
```

The key insight documented in their performance guide: **the design deliberately clusters by
predicate**. A page read from disk contains only entries for the same predicate, so consecutive
accesses in a property-valued scan have very high cache hit rates. This is exactly the same
intuition behind pg_ripple's VP (Vertical Partitioning) tables — one table per predicate — but
Virtuoso achieves it through index construction rather than physical table partitioning.

The partial `SP`, `OP`, and `GS` indices are allowed to go stale (entries are never deleted from
them). A stale entry is harmless because every lookup always validates against the full PSOG or
POGS index. This scheme saves 60–70% of space compared to four full-coverage quad indices.

**Implications for pg_ripple's `vp_rare` fallback table**:
- `vp_rare (p, s, o, g)` needs at minimum a `(p, s, o)` primary key and a `(p, o, s)` secondary
  index — the predicate-leading equivalent of PSOG + POGS.
- A `(g, s)` partial index on `vp_rare` would support graph-drop operations (drop all triples
  in a named graph) efficiently; this is an O(n) table scan without it.
- When a rare predicate is promoted to its own VP table (triple count crosses
  `pg_ripple.vp_promotion_threshold`), the `vp_rare` entries survive until the background
  merge worker cleans them up — exactly the stale-entry tolerance Virtuoso documents.

#### 1.2.2 Column-wise mode reduces storage to one-third

From Virtuoso v7 onward, the same `RDF_QUAD` table is stored in column-wise mode by default. The
documentation reports this reduces storage to ~1/3 of the row-wise equivalent.

pg_ripple's HTAP design already has this distinction: the delta partition uses heap (row store) for
fast individual inserts, while the main partition is intended for read-optimised bulk access. The
Virtuoso evidence confirms that the main partition should be built with BRIN indices
(range-based block-level indexing) rather than full B-trees, and that a column-store option for
the main partition (via PostgreSQL's `columnar` access method from pg_columnar, or a future
pg_ripple column-store extension) would be architecturally sound at v1.0.

#### 1.2.3 `sql:select-option "order"` as an escape hatch for cost model errors

Virtuoso's performance guide describes a SPARQL query annotation (`DEFINE sql:select-option
"order"`) that forces the query optimizer to join in left-to-right declaration order, bypassing the
cost model. This is intentionally a sharp tool — it is very easy to produce an unworkable plan —
but it is the only recourse when the optimizer makes a radically wrong cardinality estimate.

**Implication for pg_ripple's SPARQL query hint mechanism**: pg_ripple already plans a
`http://pg-ripple.io/hints/` IRI prefix for query hints (via `FROM`/`GRAPH` clauses). One concrete
hint should be `<http://pg-ripple.io/hints/join-order>` which, when present, appends
`/*+ ORDERED */` (or a SET LOCAL `join_collapse_limit = 1`) to the generated PostgreSQL query.
This gives power users the same escape hatch Virtuoso documents without requiring a separate
syntax extension.

#### 1.2.4 Large-graph vs. many-small-graphs index scheme selection

Virtuoso documents a known pathological case: when millions of small graphs each share the same
triple (the same (S, P, O) appears in 100,000 or more graphs), the default `GS` partial index
causes O(n) scan cost for graph-drop operations. Their solution is to replace it with a covering
index starting with G:

```sql
DROP INDEX RDF_QUAD_GS;
CREATE COLUMN INDEX RDF_QUAD_GPSO ON RDF_QUAD (G, P, S, O);
```

pg_ripple will face this same workload split:

| Workload | Optimal layout | Default for pg_ripple |
|---|---|---|
| Few large graphs (DBpedia-style) | Predicate-partitioned VP tables, G as filter | Yes — VP tables |
| Named-graph access control (many small graphs) | G-leading index on `vp_rare` | Needs `(g, p, s, o)` index added |
| Graph federation (GRAPH {...} dominant) | G-first on all VP tables | Optional GUC from v0.13.0 |

A GUC `pg_ripple.named_graph_optimized` (default `off`) that, when enabled, adds a `(g, s, o)` B-tree index on every VP table would address the many-small-graphs case without incurring cost in the default workload.

---

## 2. maplib (DataTreehouse/maplib)

**Repository**: https://github.com/DataTreehouse/maplib  
**License**: Apache 2.0  
**What it is**: A Rust library (with a Python API) for mapping tabular data (Pandas/Polars DataFrames)
to RDF triples using OTTR (Reasonable Ontology Templates). Built on Apache Arrow and Polars; returns
SPARQL results as Arrow `RecordBatch` / Polars `DataFrame` objects for zero-copy consumption by
Python data science pipelines. SHACL and Datalog are not open-source in the current version (2025).

### 2.1 Relevance

maplib is the closest open-source counterpart to pg_ripple in the Rust ecosystem. Both are Rust
systems operating on RDF data; maplib targets the analytics pipeline (Polars/Arrow) where pg_ripple
targets the operational database. The overlap makes maplib the best source of Rust API patterns
for bulk load and for returning structured query results efficiently.

### 2.2 Lessons

#### 2.2.1 OTTR template expansion as a first-class bulk-load primitive

OTTR (Reasonable Ontology Templates) defines parameterized RDF pattern templates. A template is a
named pattern with typed parameters; a usage is an instantiation of that template with concrete
argument rows. maplib expands an entire Polars DataFrame of argument rows against a template to
produce a batch of triples — without iterating row-by-row in the API layer.

Conceptually:

```
CREATE TEMPLATE :Person(?id IRI, ?name xsd:string, ?dob xsd:date)
AS {
    ?id rdf:type :Person .
    ?id foaf:name ?name .
    ?id :dateOfBirth ?dob .
}

-- Bulk expand against a SQL table:
SELECT pg_ripple.expand_template(
    :'http://example.org/Person',
    'SELECT iri(?id) AS id, name, dob FROM employees'
) AS triple_count;
```

Each row in the query result becomes three triples. The key advantage over iterating `INSERT TRIPLE`
calls from application code is that the template expansion can be entirely inside the SQL executor:
encode all argument values once via a batch SPI call, then bulk-insert into VP tables.

**Implication for `src/sparql/` or a new `src/template/` module (v0.8.0 Serialisation + Import)**:

Implement `pg_ripple.expand_template(template_iri TEXT, query TEXT) RETURNS BIGINT` as a
`#[pg_extern]` that:
1. Looks up the template definition in `_pg_ripple.templates (iri, pattern)`.
2. Executes `query` via `SpiClient::run`.
3. Encodes all distinct IRI/literal values in the result set with a single batch
   `ON CONFLICT DO NOTHING RETURNING` into `_pg_ripple.resources`.
4. Bulk-inserts the generated triples into the appropriate VP tables or `vp_rare`.

The template catalog table (`_pg_ripple.templates`) should store the OTTR pattern as a JSON
structure, not raw Turtle, so the Rust code can deserialise it without a full Turtle parser round-trip.

#### 2.2.2 Arrow RecordBatch as SPARQL result container

maplib returns SPARQL query results as Arrow `RecordBatch` objects transferred zero-copy from
Rust into Python via Arrow IPC. Each SPARQL variable maps to an Arrow column; each row is a
solution binding.

pg_ripple's current plan returns SPARQL results as PostgreSQL `SETOF RECORD` rows via SPI —
one `HeapTuple` allocation per row. For large result sets (millions of bindings) this is the
dominant overhead.

**Implication for a post-v1.0 optimization** (worth noting in the roadmap now):

The PostgreSQL `COPY TO` protocol can stream rows as raw binary. An Arrow IPC stream is a strict
superset of binary tabular data. Exposing `pg_ripple.sparql_to_arrow(query TEXT) RETURNS bytea`
that returns the full result set as an Arrow IPC buffer would allow Python/Rust clients using
`pyarrow`, `polars`, or `arrow-rs` to consume results without individual row deserialization.

The prerequisite is that pg_ripple's result decode path be refactored to produce `Vec<i64>` column
arrays (one per projected variable) rather than row-oriented `Vec<Datum>`. The i64 arrays can then
be dictionary-decoded in a single bulk `SELECT id, value FROM _pg_ripple.resources WHERE id = ANY($1)`
call, producing Arrow `Utf8Array` / `LargeUtf8Array` columns directly.

#### 2.2.3 Zero-copy batch dictionary decode

maplib decodes encoded integer IDs back to string form using Polars join operations against the
dictionary, which is itself a Polars DataFrame. The join is vectorized and runs in parallel across
CPU cores.

pg_ripple's decode path currently does individual `SPI_execute` calls. The vectorized equivalent
in PostgreSQL is:

```sql
SELECT r.id, r.value
FROM unnest($1::BIGINT[]) WITH ORDINALITY AS u(id, ord)
JOIN _pg_ripple.resources r ON r.id = u.id
ORDER BY u.ord;
```

This reduces decode SPI round-trips from O(n) to O(1) for a result set of n rows. The `WITH
ORDINALITY` preserves row order. Implementation belongs in `src/sparql/decode.rs`.

---

## 3. Stardog

**Website**: https://stardog.com  
**License**: Proprietary commercial (no source available)  
**What it is**: An enterprise knowledge graph platform. Stardog is notable for being the largest
purely commercial entrant in the SPARQL space with documented production deployments at pharma,
financial, and US government organisations. Key differentiators: query-time reasoning (not
materialization), multi-schema reasoning, virtual graphs (federated SPARQL without triple
materialization), and entity resolution.

**Source for this analysis**: public documentation at docs.stardog.com (accessed April 2025).

### 3.1 Relevance

Stardog's architecture represents the current state of the art in commercial SPARQL deployment.
Its `How does reasoning work?` documentation section contains an unusually frank comparison of
query-time reasoning (rewriting) vs. materialization that directly affects pg_ripple's design
decisions.

### 3.2 Lessons

#### 3.2.1 Query-time reasoning via query rewriting: the case for and against

Stardog's Blackout reasoner uses **query rewriting**: given a query `?x rdf:type :Person`, it
rewrites it to `UNION` of all known subclasses of `:Person`:

```
Distinct [#2]
└─ Projection(?person) [#2]
   └─ Union [#2]
      +─ Scan[POSC](?person, rdf:type, :Customer) [#1]
      └─ Scan[POSC](?person, rdf:type, :Employee) [#1]
```

No derived triples are stored; inference happens entirely in the query plan. Stardog's public docs
list four specific disadvantages of materialization that query rewriting avoids:
- **Data freshness**: rewriting requires no re-materialization after schema or data changes.
- **Data size blowup**: derived inferences are not stored.
- **Fixed schema**: rewriting supports multiple schemas per database; materialization locks to one.
- **Truth maintenance cost**: deletion propagation in materialized systems is expensive.

pg_ripple's datalog.md plans **both**: on-demand CTE mode (equivalent to query rewriting) and
materialised mode (via pg_trickle IVM). The on-demand CTE mode is correct and should be the
default. The RDFox section below shows why materialization can still be justified at scale.

**Implication**: The on-demand Datalog mode (strata compiled to `WITH RECURSIVE` CTEs attached to
each SPARQL query) should be the v0.9.0 delivery, as it avoids the truth-maintenance complexity.
The materialized mode should be explicitly labelled as an opt-in performance feature and not the
default, matching Stardog's documented rationale.

#### 3.2.2 Multiple named reasoning schemas (schema multi-tenancy)

Stardog supports multiple named schemas per database, each mapping to a set of named graphs:

```
$ stardog reasoning schema --list myDB
+----------------+----------------------------------+
| Schema         | Graphs                           |
+----------------+----------------------------------+
| default        | <tag:stardog:api:context:schema> |
| employeeSchema | :personGraph :employeeGraph      |
| customerSchema | :personGraph :customerGraph      |
+----------------+----------------------------------+
```

Each query can specify which schema to use with `--schema employeeSchema`. This is directly useful
for multi-tenant SaaS deployments where different tenants need different inference rules.

**Implication for `_pg_ripple.rule_sets`**: The rule catalog should support named rule sets, not
just a flat list of rules. Schema: `_pg_ripple.rule_sets (id BIGSERIAL, name TEXT UNIQUE, graph_ids BIGINT[])`.
The `graph_ids` column points to named graph identifiers in the dictionary; rules loaded from those
graphs form the named rule set. The `pg_ripple.sparql(query, rule_set := 'employeeSchema')` API
should then accept an optional rule set name, defaulting to `'default'`.

#### 3.2.3 Schema versioning via 64-bit hash for reasoner invalidation

Stardog 10 added schema versioning: a 64-bit hash is computed from the contents of each schema
graph. When a schema graph is updated (INSERT/DELETE to that named graph), the hash changes and
the reasoner's internal compiled representation is invalidated and recompiled on next use.

pg_ripple's on-demand Datalog CTE compiler caches the compiled SQL for each rule set. The cache
must be invalidated when the rule set changes. The Stardog mechanism is clean: hash the rule set
contents at compilation time, store the hash alongside the cached SQL, and recompile when the
hash changes at query time.

**Implication for `src/datalog/catalog.rs`**: Store compiled CTE SQL alongside a `rules_hash
BIGINT` in `_pg_ripple.compiled_rule_sets`. At query time, recompute the hash of the active rules
and compare. If different, recompile and update. The hash can be XXH3-128 truncated to 64 bits
(reusing the existing `xxhash-rust` dependency).

#### 3.2.4 Graph-level access control as the de-facto industry standard

Stardog enforces access control at the named graph level. Both Stardog and Virtuoso treat named
graphs as security principals — graph-level ACL is the observed industry norm.

pg_ripple can implement this more elegantly than any standalone triple store by leveraging
PostgreSQL's row-level security (RLS):

```sql
-- Policy: a user can only see triples in their allowed graphs
CREATE POLICY view_graph ON _pg_ripple.vp_7
    USING (g = ANY(pg_ripple.allowed_graph_ids()));
```

Where `pg_ripple.allowed_graph_ids()` is a `SECURITY DEFINER` function reading a
`_pg_ripple.graph_acl (role_name TEXT, graph_id BIGINT)` table. This delegates enforcement to
PostgreSQL's proven security implementation rather than requiring application-level filtering.

**This should be the primary access-control mechanism from v0.13.0 (Admin & Security), rather than
inventing a pg_ripple-specific ACL model.** The RLS approach also works transparently with
existing PostgreSQL tooling (pg_dump includes RLS policies; pgBouncer and connection poolers are
unaffected).

#### 3.2.5 Virtual graphs: federated SPARQL via FDW

Stardog's virtual graphs let users query a remote SQL database (Oracle, Postgres, SQL Server) or
CSV file as if it were a named RDF graph, without materializing any triples. The mapping from
relational to RDF is defined by R2RML or direct mapping rules.

The PostgreSQL-native equivalent is a Foreign Data Wrapper (FDW). A VP-shaped FDW over an
external SQL table:

```sql
CREATE FOREIGN TABLE _pg_ripple.vp_12345_remote (s BIGINT, o BIGINT, g BIGINT)
    SERVER remote_hr_db
    OPTIONS (table_name 'employee_graph_view');
```

would make the remote table appear as a named graph in SPARQL queries via `UNION ALL` with the
local VP tables. No triple materialization is needed.

This is a post-v1.0 feature but the FDW hook is the correct mechanism and should be noted in the
roadmap. The architecture is compatible with pg_ripple's predicate-based table lookup because the
FDW can be created for a specific predicate ID, making it a first-class member of the VP table
family.

---

## 4. GraphDB (Ontotext / Graphwise)

**Website**: https://graphdb.ontotext.com  
**License**: Proprietary (Free and Enterprise editions; no source available)  
**What it is**: A high-performance, enterprise-grade RDF database (originally OWLIM) built on the
RDF4J Java framework. Ontotext was acquired by Graphwise in 2023. Known for excellent SPARQL
compliance and tight integration with Lucene/Elasticsearch for full-text search.

**Source for this analysis**: GraphDB documentation (graphdb.ontotext.com/documentation, accessed
April 2025).

### 4.1 Relevance

GraphDB's most distinctive user-facing features — its FTS connectors, `onto:disable-sameAs` query
hints, and SPARQL explain plan — are directly implementable in pg_ripple with less effort than in
a standalone triple store, because PostgreSQL provides the infrastructure. The contrast is
instructive.

### 4.2 Lessons

#### 4.2.1 Full-text search via magic predicates vs. native `tsvector`

GraphDB integrates full-text search through "Lucene/Elasticsearch connectors". In SPARQL, FTS is
accessed via magic predicates:

```sparql
PREFIX luc: <http://www.ontotext.com/connectors/lucene#>
SELECT ?s WHERE {
    ?s luc:query "label:\"knowledge graph\" AND type:article" .
}
```

The connector intercepts the magic predicate and fires an external Lucene/Elasticsearch query,
then merges results back into the SPARQL result set.

pg_ripple can provide better FTS natively, because PostgreSQL's `tsvector`/`tsquery` is
built into the engine. There are two integration points:

1. **Dictionary FTS index**: Add `pg_ripple_fts tsvector GENERATED ALWAYS AS (to_tsvector('english', value)) STORED` to `_pg_ripple.resources`. A GIN index on that column allows literal-value text search directly in SQL.

2. **SPARQL FILTER extension function**: Expose a SPARQL extension function `bif:contains(literal, query)` (Virtuoso-compatible spelling) or `pg_ripple:fts(literal, query)` that translates to a `@@` operator against the dictionary FTS index in the generated SQL. Filter pushdown means the FTS lookup happens before the triple join, not after.

This is implementable from v0.4.0 (SPARQL Advanced) and avoids the complexity of an external
connector architecture.

#### 4.2.2 `onto:disable-sameAs` pseudo-graph as a query hint mechanism

GraphDB uses a special named graph IRI `<http://www.ontotext.com/disable-sameAs>` as a query
optimizer hint. Including this graph in a `FROM NAMED` clause disables `owl:sameAs` reasoning for
that query without a separate syntax extension. The hint is SPARQL-compatible and invisible to
non-GraphDB clients (they simply see it as a normal named graph name).

This is the right pattern for pg_ripple's query hint system. The reserved IRI prefix approach
planned in AGENTS.md is validated here:

```sparql
-- Disable inference for this query
SELECT ?s ?p ?o WHERE {
    FROM <http://pg-ripple.io/hints/no-inference>
    GRAPH ?g { ?s ?p ?o }
}

-- Force loop join (override planner)
SELECT ?s WHERE {
    FROM <http://pg-ripple.io/hints/join-order>
    { ?s :knows ?x . ?x :knows ?y }
}
```

The hint IRI is extracted during `src/sparql/algebrizer.rs` FROM clause processing and converted
into optimizer flags that are propagated through the `JoinPlan` to `src/sparql/emitter.rs`.

The reserved prefix should be `http://pg-ripple.io/hints/` (consistent with AGENTS.md). Define
at minimum:
- `no-inference` — skip all Datalog CTE injection for this query
- `join-order` — set `SET LOCAL join_collapse_limit = 1` for this query
- `explain` — return query plan instead of results (see next lesson)

#### 4.2.3 SPARQL explain endpoint for query profiling

GraphDB provides a `SPARQL EXPLAIN` variant (accessible via the query UI or HTTP API) that returns
the query plan rather than results. The plan shows estimated cardinalities, index choices, and join
order decisions — exactly the information a developer needs to debug a slow query.

pg_ripple should expose this as a SQL function from v0.12.0 (Performance):

```sql
-- Returns the PostgreSQL EXPLAIN ANALYZE output for the generated SQL
SELECT pg_ripple.sparql_explain(
    'SELECT ?s ?p WHERE { ?s :type :Person . ?s :name ?n }',
    analyze := true
);
```

Implementation: `sparql_explain` calls the same translation pipeline as `sparql_query` but wraps
the generated SQL in `EXPLAIN (ANALYZE, FORMAT JSON)`. The return value is the raw PostgreSQL
JSON plan. For v0.12.0 this is sufficient; a future version could parse the JSON plan and
synthesize a SPARQL-level explanation (join order, triple-pattern cardinalities) as a separate
function.

#### 4.2.4 Language-tag literal caching for `langMatches()` performance

GraphDB's `in-memory-literal-properties` configuration caches language-tagged literals in memory
to accelerate `langMatches()` filter performance. Without a cache, every `FILTER langMatches(?x,
"en")` requires scanning the dictionary for all literals with an `en` language tag.

pg_ripple's dictionary encodes IRIs and literals alike as `i64` IDs. Language-tagged literals are
stored as `"value"@lang` strings. The lookup problem is: given a language tag pattern, find all
dictionary IDs whose string form matches.

A secondary GIN index on a `lang_tag TEXT GENERATED ALWAYS AS (...)` column of `_pg_ripple.resources`
(extracting the `@lang` suffix) would make `langMatches()` a pure index scan rather than a
sequential dictionary scan. The expression:

```sql
CREATE INDEX resources_lang_tag ON _pg_ripple.resources (lang_tag)
    WHERE lang_tag IS NOT NULL;
```

can be added in v0.4.0 alongside the FTS index. The `lang_tag` extraction is a simple string
operation: everything after the last `@` in a literal string.

---

## 5. RDFox (Oxford Semantic Technologies, acquired by Samsung)

**Website**: https://oxfordsemantic.tech  
**License**: Proprietary commercial (no source available)  
**What it is**: An in-memory RDF triple store with the highest-performance incremental Datalog
reasoning engine in the industry (2–3 million inferences/second in published benchmarks). Developed
at the University of Oxford; commercialised by Oxford Semantic Technologies; acquired by Samsung
in 2024. Not on-disk like pg_ripple — RDFox trades persistence for reasoning throughput.

**Source for this analysis**: RDFox documentation v7.5 (docs.oxfordsemantic.tech, accessed April
2025).

### 5.1 Relevance

RDFox's public documentation is unusually detailed about its reasoning algorithms. It is the
best-documented example of incremental materialization-based reasoning with support for deletion,
which is the hardest part of pg_ripple's planned Datalog materialized mode.

### 5.2 Lessons

#### 5.2.1 Incremental reasoning: Addition, Deletion, and BwdChain phases

RDFox implements materialization-based reasoning. When triples are added or deleted, it does
**not** recompute the full materialization from scratch. Instead, it uses three phases that the
reasoning profiler reports explicitly:

- **Addition**: Apply rules to the delta of newly added triples — derive only consequences of the
  new triples, not of the entire dataset.
- **Deletion**: Identify derived triples that are no longer supported after a source triple is
  deleted.
- **BwdChain (backward chaining)**: For deleted derived triples, use backward chaining to determine
  whether any remaining derivation supports the fact. If yes, the fact survives. If no, it is
  retracted.

The BwdChain phase is the key insight. Without it, deletion requires full re-materialization.
With it, deletion is O(affected-rules × affected-triples), not O(all-rules × all-triples).

**Implication for `src/datalog/` materialized mode** (v0.9.0+):

When a triple is deleted:
1. Query the rule catalog for all rule heads that could have derived it (rules where the deleted
   fact pattern matches the head atom).
2. For each such rule, re-evaluate the rule body against the current dataset excluding the deleted
   fact. If the body still holds (via a different derivation path — the "BwdChain" check), the
   derived fact survives.
3. If no derivation survives, delete the derived fact and recurse (in case derived-from-derived
   chains exist).

This is expensive enough that pg_ripple's materialized mode should be optional and explicitly
documented as not supporting high-frequency individual-triple deletes. The on-demand CTE mode
does not have this limitation and should be the default.

#### 5.2.2 Distinguishing explicit from derived facts in queries

RDFox provides `query.fact-domain explicit` to query only explicitly asserted facts, ignoring all
derived triples. This is the RDFox equivalent of querying the base table in a materialized view
scenario.

pg_ripple should implement this as a SPARQL hint:

```sparql
SELECT ?s WHERE {
    FROM <http://pg-ripple.io/hints/explicit-only>
    { ?s rdf:type :Person }
}
```

Or as a SQL function parameter: `pg_ripple.sparql(query, include_derived := false)`.

The storage mechanism: if VP tables carry a `source SMALLINT` column (0 = explicit, rule_id > 0
= derived by rule N), then `include_derived := false` adds a `WHERE source = 0` filter to every
VP table scan in the generated SQL. This is a single-column predicate on an integer column —
negligible overhead with a partial index `CREATE INDEX ON vp_xxx (s, o, g) WHERE source = 0`.

Note: adding `source` to VP tables changes the schema. This should be planned for v0.9.0
alongside the materialized Datalog delivery, not retrofitted.

#### 5.2.3 Multi-head rules and named-graph heads

RDFox supports rules with multiple head atoms that write to different named graphs:

```
:monthlyPayment[?id, ?m] :Payroll :-
    [?id, rdf:type, :Employee],
    :yearlySalary[?id, ?s] :HR,
    BIND(?s / 12 AS ?m) .
```

This rule reads from the `:HR` named graph and writes to `:Payroll`, joining across named graphs in
a single rule. Each head atom can target a different named graph.

pg_ripple's rule IR in `src/datalog/` should support multi-head rules from the start. The `Rule`
struct should hold `Vec<HeadAtom>`, not a single `HeadAtom`, and each `HeadAtom` should carry an
optional graph ID binding (defaulting to the default graph, ID 0). The SQL emitter generates one
`INSERT INTO vp_{id} (s, o, g)` per head atom, all within the same transaction.

#### 5.2.4 AGGREGATE body formulas in rules

RDFox's rule language includes `AGGREGATE` as a body formula:

```
[?d, :deptAvgSalary, ?z] :-
    [?d, rdf:type, :Department],
    AGGREGATE(
        [?x, :worksFor, ?d],
        [?x, :salary, ?s]
        ON ?d
        BIND AVG(?s) AS ?z) .
```

The `AGGREGATE ... ON ?groupVar BIND agg(?v) AS ?result` syntax is more expressive than SQL's
`GROUP BY` because the aggregation scope is explicit in the rule body rather than implied by
grouping all non-aggregate projections.

**Implication for `src/datalog/parser.rs`**: The Datalog rule parser should support an `AGGREGATE`
body formula with explicit `ON` grouping variables and `BIND aggfn(...) AS ?var` bindings. The
SQL compilation maps this to:

```sql
WITH agg_cte AS (
    SELECT ?d, AVG(?s) AS ?z
    FROM ...
    GROUP BY ?d
)
SELECT ?d, ?z FROM agg_cte
```

This is the same lesson from Logica (see `HeadAtom::Aggregate`) but from a different angle: it
needs to be expressible in the **rule body**, not just in the head. Rules that compute statistics
(department average salary, subgraph density, triple count per predicate) require this.

#### 5.2.5 The Tuple Table abstraction: a uniform interface over storage

RDFox's "tuple table" is a virtual table interface. The same atom syntax in a rule body can refer
to:
- In-memory RDF triples (the default graph or a named graph)
- External data sources (CSV, relational database via JDBC)
- Built-in tables (e.g., `SKOLEM` for blank node generation)

All three are addressed the same way in rules. This is RDFox's equivalent of PostgreSQL's
**Foreign Data Wrapper** plus **materialized/regular tables** accessed via a uniform SQL interface.

pg_ripple already achieves this implicitly:
- VP tables and `vp_rare` = in-memory quads (PostgreSQL heap tables)
- FDW-backed VP tables = external data sources (see Stardog virtual graphs section)
- `_pg_ripple.resources` = built-in lookup table

The missing piece is making the **SPARQL-layer** treat FDW-backed VP tables transparently. The
predicate catalog (`_pg_ripple.predicates`) already maps predicate IDs to `table_oid`. An FDW
table has a valid OID; the SPARQL SQL emitter needs no special case — it queries by OID like any
other. The transparency falls out naturally from the existing architecture.

**Document this design property explicitly in the implementation plan**: foreign VP tables registered
via `pg_ripple.register_foreign_graph(predicate_iri, server_name, remote_table)` will appear
automatically in SPARQL queries. No query layer changes are required.

---

## 6. Summary: Changes to pg_ripple Architecture

| Source | Lesson | Target module | Roadmap version |
|---|---|---|---|
| Virtuoso | Add `(g, p, s, o)` index to `vp_rare` for graph-drop performance | `src/storage/` migration | v0.1.0 / v0.5.0 |
| Virtuoso | `<http://pg-ripple.io/hints/join-order>` hint → `SET LOCAL join_collapse_limit=1` | `src/sparql/emitter.rs` | v0.12.0 |
| Virtuoso | GUC `pg_ripple.named_graph_optimized` adds G-leading index to all VP tables | `src/admin/` | v0.13.0 |
| maplib | `pg_ripple.expand_template(iri, query)` for OTTR-style DataFrame→RDF bulk load | `src/template/` (new) | v0.8.0 |
| maplib | Batch dictionary decode via `unnest($ids) JOIN resources` in `decode.rs` | `src/sparql/decode.rs` | v0.3.0 |
| maplib | Columnar Arrow IPC result stream `sparql_to_arrow()` | post-v1.0 roadmap | post-v1.0 |
| Stardog | On-demand Datalog CTE mode as default; materialized mode as opt-in | `src/datalog/compiler.rs` | v0.9.0 |
| Stardog | Named rule sets in `_pg_ripple.rule_sets (name, graph_ids[])` catalog | `src/datalog/catalog.rs` | v0.9.0 |
| Stardog | Rule set cache keyed on XXH3-64 hash of compiled rules | `src/datalog/catalog.rs` | v0.9.0 |
| Stardog | Graph-level ACL via PostgreSQL RLS on `g` column | `src/admin/` | v0.13.0 |
| Stardog | FDW-backed VP tables as virtual named graphs | `src/admin/` | post-v1.0 |
| GraphDB | FTS via GIN index on `_pg_ripple.resources.lang_tag` + `bif:contains` filter fn | `src/sparql/`, migration | v0.4.0 |
| GraphDB | `<http://pg-ripple.io/hints/no-inference>` FROM hint | `src/sparql/algebrizer.rs` | v0.9.0 |
| GraphDB | `pg_ripple.sparql_explain(query, analyze)` wraps generated SQL in EXPLAIN | `src/sparql/mod.rs` | v0.12.0 |
| RDFox | Addition/Deletion/BwdChain phases for incremental materialization | `src/datalog/materializer.rs` (new) | v0.9.0 |
| RDFox | `source SMALLINT` column on VP tables (0=explicit, rule_id=derived) | `src/storage/` schema | v0.9.0 |
| RDFox | `include_derived := false` param and `explicit-only` hint | `src/sparql/mod.rs` | v0.9.0 |
| RDFox | Multi-head rules: `Vec<HeadAtom>` in Rule IR, each with optional graph ID | `src/datalog/plan.rs` | v0.9.0 |
| RDFox | `AGGREGATE ... ON ?g BIND aggfn(...) AS ?v` body formula in rule syntax | `src/datalog/parser.rs` | v0.9.0 |
