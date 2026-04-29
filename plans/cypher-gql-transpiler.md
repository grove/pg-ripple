# Cypher / GQL Transpiler — Detailed Implementation Plan

**Status:** Planning  
**Scope:** Beyond v1.1 — full openCypher 9 conformance, GQL conformance,
mixed Cypher/SPARQL transactions, indexing hints, APOC compatibility shim  
**References:**
- [future-directions.md §B.1](future-directions.md) — strategic framing  
- [cypher.md](cypher.md) — ADR (Cypher-to-SPARQL rewrite)  
- [cypher/transpilation_reassessment.md](cypher/transpilation_reassessment.md) — lessons from initial transpiler work  
- [cypher/query_language_landscape.md](cypher/query_language_landscape.md) — language comparison  
- [cypher/prior_art_graph_systems.md](cypher/prior_art_graph_systems.md) — 8-system architectural survey  
- [cypher/cypher_lpg_analysis.md](cypher/cypher_lpg_analysis.md) — VP table / LPG alignment  

---

## 0. Executive Summary

v1.1 delivers a limited Cypher pattern-matching subset via transpilation to
SPARQL algebra. This plan covers the **five post-v1.1 work items** identified
in future-directions.md §B.1:

| # | Deliverable | Architecture | Estimated effort |
|---|---|---|---|
| 1 | Full openCypher 9 conformance (TCK CI gate) | Native Cypher→SQL engine | 14–18 person-weeks |
| 2 | GQL conformance (ISO/IEC 39075:2024) | Grammar extension of the Cypher engine | 8–12 person-weeks |
| 3 | Mixed Cypher/SPARQL transactions | Shared SPI transaction context | 3–4 person-weeks |
| 4 | Cypher-native indexing hints | Plan hint annotations | 2–3 person-weeks |
| 5 | APOC compatibility shim | `pg_ripple_apoc.*` SQL functions | 10–14 person-weeks |

**Total: 37–51 person-weeks** (9–13 person-months), delivered across 3 releases.

### Key architectural decision

The transpilation reassessment (see reference above) established that a static
Cypher→SPARQL→SQL transpiler hits a ~98% TCK compliance ceiling due to
irreducible semantic mismatches (null propagation, variable scoping, multigraph
path traversal, runtime path constraints). To reach full openCypher 9
conformance, this plan adopts **Option B from the reassessment: a native
Cypher→SQL execution engine** that shares storage (VP tables, dictionary, HTAP)
and infrastructure with the SPARQL engine but has its own algebra→SQL compiler.

This is the same architecture used by Apache AGE (Cypher→PostgreSQL plan trees),
Kuzu (Cypher→columnar execution), and Stardog's deprecated-then-rebuilt
approach. The prior art uniformly validates that SPARQL-mediated transpilation
cannot achieve full Cypher compliance; every system that tried eventually built
a native engine.

---

## 1. Full openCypher 9 Conformance

### 1.1 Scope

openCypher 9 is the last standalone release of the openCypher specification
before the project merged into the GQL standards track. The openCypher TCK
(Technology Compatibility Kit) contains ~2,000 Gherkin/Cucumber scenarios
organised into three feature directories:

- `clauses/` — MATCH, WHERE, RETURN, WITH, CREATE, MERGE, SET, REMOVE, DELETE,
  UNWIND, CALL, FOREACH, UNION
- `expressions/` — literals, operators, functions, list comprehensions, CASE,
  pattern expressions, type coercions
- `useCases/` — end-to-end scenario tests

**Target: ≥95% TCK pass rate as a required CI gate; 100% as aspirational.**

The two provably-impossible-via-transpilation scenarios (Match4[8]: runtime path
constraints; Match6[14]: multigraph parallel edge traversal) become solvable
with a native engine because:

- Runtime path constraints → multi-phase execution: resolve the variable first,
  then construct the recursive CTE dynamically.
- Multigraph path traversal → row-level VP table traversal instead of
  SPARQL-algebra set-based traversal.

### 1.2 Architecture: `cypher-algebra` crate + native SQL compiler

#### 1.2.1 Standalone `cypher-algebra` crate

A new standalone crate (publishable to crates.io) modelled on `spargebra`:

```
crates/cypher-algebra/
    src/
        grammar.rs        — pest/winnow grammar for openCypher 9 + GQL extensions
        lexer.rs          — token stream (keywords, identifiers, literals, operators)
        ast.rs            — concrete syntax tree (1:1 with grammar productions)
        algebra.rs        — normalized Cypher algebra IR:
                              CypherQuery, Match, Create, Merge, Set, Remove, Delete,
                              Return, With, Unwind, Union, Call, Foreach,
                              PatternExpr, PathPattern, ShortestPath,
                              Expression, Literal, FunctionCall, ListComprehension
        normalize.rs      — AST → algebra lowering (scope resolution, name binding)
        semantic_check.rs — type checking, scope validation, deprecation warnings
        error.rs          — CypherParseError, CypherSemanticError
    tests/
        tck_adapter.rs    — Cucumber step definitions for the openCypher TCK
        parser_tests.rs   — unit tests for grammar coverage
```

**Parser technology choice:** `winnow` (Rust, zero-copy, excellent error
messages). Alternative considered: `pest` (PEG grammar, used by `lora-parser`).
`winnow` is preferred because it produces an AST directly without a separate
CST→AST pass, and it handles left-recursion through explicit combinator
patterns — important for Cypher's `WHERE` expression grammar which is heavily
left-recursive.

**Why not reuse an existing crate:** No Rust crate fills this role (confirmed
April 2026 survey — see [cypher_lpg_analysis.md §5](cypher/cypher_lpg_analysis.md)).
`drasi-query-cypher` is hardwired to Microsoft Drasi's runtime types;
`open-cypher` is abandoned (3 years stale); `lora-parser`, `plexus-parser`,
`sparrowdb-cypher` are all tightly coupled to their respective engines.

**Effort: 6–8 person-weeks** for the parser + algebra + TCK adapter.

#### 1.2.2 Native Cypher→SQL compiler (`src/cypher/`)

A new module tree inside the pg_ripple extension:

```
src/cypher/
    mod.rs               — public API: pg_ripple.cypher(query TEXT) → SETOF RECORD
    translator.rs        — CypherAlgebra → SQL string generation
    writer.rs            — CREATE/MERGE/SET/REMOVE/DELETE → VP table DML
    path_compiler.rs     — variable-length paths → WITH RECURSIVE ... CYCLE
    shortest_path.rs     — shortestPath/allShortestPaths → bidirectional BFS CTE
    merge_compiler.rs    — MERGE → ON CONFLICT / advisory lock patterns
    expression.rs        — Cypher expression → SQL expression (null semantics, type coercions)
    function_registry.rs — Cypher built-in functions → SQL/pg_ripple equivalents
    explain.rs           — EXPLAIN CYPHER → annotated plan output
    plan_cache.rs        — prepared statement cache (keyed on Cypher text hash)
```

**Design principles:**

1. **VP table SQL directly** — the compiler emits SQL that joins VP tables using
   the dictionary-encoded integer join pattern, identical to what
   `src/sparql/sqlgen.rs` produces. It does NOT go through SPARQL algebra.

2. **Cypher null semantics natively** — Cypher's three-valued logic (`true`,
   `false`, `null`) with null-propagating operators compiles to SQL `CASE`
   expressions and `COALESCE` wrappers. No attempt to map to SPARQL's different
   unbound-variable semantics.

3. **Row-level edge traversal** — variable-length path compilation uses
   `WITH RECURSIVE` over VP table rows (not SPARQL property path set semantics),
   preserving edge multiplicity for multigraph workloads.

4. **Multi-phase execution** — when a path constraint depends on a runtime
   variable, the compiler emits two SPI calls: one to resolve the variable, one
   to execute the parameterised recursive CTE.

5. **Shared infrastructure** — dictionary encode/decode, VP table lookup,
   HTAP write path, merge worker, SHACL validation, RDF-star edge properties
   are all reused from the existing codebase. No duplication.

**Shared components (zero new code):**

| Component | Module |
|---|---|
| Dictionary encode/decode | `src/dictionary/` |
| VP table OID lookup | `src/storage/predicates.rs` |
| HTAP write path | `src/storage/insert_triple()` |
| Merge worker | `src/storage/merge.rs` |
| SHACL validation | `src/shacl/` |
| RDF-star (edge properties) | `src/dictionary/rdf_star.rs` |
| Batch decode | `src/sparql/decode.rs` |
| Plan cache infrastructure | `src/sparql/plan.rs` (pattern reuse) |

**New code unique to Cypher:**

| Component | Reason it cannot be shared with SPARQL |
|---|---|
| `expression.rs` | Cypher null propagation ≠ SPARQL UNDEF |
| `path_compiler.rs` | Row-level traversal ≠ SPARQL property path set semantics |
| `shortest_path.rs` | No SPARQL equivalent |
| `merge_compiler.rs` | MERGE upsert has no SPARQL equivalent |
| `function_registry.rs` | Cypher built-in functions differ from SPARQL |

**Effort: 8–10 person-weeks** for the full native compiler.

### 1.3 openCypher TCK integration

The TCK is a set of Cucumber `.feature` files (Apache 2.0 licensed). Integration
into pg_ripple's CI:

1. **TCK runner**: A Rust binary (`tests/cypher_tck/`) that:
   - Parses Cucumber feature files using the `cucumber` Rust crate
   - Sets up a pg_ripple test database (via `cargo pgrx test` harness)
   - Executes `Given` steps (graph setup via `pg_ripple.cypher()` CREATE statements)
   - Executes `When` steps (query via `pg_ripple.cypher()`)
   - Asserts `Then` steps (result comparison with TCK expected output format)
   - Reports pass/fail/skip per scenario

2. **CI gate configuration**:
   - **Required**: ≥95% pass rate (fail the build below this threshold)
   - **Informational**: Full pass/fail report uploaded as CI artifact
   - **Tracking**: Pass rate trend line in CI dashboard (similar to W3C SPARQL suite)

3. **TCK scenario classification**:
   - **Green**: passes, verified
   - **Yellow**: passes, but edge-case coverage is fragile
   - **Red**: known failure, documented with issue link
   - **Skip**: intentionally unsupported feature (documented)

**Effort: 2–3 person-weeks** (TCK runner + CI integration).

### 1.4 Clause-by-clause implementation plan

| Clause | TCK features | Complexity | Dependencies | Est. weeks |
|---|---|---|---|---|
| `MATCH` (basic patterns) | `clauses/Match1-7` | Medium | `cypher-algebra` parser | 1.5 |
| `WHERE` (filters) | `clauses/Where1-4` | Medium | `expression.rs` | 1.0 |
| `RETURN` / `WITH` | `clauses/Return1-7`, `With1-4` | Medium | Scope management | 1.0 |
| `ORDER BY` / `SKIP` / `LIMIT` | `clauses/OrderBy1-4`, `Skip1-2`, `Limit1-2` | Low | — | 0.5 |
| `OPTIONAL MATCH` | `clauses/OptionalMatch1-3` | Medium | LEFT JOIN emission | 0.5 |
| `CREATE` | `clauses/Create1-3` | Medium | `writer.rs`, dictionary | 1.0 |
| `DELETE` / `DETACH DELETE` | `clauses/Delete1-5` | Medium | `writer.rs`, cascade | 1.0 |
| `SET` / `REMOVE` | `clauses/Set1-4`, `Remove1-2` | Medium | `writer.rs` | 0.5 |
| `MERGE` | `clauses/Merge1-6` | High | `merge_compiler.rs`, advisory locks | 1.5 |
| `UNWIND` | `clauses/Unwind1` | Low | LATERAL unnest | 0.5 |
| `UNION` / `UNION ALL` | `clauses/Union1-2` | Low | — | 0.3 |
| `FOREACH` | `clauses/Foreach1` | Medium | Loop-to-DML compilation | 0.5 |
| Variable-length paths | `clauses/Match4` (partial) | High | `path_compiler.rs` | 1.5 |
| `shortestPath` | `expressions/ShortestPath1-3` | High | `shortest_path.rs` | 1.5 |
| Expressions & functions | `expressions/*` | Medium | `expression.rs`, `function_registry.rs` | 2.0 |
| List comprehensions | `expressions/List1-4` | High | LATERAL + subquery | 1.0 |
| Pattern expressions | `expressions/Pattern1-2` | Medium | EXISTS subquery | 0.5 |
| Type coercions | `expressions/Type1-3` | Low | CAST mapping | 0.5 |

### 1.5 Cypher function mapping

Core Cypher built-in functions and their pg_ripple SQL equivalents:

| Cypher function | SQL / pg_ripple equivalent |
|---|---|
| `id(n)` | Subject IRI (dictionary decode of `s` column) |
| `labels(n)` | `SELECT o FROM vp_rdf_type WHERE s = n_id` (array agg) |
| `type(r)` | Predicate IRI (from `_pg_ripple.predicates` catalog) |
| `properties(n)` | JSON object from all VP table rows where `s = n_id` |
| `keys(n)` | Predicate IRIs for VP tables containing `s = n_id` |
| `size(list)` | `array_length(list, 1)` or `char_length(str)` |
| `length(path)` | Recursive CTE depth counter |
| `nodes(path)` | Array of subject IDs from path traversal |
| `relationships(path)` | Array of edge tuples from path traversal |
| `head(list)` | `list[1]` |
| `last(list)` | `list[array_length(list, 1)]` |
| `tail(list)` | `list[2:]` |
| `range(start, end)` | `generate_series(start, end)` |
| `coalesce(a, b, ...)` | `COALESCE(a, b, ...)` |
| `timestamp()` | `extract(epoch FROM now()) * 1000` |
| `toInteger(x)` | `CAST(x AS BIGINT)` |
| `toFloat(x)` | `CAST(x AS DOUBLE PRECISION)` |
| `toString(x)` | Dictionary decode (for IDs) or `CAST(x AS TEXT)` |
| `toBoolean(x)` | `CAST(x AS BOOLEAN)` |
| `exists(n.prop)` | `EXISTS (SELECT 1 FROM vp_prop WHERE s = n_id)` |
| `count(*)` | `COUNT(*)` |
| `collect(x)` | `array_agg(x)` |
| `sum/avg/min/max` | Direct SQL aggregates |
| String functions | PostgreSQL string functions (upper, lower, trim, replace, etc.) |
| Math functions | PostgreSQL math functions (abs, ceil, floor, round, sign, etc.) |
| `startNode(r)` / `endNode(r)` | `s` / `o` columns of the VP table row |

---

## 2. GQL Conformance (ISO/IEC 39075:2024)

### 2.1 Scope

GQL (ISO/IEC 39075:2024, published April 2024, 610 pages) is the ISO standard
for property graph queries. It absorbs and standardizes openCypher, PGQL
(Oracle), and G-CORE (LDBC). The openCypher project has stopped independent
releases and defers to GQL going forward.

**Relationship to openCypher**: GQL's Graph Pattern Matching (GPM) layer is
essentially identical to openCypher for the `MATCH … WHERE … RETURN` core.
A parser that handles GQL grammar accepts all openCypher queries for the GPM
subset (~90% of practical workloads).

**Key divergences from openCypher 9:**

| Feature | openCypher 9 | GQL | Implementation impact |
|---|---|---|---|
| Mutation syntax | `CREATE` | `INSERT` | Parse both; same algebra node |
| Quantified path patterns | `[*m..n]` | `{m,n}` (formal GPM) | Grammar branch; same CTE shape |
| `NEXT` clause | Not present | Sequential step composition | New algebra operator |
| `GRAPH TYPE` DDL | N/A | Formal graph schema | New catalog + DDL handler |
| Multi-graph queries | `USE` extension | `MATCH FROM graph` built-in | Named graph routing |
| Composite queries | `UNION` only | `UNION`, `INTERSECT`, `EXCEPT` | New set operators |
| Element identity | Implicit | Mandatory element IDs | Already satisfied (dictionary IDs) |
| Path modes | Not specified | `WALK`, `TRAIL`, `SIMPLE`, `ACYCLIC` | New CTE shapes per mode |

### 2.2 Implementation plan

#### Phase 1: GQL grammar extension (3–4 person-weeks)

Extend the `cypher-algebra` crate grammar to accept GQL syntax:

- `INSERT` as alias for `CREATE`
- `{m,n}` quantified path patterns alongside `[*m..n]`
- `NEXT` clause → sequential algebra composition
- `MATCH ... FROM graph_name` → named graph routing
- `INTERSECT` / `EXCEPT` set operations
- Path mode annotations: `WALK`, `TRAIL`, `SIMPLE`, `ACYCLIC`

The algebra IR is the same for both dialects — only the parser has two branches.

#### Phase 2: GQL-specific operators (3–5 person-weeks)

- **`GRAPH TYPE` DDL**: Map to a catalog table `_pg_ripple.graph_types` that
  records node types, edge types, and property schemas. This is the LPG
  equivalent of SHACL shapes — and can reuse SHACL's validation infrastructure.

- **Path modes**: Each mode compiles to a different recursive CTE shape:
  - `WALK` — unrestricted traversal (may revisit nodes and edges)
  - `TRAIL` — no edge repeated (track edge IDs in the `CYCLE` clause)
  - `SIMPLE` — no node repeated (track node IDs)
  - `ACYCLIC` — no node repeated, no return to start (SIMPLE + start ≠ end check)

- **`NEXT` clause**: Sequential composition of two graph pattern matching
  operations. The output of the first becomes the driving table of the second.
  Compiles to a CTE chain: `WITH step1 AS (...), step2 AS (SELECT ... FROM step1 ...)`.

- **Element identity**: GQL mandates that each node/edge has a stable identity.
  pg_ripple's dictionary IDs satisfy this — an IRI is a stable, globally-unique
  node identity. Edges are identified by `(s, p, o, i)` where `i` is the
  statement ID from the shared sequence.

#### Phase 3: GQL conformance suite (2–3 person-weeks)

The GQL formal test suite is still under development by ISO/IEC JTC 1/SC 32 WG3.
As of April 2026, no public conformance kit comparable to the openCypher TCK has
been released. The first Technical Corrigendum (TC1) is scheduled for December
2025.

**Strategy:**
- Track the GQL conformance suite as it emerges
- Build internal test scenarios based on the GQL specification examples
- Target ≥90% compliance on the GQL test suite when it becomes available
- Publish compliance reports (informational, not blocking CI) until the suite
  stabilises

**Effort: 8–12 person-weeks total** for all three phases.

### 2.3 SQL/PGQ tracking

SQL:2023 Part 16 (SQL/PGQ) embeds GQL's GPM inside SQL via `GRAPH_TABLE()`:

```sql
SELECT person_name, friend_name
FROM GRAPH_TABLE(
    social_graph
    MATCH (p:Person)-[:KNOWS]->(f:Person)
    COLUMNS (p.name AS person_name, f.name AS friend_name)
);
```

PostgreSQL does not yet implement SQL/PGQ. When it does, pg_ripple's VP tables
should expose themselves as `GRAPH_TABLE()`-compatible views. This is a future
integration point, not a deliverable of this plan. **Track PG19/PG20 release
notes for SQL/PGQ progress.**

---

## 3. Mixed Cypher/SPARQL Transactions

### 3.1 Motivation

Since both Cypher and SPARQL compile to the same VP table SQL and share the same
dictionary, a single SQL function call can contain both query languages operating
on the same data within one PostgreSQL transaction. This is trivially expressible:

```sql
BEGIN;
-- Cypher: create some nodes
SELECT pg_ripple.cypher('CREATE (a:Person {name: "Alice"})-[:KNOWS]->(b:Person {name: "Bob"})');
-- SPARQL: query the result
SELECT * FROM pg_ripple.sparql('SELECT ?name WHERE { ?p a <ex:Person> ; <ex:name> ?name }');
-- Both see the same transaction snapshot
COMMIT;
```

### 3.2 Implementation

**Already works at the SQL level** — both functions use SPI within the same
backend, sharing the same transaction context. No special coordination is
needed.

What IS needed:

1. **A combined function** for atomic mixed-language queries:

   ```sql
   SELECT pg_ripple.graph_query($$
       CYPHER { MATCH (p:Person)-[:KNOWS]->(f) RETURN f.name AS friend }
       SPARQL { SELECT ?label WHERE { ?f rdfs:label ?label } }
       JOIN ON friend = ?f
   $$);
   ```

   This requires a mini-parser for the combined syntax, splitting the input into
   Cypher and SPARQL fragments, compiling each independently, and joining the
   result sets via a SQL JOIN in a wrapping CTE.

2. **HTTP endpoint support**: The `pg_ripple_http` service should accept a
   content type (`application/x-cypher-query` or `application/sparql-query`)
   and route to the correct compiler. Mixed queries use a new content type
   (`application/x-graph-query`).

3. **Explain integration**: `EXPLAIN` on a mixed query shows both the Cypher
   plan and the SPARQL plan, with the join point annotated.

**Effort: 3–4 person-weeks.**

---

## 4. Cypher-Native Indexing Hints

### 4.1 Motivation

Neo4j applications use Cypher hint comments to influence the planner:

```cypher
MATCH (n:Person)
USING INDEX n:Person(name)
WHERE n.name = 'Alice'
RETURN n
```

When migrating from Neo4j, users expect these hints to be understood.

### 4.2 Implementation

Cypher indexing hints map to PostgreSQL plan hints via the `pg_hint_plan`
extension (if available) or pg_ripple's own hint annotation system:

| Cypher hint | pg_ripple action |
|---|---|
| `USING INDEX n:Label(prop)` | Emit `/*+ IndexScan(vp_prop vp_prop_so_idx) */` |
| `USING SCAN n:Label` | Emit `/*+ SeqScan(vp_rdf_type) */` |
| `USING JOIN ON n` | Emit `/*+ NestLoop(...) */` or `/*+ HashJoin(...) */` |

**Implementation steps:**

1. Parse `USING INDEX`, `USING SCAN`, `USING JOIN ON` in the `cypher-algebra`
   grammar as plan hint annotations on the algebra IR.

2. In `translator.rs`, propagate hint annotations to the generated SQL as
   `pg_hint_plan`-compatible comments when `pg_hint_plan` is installed, or as
   `SET LOCAL` GUC overrides (e.g., `enable_seqscan = off`) when it is not.

3. Log a `NOTICE` when a hint references a non-existent index or label.

4. Document the mapping table so Neo4j users know what to expect.

**Effort: 2–3 person-weeks.**

---

## 5. APOC Compatibility Shim

### 5.1 Motivation

A surprisingly large fraction of Neo4j applications depend on APOC (Awesome
Procedures on Cypher) — Neo4j's de facto standard library of ~450 procedures
and functions. Users migrating from Neo4j often discover that their application
is "really an APOC application" with Cypher serving mainly as the glue language.

The full APOC library is too large to implement comprehensively. This plan
targets the **30 most-used APOC procedures** (based on Neo4j community surveys
and GitHub usage analysis), which cover an estimated 80% of real-world APOC
usage.

### 5.2 Prioritised APOC procedures

#### Tier 1: High-impact, straightforward mapping (4–5 person-weeks)

| APOC procedure | pg_ripple implementation | Complexity |
|---|---|---|
| `apoc.path.expand()` | Variable-length VP table traversal with label/relationship filters | Medium |
| `apoc.path.subgraphAll()` | BFS traversal returning all reachable nodes + relationships | Medium |
| `apoc.path.subgraphNodes()` | BFS traversal returning only reachable nodes | Low |
| `apoc.path.spanningTree()` | BFS traversal with no repeated nodes (SIMPLE path mode) | Medium |
| `apoc.neighbors.byhop()` | k-hop neighborhood query via recursive CTE | Low |
| `apoc.neighbors.athop()` | Single-hop neighborhood | Low |
| `apoc.convert.toJson()` | `pg_ripple.export_jsonld()` or `row_to_json()` | Low |
| `apoc.convert.fromJsonMap()` | `jsonb_to_record()` + dictionary encode | Low |
| `apoc.convert.fromJsonList()` | `jsonb_array_elements()` + dictionary encode | Low |
| `apoc.create.uuid()` | `gen_random_uuid()` (PG built-in) | Trivial |
| `apoc.text.join()` | `string_agg()` | Trivial |
| `apoc.coll.flatten()` | Custom unnest + re-aggregate | Low |
| `apoc.coll.toSet()` | `array_agg(DISTINCT ...)` | Trivial |
| `apoc.map.fromPairs()` | `jsonb_object_agg()` | Low |
| `apoc.map.merge()` | `jsonb_concat(a, b)` or `a || b` | Trivial |

#### Tier 2: Medium-impact, moderate complexity (3–5 person-weeks)

| APOC procedure | pg_ripple implementation | Complexity |
|---|---|---|
| `apoc.load.json()` | `pg_ripple.load_json()` (HTTP fetch + JSON-LD parse) | Medium |
| `apoc.load.csv()` | `pg_ripple.bulk_load()` with CSV adapter | Medium |
| `apoc.periodic.iterate()` | `pg_ripple.sparql_update()` in batched loop | Medium |
| `apoc.periodic.commit()` | Batch commit wrapper using advisory locks | Medium |
| `apoc.refactor.mergeNodes()` | `pg_ripple.merge_subjects()` (owl:sameAs canonicalisation) | Medium |
| `apoc.refactor.rename.type()` | Rename predicate: create new VP table, copy rows, drop old | High |
| `apoc.algo.dijkstra()` | Weighted shortest path via recursive CTE + cost accumulator | High |
| `apoc.algo.allSimplePaths()` | Recursive CTE with SIMPLE path mode (no node repeat) | Medium |
| `apoc.date.parse()` / `format()` | `to_timestamp()` / `to_char()` | Low |
| `apoc.export.json.all()` | `pg_ripple.export_jsonld()` | Low |
| `apoc.export.csv.all()` | `COPY (SELECT ...) TO STDOUT WITH CSV` wrapper | Low |
| `apoc.meta.graph()` | Schema introspection from `_pg_ripple.predicates` + `vp_rdf_type` | Medium |
| `apoc.meta.schema()` | Schema introspection with property type sampling | Medium |
| `apoc.schema.assert()` | Compare expected schema against actual VP table structure | Medium |

#### Tier 3: Lower-priority, deferred (3–4 person-weeks, future release)

| APOC procedure | Notes |
|---|---|
| `apoc.trigger.add()` | Map to PostgreSQL triggers on VP tables |
| `apoc.custom.asProcedure()` | Map to `CREATE FUNCTION` wrapper |
| `apoc.lock.all()` | `pg_advisory_lock()` wrapper |
| `apoc.nodes.group()` | Graph summarisation / node collapsing |
| `apoc.graph.fromDocument()` | JSON→RDF import (overlaps with JSON-LD) |
| `apoc.spatial.*` | Delegate to PostGIS + GeoSPARQL integration |
| `apoc.nlp.*` | Delegate to `pg_ripple.nlq()` (NL→SPARQL) |
| NLP procedures | Wrap `pg_ripple`'s existing NL→SPARQL pipeline |

### 5.3 Schema and function naming

All APOC functions are registered in the `pg_ripple_apoc` schema:

```sql
CREATE SCHEMA IF NOT EXISTS pg_ripple_apoc;

-- Example:
CREATE FUNCTION pg_ripple_apoc.path_expand(
    start_node TEXT,       -- IRI of start node
    rel_types  TEXT[],     -- relationship type filters (empty = all)
    labels     TEXT[],     -- label filters for end nodes
    min_hops   INT DEFAULT 1,
    max_hops   INT DEFAULT 5,
    limit_val  INT DEFAULT -1  -- -1 = unlimited
) RETURNS TABLE (path JSONB) ...
```

A compatibility view maps the dotted APOC names to the flat SQL function names,
so `CALL apoc.path.expand(...)` syntax works within Cypher queries processed by
`pg_ripple.cypher()`.

**Effort: 10–14 person-weeks** for Tier 1 + Tier 2.

---

## 6. Delivery Schedule

### Release v1.2: Native Cypher Engine + TCK Gate

**Scope:** Deliverables 1 (full openCypher 9) + 4 (indexing hints)  
**Effort:** 16–21 person-weeks  
**Prerequisites:** v1.1 released (limited Cypher transpiler)

| Milestone | Description | Duration |
|---|---|---|
| M1 | `cypher-algebra` crate: grammar, AST, algebra IR | 4 weeks |
| M2 | `cypher-algebra` crate: normalize, semantic check, error reporting | 2 weeks |
| M3 | TCK adapter + CI integration (parse-level: ≥95% parse rate) | 2 weeks |
| M4 | `src/cypher/translator.rs`: basic MATCH/WHERE/RETURN/WITH | 3 weeks |
| M5 | `src/cypher/writer.rs`: CREATE/SET/DELETE | 2 weeks |
| M6 | `src/cypher/path_compiler.rs` + `shortest_path.rs` | 2 weeks |
| M7 | `src/cypher/merge_compiler.rs` + `expression.rs` full coverage | 2 weeks |
| M8 | Indexing hints (§4) | 1 week |
| M9 | TCK gate: ≥95% pass rate, CI blocking | 2 weeks |

**Total: ~20 weeks** with a single developer; parallelizable across 2 developers
to ~12 calendar weeks (M1–M3 and M4–M5 can overlap; M6–M7 depends on M4).

### Release v1.3: GQL Conformance + Mixed Transactions

**Scope:** Deliverables 2 (GQL) + 3 (mixed Cypher/SPARQL)  
**Effort:** 11–16 person-weeks  
**Prerequisites:** v1.2 released (native Cypher engine)

| Milestone | Description | Duration |
|---|---|---|
| M10 | GQL grammar extension in `cypher-algebra` | 3 weeks |
| M11 | GQL-specific operators (GRAPH TYPE, path modes, NEXT) | 4 weeks |
| M12 | GQL conformance suite integration (informational CI) | 2 weeks |
| M13 | Mixed Cypher/SPARQL transaction support | 3 weeks |
| M14 | HTTP endpoint routing (content-type dispatch) | 1 week |

### Release v1.4: APOC Compatibility Shim

**Scope:** Deliverable 5 (APOC)  
**Effort:** 10–14 person-weeks  
**Prerequisites:** v1.2 released (native Cypher engine for CALL dispatch)

| Milestone | Description | Duration |
|---|---|---|
| M15 | APOC Tier 1 procedures (path expansion, converters, collections) | 5 weeks |
| M16 | APOC Tier 2 procedures (import/export, refactor, algorithms, meta) | 5 weeks |
| M17 | APOC compatibility view layer + documentation | 2 weeks |
| M18 | APOC integration tests (based on Neo4j migration recipes) | 2 weeks |

---

## 7. Risk Analysis

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `cypher-algebra` parser takes longer than estimated due to Cypher grammar complexity (deeply nested expressions, unicode identifier rules) | Medium | Medium | Start with the openCypher Antlr grammar as reference; use winnow's error recovery to handle partial parses |
| GQL spec is 610 pages; full compliance requires handling obscure features | High | Low | Target the practical subset (GPM core); defer esoteric features to informational compliance |
| openCypher TCK has undocumented assumptions about Neo4j-specific behaviour | Medium | Medium | Document deviations in a compatibility matrix; engage with openCypher community |
| SQL/PGQ ships in PG19/PG20 and makes our Cypher engine redundant | Low | High | Our engine adds write support, APOC, and mixed SPARQL which PGQ won't have; position as complementary |
| APOC procedure semantics differ subtly from Neo4j (e.g., transaction handling) | High | Medium | Document per-procedure compatibility notes; focus on result equivalence, not implementation equivalence |
| Native Cypher engine duplicates SQL generation infrastructure from SPARQL engine | Medium | Medium | Factor shared SQL emission utilities into `src/sql_builder/` module; both engines import from there |

---

## 8. Success Criteria

| Deliverable | Metric | Target |
|---|---|---|
| openCypher 9 conformance | TCK pass rate (CI gate) | ≥95% |
| GQL conformance | Internal test suite (informational) | ≥90% of GPM core |
| Mixed transactions | Integration test suite | 100% pass |
| Indexing hints | Performance test: hinted vs. unhinted plans | Correct plan selection |
| APOC Tier 1 | Procedure-level integration tests | 100% pass |
| APOC Tier 2 | Procedure-level integration tests | 100% pass |
| Migration guide | Neo4j→pg_ripple migration tested end-to-end | 3+ migration recipes |

---

## 9. Testing Strategy

### 9.1 Unit tests

- `cypher-algebra` crate: parser round-trip tests (parse → algebra → pretty-print → re-parse)
- Expression compiler: Cypher expression → SQL expression for every operator and function
- Path compiler: variable-length path patterns → recursive CTE SQL strings

### 9.2 Integration tests (`#[pg_test]`)

- End-to-end `pg_ripple.cypher()` calls for every clause
- HTAP verification: Cypher writes go to delta, visible immediately, survive merge
- RDF-star edge properties: `CREATE (a)-[:KNOWS {since: 2020}]->(b)` round-trips
- SHACL validation fires for Cypher writes that violate constraints
- Mixed Cypher/SPARQL: write with Cypher, read with SPARQL (and vice versa)

### 9.3 Conformance suites (CI)

- **openCypher TCK**: ≥95% required, 100% aspirational
- **GQL test suite**: informational until the suite stabilises
- **Regression gate**: no TCK scenario may regress from green to red between releases

### 9.4 Performance benchmarks

- **Cypher vs. SPARQL equivalence**: same query expressed in both languages
  should produce plans within 10% of each other's execution time
- **Neo4j comparison**: benchmark the top 10 LDBC Social Network Benchmark
  queries in Cypher on pg_ripple vs. Neo4j Community Edition
- **APOC path expansion**: `apoc.path.expand()` on a 1M-triple graph should
  complete within 2× the equivalent SPARQL property path query

---

## 10. Documentation Deliverables

| Document | Content | Audience |
|---|---|---|
| Cypher Quick Start Guide | Install, first query, migration from Neo4j | New users |
| Cypher–SPARQL Comparison | Side-by-side query examples | Bilingual users |
| GQL Guide | GQL-specific syntax and features | Standards-aware users |
| APOC Compatibility Matrix | Per-procedure status (✅ / ⚠️ / ❌) | Neo4j migrants |
| Neo4j Migration Recipe | Step-by-step: export from Neo4j, import to pg_ripple, rewrite queries | DevOps |
| Cypher EXPLAIN Guide | Reading Cypher query plans, hint usage | Performance tuning |
| LPG↔RDF Mapping Reference | Detailed node/edge/property mapping rules | Architects |

---

## 11. Open Questions

1. **Should `cypher-algebra` support Cypher 25 (GQL-aligned) from day one, or
   openCypher 9 first with GQL added later?** Recommended: openCypher 9 first
   (v1.2), GQL grammar extension second (v1.3). This reduces the initial parser
   scope and aligns with the TCK (which tests openCypher 9, not GQL).

2. **Should the APOC shim live in the main extension or a separate extension?**
   Recommended: separate `pg_ripple_apoc` extension (same Cargo workspace) so
   users who don't need APOC don't pay any catalog overhead.

3. **How should `id(n)` behave?** Neo4j returns an internal integer ID;
   pg_ripple should return the IRI string (the canonical identity). This is a
   semantic difference that must be documented in the migration guide.

4. **Should we implement `CALL` for user-defined procedures?** Deferred. Cypher
   `CALL` is syntactic sugar for function invocation; pg_ripple can route it to
   PostgreSQL `SELECT function_name(...)` when the function exists.

5. **What happens when the Cypher and SPARQL engines produce different results
   for the same logical query?** This should be treated as a bug. Add a CI check
   that runs a suite of equivalent Cypher/SPARQL pairs and asserts identical
   result sets.
