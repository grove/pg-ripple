# pg_trickle Integration Analysis for pg_triple

## 1. What Is pg_trickle?

[pg_trickle](https://github.com/grove/pg-trickle) is a PostgreSQL 18 extension (Rust/pgrx 0.17) that provides **declarative, automatically-refreshing materialized views** — called *stream tables* — powered by Incremental View Maintenance (IVM). When a base table changes, pg_trickle computes only the delta (changed rows), not the full result set. It supports the full SQL surface: JOINs, aggregates, window functions, CTEs (including `WITH RECURSIVE`), subqueries, LATERAL, and TopK.

Key capabilities relevant to pg_triple:

- **Incremental View Maintenance**: Only changed rows are processed (5–90× faster than full recomputation at 1% change rate)
- **DAG-aware scheduling**: Stream tables can depend on other stream tables; refreshed in topological order
- **Trigger-based and WAL-based CDC**: Hybrid change data capture with automatic mode selection
- **IMMEDIATE mode**: In-transaction IVM — stream table updated within the same transaction as the DML
- **Full SQL coverage**: GROUP BY, JOIN, WINDOW, WITH RECURSIVE, EXISTS, LATERAL, all expression types
- **Same tech stack**: PostgreSQL 18, Rust, pgrx 0.17 — identical to pg_triple

---

## 2. Integration Opportunities

### 2.1 Extended Vertical Partitioning (ExtVP) via Stream Tables

**Problem**: The deep-dive report identifies Extended Vertical Partitioning (ExtVP) as a critical optimization for world-class performance. ExtVP pre-computes semi-joins between frequently co-joined predicate tables. Our implementation plan defers ExtVP to post-1.0.

**pg_trickle solution**: Stream tables are a perfect implementation mechanism for ExtVP materialized views.

```sql
-- Pre-computed semi-join: subjects that have both foaf:knows AND foaf:name
SELECT pgtrickle.create_stream_table(
    name  => '_pg_triple.extvp_knows_name_ss',
    query => $$
        SELECT k.s, k.o AS knows_obj
        FROM _pg_triple.vp_7 k  -- foaf:knows
        WHERE EXISTS (
            SELECT 1 FROM _pg_triple.vp_12 n  -- foaf:name
            WHERE n.s = k.s
        )
    $$,
    schedule => '10s'
);
```

**Benefits**:
- ExtVP views stay incrementally up-to-date as triples are inserted/deleted — no manual refresh
- pg_trickle's EXISTS/semi-join delta operators handle the maintenance efficiently
- The SPARQL→SQL translator can rewrite queries to target these stream tables instead of raw VP tables
- pg_trickle's DAG awareness ensures ExtVP views refresh after their source VP tables

**Impact**: Brings ExtVP from "post-1.0" to achievable within the 0.x roadmap without building custom materialized view infrastructure.

### 2.2 SPARQL Query Result Caching

**Problem**: Frequently-executed SPARQL queries (e.g., dashboard queries, API-backing queries) re-execute the same SQL each time, including dictionary decoding.

**pg_trickle solution**: Wrap high-frequency SPARQL queries as stream tables that stay current automatically.

```sql
-- A "live" SPARQL result: always-fresh materialized SPARQL query
SELECT pgtrickle.create_stream_table(
    name  => 'active_person_emails',
    query => $$
        SELECT r1.value AS person, r2.value AS email
        FROM _pg_triple.vp_7 t          -- rdf:type
        JOIN _pg_triple.resource_dict r1 ON r1.id = t.s
        JOIN _pg_triple.vp_15 e          -- foaf:mbox
          ON e.s = t.s
        JOIN _pg_triple.resource_dict r2 ON r2.id = e.o
        WHERE t.o = 42                   -- foaf:Person
    $$,
    schedule => '1s'
);

-- Query is now a simple table scan — sub-millisecond
SELECT * FROM active_person_emails WHERE person LIKE '%Alice%';
```

**Benefits**:
- Converts multi-join SPARQL-generated SQL (VP table joins + dictionary decodes) into a simple table scan
- pg_trickle's differential mode processes only the triples that changed, not the full join
- Dictionary decoding happens once during materialization, not on every query
- Particularly powerful for star queries and analytical dashboards

**API surface**: A new pg_triple function:
```sql
pg_triple.create_sparql_view(
    name     TEXT,
    sparql   TEXT,
    schedule TEXT DEFAULT '5s'
) RETURNS VOID
```
This would: parse SPARQL → generate SQL → create a pg_trickle stream table.

### 2.3 HTAP Delta→Main Merge Replacement

**Problem**: Our implementation plan (v0.5.0) calls for building a custom background worker to merge delta partitions into main partitions — a non-trivial piece of infrastructure.

**pg_trickle alternative**: Model each VP table's "main" partition as a stream table over the delta.

```sql
-- The delta table is the source of truth (base table)
-- The main table is a stream table that mirrors it
SELECT pgtrickle.create_stream_table(
    name  => '_pg_triple.vp_7_main',
    query => $$
        SELECT s, o, g FROM _pg_triple.vp_7_delta
    $$,
    schedule     => '30s',
    refresh_mode => 'DIFFERENTIAL'
);
```

**Analysis**: This approach is elegant but has trade-offs:

| Aspect | Custom Merge Worker | pg_trickle Stream Table |
|---|---|---|
| Complexity | High (custom BGW, SPI, latch signaling) | Low (declarative) |
| BRIN index control | Full control over CLUSTER + BRIN rebuild | pg_trickle manages storage; no BRIN control |
| Compression | Can compress main partition | Stream tables are standard heap |
| Merge granularity | Batch size configurable | Driven by schedule |
| Read path | UNION ALL of delta + main | Query the stream table directly |

**Recommendation**: Use the custom merge worker for the core HTAP path (v0.5.0) where we need full control over storage layout, but use pg_trickle stream tables for *derived aggregates and analytics* built on top of the VP tables. The two approaches complement rather than replace each other.

### 2.4 Real-Time Analytics & Statistics

**Problem**: `pg_triple.stats()` currently re-scans catalog tables on every call. Predicate distribution, triple counts, and graph sizes need to be fresh but shouldn't require full scans.

**pg_trickle solution**: Stream tables for live operational metrics.

```sql
-- Per-predicate triple count, always current
SELECT pgtrickle.create_stream_table(
    name  => '_pg_triple.predicate_stats',
    query => $$
        SELECT p.id AS predicate_id,
               p.iri,
               COUNT(*) AS triple_count,
               COUNT(DISTINCT t.s) AS distinct_subjects,
               COUNT(DISTINCT t.o) AS distinct_objects
        FROM _pg_triple.predicates p
        JOIN _pg_triple.all_triples_view t ON t.p = p.id
        GROUP BY p.id, p.iri
    $$,
    schedule => '5s'
);

-- Graph-level statistics
SELECT pgtrickle.create_stream_table(
    name  => '_pg_triple.graph_stats',
    query => $$
        SELECT g AS graph_id,
               r.value AS graph_iri,
               COUNT(*) AS triple_count
        FROM _pg_triple.all_triples_view t
        JOIN _pg_triple.resource_dict r ON r.id = t.g
        GROUP BY g, r.value
    $$,
    schedule => '10s'
);
```

**Benefits**:
- `pg_triple.stats()` becomes a simple `SELECT * FROM _pg_triple.predicate_stats` — instant
- Aggregate maintenance is algebraic (COUNT/SUM) — pg_trickle's strongest differential case
- No custom counting infrastructure needed

### 2.5 SHACL Violation Monitoring

**Problem**: The implementation plan (v0.6.0–v0.7.0) designs an async validation pipeline with a custom background worker processing a validation queue.

**pg_trickle solution**: Model SHACL constraint checks as stream tables.

```sql
-- Cardinality violation detection: subjects missing a required property
SELECT pgtrickle.create_stream_table(
    name  => '_pg_triple.shacl_violations_min_count',
    query => $$
        -- Subjects of type foaf:Person (pred 7 = rdf:type, obj 42 = foaf:Person)
        -- that are missing foaf:name (pred 12)
        SELECT t.s AS subject_id, 12 AS required_predicate
        FROM _pg_triple.vp_7 t
        WHERE t.o = 42  -- foaf:Person
          AND NOT EXISTS (
              SELECT 1 FROM _pg_triple.vp_12 n WHERE n.s = t.s
          )
    $$,
    refresh_mode => 'IMMEDIATE'  -- validate in same transaction
);

-- Any row in this stream table = a SHACL violation
-- Empty table = all constraints satisfied
```

**Benefits**:
- `IMMEDIATE` mode validates within the same transaction — no async lag
- NOT EXISTS delta operators handle the semi-join efficiently
- Violation detection is declarative, not procedural
- Multiple SHACL shapes → multiple stream tables → pg_trickle's DAG handles ordering
- Violations are queryable as regular tables for reporting

**Limitation**: Complex SHACL shapes with multi-hop validation or logical combinators (`sh:or`, `sh:and`) would still need procedural triggers. Simple cardinality, datatype, and class constraints map well to stream tables.

### 2.6 Inference Materialization

**Problem**: RDF inference (RDFS entailment: `rdfs:subClassOf`, `rdfs:subPropertyOf`, `owl:sameAs`) requires computing the transitive closure of class/property hierarchies. This is computationally expensive at query time.

**pg_trickle solution**: Materialize inferred triples as stream tables using `WITH RECURSIVE`.

```sql
-- Materialize transitive closure of rdfs:subClassOf
SELECT pgtrickle.create_stream_table(
    name  => '_pg_triple.inferred_subclass',
    query => $$
        WITH RECURSIVE closure(sub, super) AS (
            -- Direct subclass relationships
            SELECT s AS sub, o AS super
            FROM _pg_triple.vp_99  -- rdfs:subClassOf
          UNION
            -- Transitive closure
            SELECT c.sub, sc.o AS super
            FROM closure c
            JOIN _pg_triple.vp_99 sc ON sc.s = c.super
        )
        SELECT sub, super FROM closure
    $$,
    schedule => '30s'
);

-- Now type queries can use the materialized closure
-- "Find all instances of Animal (including subclasses)"
-- becomes: JOIN _pg_triple.inferred_subclass ON super = :Animal_id
```

**Benefits**:
- pg_trickle supports `WITH RECURSIVE` in both FULL and DIFFERENTIAL modes
- Transitive closure is recomputed incrementally when class hierarchy changes (rare event)
- SPARQL queries involving `rdfs:subClassOf*` can query the stream table instead of running recursive CTEs at query time
- Massive performance win for inference-heavy workloads

### 2.7 Ontology Change Propagation

**Problem**: When an ontology changes (new classes, properties, or relationships), multiple derived structures need updating: ExtVP views, SHACL constraints, inference materializations, statistics.

**pg_trickle solution**: Model these as a DAG of stream tables:

```
Ontology triples (base)
    ├── inferred_subclass (stream table, WITH RECURSIVE)
    ├── inferred_subproperty (stream table, WITH RECURSIVE)
    ├── predicate_stats (stream table, GROUP BY)
    └── shacl_violations (stream table, NOT EXISTS)
         └── violation_summary (stream table, COUNT)
```

pg_trickle's DAG-aware scheduler automatically refreshes these in topological order when ontology triples change. Diamond-shaped dependencies (e.g., two views both depending on `rdf:type` and feeding into a summary) are handled atomically.

---

## 3. Integration Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      pg_triple                               │
│                                                              │
│  ┌──────────┐  ┌──────────┐  ┌───────────┐  ┌───────────┐  │
│  │Dictionary│  │VP Tables │  │  SPARQL   │  │  SHACL    │  │
│  │ Encoder  │  │(delta+   │  │  Engine   │  │  Engine   │  │
│  │          │  │ main)    │  │           │  │           │  │
│  └──────────┘  └────┬─────┘  └─────┬─────┘  └─────┬─────┘  │
│                     │              │              │          │
│         ┌───────────▼──────────────▼──────────────▼───┐     │
│         │              pg_trickle                      │     │
│         │                                              │     │
│         │  ┌──────────┐  ┌──────────┐  ┌──────────┐   │     │
│         │  │  ExtVP   │  │ Inference│  │  Stats   │   │     │
│         │  │  Views   │  │  Closure │  │  Aggs    │   │     │
│         │  └──────────┘  └──────────┘  └──────────┘   │     │
│         │  ┌──────────┐  ┌──────────┐  ┌──────────┐   │     │
│         │  │  SPARQL  │  │  SHACL   │  │  Query   │   │     │
│         │  │  Views   │  │ Monitors │  │  Cache   │   │     │
│         │  └──────────┘  └──────────┘  └──────────┘   │     │
│         │                                              │     │
│         │  CDC triggers on VP tables → IVM engine      │     │
│         │  DAG scheduler → topological refresh         │     │
│         └──────────────────────────────────────────────┘     │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### Extension Dependency

pg_trickle would be an **optional dependency** of pg_triple:

```sql
-- pg_triple.control
requires = ''  -- pg_trickle is optional

-- When pg_trickle is available, enable advanced features
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_trickle') THEN
        PERFORM pg_triple._enable_stream_table_features();
    END IF;
END $$;
```

pg_triple functions that create stream tables check for pg_trickle's presence:

```rust
#[pg_extern]
fn create_sparql_view(name: &str, sparql: &str, schedule: &str) -> Result<(), PgTripleError> {
    // Check if pg_trickle is installed
    let has_trickle = Spi::get_one::<bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_trickle')"
    )?.unwrap_or(false);

    if !has_trickle {
        return Err(PgTripleError::MissingDependency(
            "pg_trickle extension required for SPARQL views. Install with: CREATE EXTENSION pg_trickle"
        ));
    }

    // Parse SPARQL → SQL
    let sql = sparql_to_sql(sparql)?;

    // Create stream table via pg_trickle
    Spi::run(&format!(
        "SELECT pgtrickle.create_stream_table($1, $2, schedule => $3)",
    ), &[name.into(), sql.into(), schedule.into()])?;

    Ok(())
}
```

---

## 4. Roadmap Integration

| pg_triple Version | pg_trickle Feature | Priority |
|---|---|---|
| v0.5.0 (HTAP) | Real-time statistics stream tables | High |
| v0.6.0 (SHACL) | SHACL violation monitors (IMMEDIATE mode) | Medium |
| v0.7.0 (SHACL Advanced) | Multi-shape DAG validation | Medium |
| v0.8.0 (Serialization) | Inference materialization (WITH RECURSIVE) | High |
| v0.9.0 (Performance) | ExtVP stream tables, SPARQL view caching | High |
| v0.10.0 (Admin) | `pg_triple.create_sparql_view()` API | Medium |
| Post-1.0 | Full ExtVP automation, ontology change propagation DAG | High |

---

## 5. Performance Implications

### Wins

| Scenario | Without pg_trickle | With pg_trickle | Improvement |
|---|---|---|---|
| `pg_triple.stats()` | Full scan of all VP tables | Read from `predicate_stats` stream table | 100–1000× |
| Star query (cached) | 5-way VP join + dict decode | Single table scan | 10–50× |
| `rdfs:subClassOf*` traversal | Recursive CTE at query time | Read materialized closure | 5–20× |
| ExtVP semi-join | Not available (full VP join) | Pre-computed stream table | 2–10× |
| SHACL check | Scan + validate post-insert | IMMEDIATE mode — in-transaction | Same latency, no async lag |

### Costs

| Concern | Mitigation |
|---|---|
| Write-path overhead (CDC triggers) | pg_trickle's hybrid CDC: 20–55 µs/row trigger, ~5 µs/row WAL mode. Acceptable given VP tables are already I/O-bound on inserts. |
| Memory for stream table storage | Stream tables are heap tables — standard PG memory management. ExtVP views are subsets of VP tables, so storage is bounded. |
| Scheduler CPU | pg_trickle's zero-change overhead is 3.2ms average. With 10–20 stream tables, scheduling adds <100ms/sec total CPU. |
| Extension coupling | pg_trickle is optional; all core pg_triple features work without it. |

---

## 6. Shared Tech Stack Advantages

Both extensions share the identical technology foundation:

| Aspect | pg_triple | pg_trickle |
|---|---|---|
| Language | Rust (Edition 2024) | Rust (Edition 2024) |
| PG binding | pgrx 0.17 | pgrx 0.17 |
| Target PG | 18 | 18 |
| Background workers | pgrx `BackgroundWorker` | pgrx `BackgroundWorker` |
| SPI usage | Extensive | Extensive |
| Shared memory | Dictionary cache | Change buffers, DAG state |

This means:
- **No ABI incompatibility risk**: Both compiled against the same pgrx version targeting PG18
- **Shared development knowledge**: Patterns learned in one project transfer directly
- **Shared CI/CD**: Same `cargo pgrx test`, `cargo pgrx regress`, Docker-based testing infrastructure
- **Potential code sharing**: Common pgrx utilities (SPI helpers, GUC patterns, BGW patterns) could be extracted into a shared crate

---

## 7. Deployment Model

### Minimal (pg_triple only)

```ini
# postgresql.conf
shared_preload_libraries = 'pg_triple'
```

```sql
CREATE EXTENSION pg_triple;
-- Full triple store, no stream tables
```

### Enhanced (pg_triple + pg_trickle)

```ini
# postgresql.conf
shared_preload_libraries = 'pg_trickle, pg_triple'
max_worker_processes = 16
```

```sql
CREATE EXTENSION pg_trickle;
CREATE EXTENSION pg_triple;

-- Now these work:
SELECT pg_triple.create_sparql_view('my_view', 'SELECT ?s ?name WHERE { ... }');
SELECT pg_triple.enable_inference_materialization();
SELECT pg_triple.enable_live_statistics();
```

### Docker / CNPG

Both extensions ship as OCI images for CloudNativePG, making combined deployment straightforward:

```yaml
spec:
  postgresql:
    shared_preload_libraries: [pg_trickle, pg_triple]
    extensions:
      - name: pg-trickle
        image:
          reference: ghcr.io/grove/pg_trickle-ext:0.17.0
      - name: pg-triple
        image:
          reference: ghcr.io/grove/pg_triple-ext:1.0.0
```

---

## 8. Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| pg_trickle API changes (pre-1.0) | Medium | Medium | Pin to specific pg_trickle version; abstract calls behind pg_triple wrapper functions |
| CDC trigger conflicts (both extensions adding triggers) | Low | High | pg_triple's VP tables are internal (`_pg_triple` schema); pg_trickle CDC triggers are per-table and non-conflicting. Verify in integration tests. |
| Background worker slot exhaustion | Low | Medium | Document `max_worker_processes` sizing: pg_trickle needs 2–3, pg_triple merge worker needs 1, plus custom needs |
| Shared memory contention | Low | Low | Different shared memory segments; no overlap. pg_trickle uses its own shmem for DAG state; pg_triple uses its own for dictionary cache |

---

## 9. Recommendations

1. **Start with statistics** (v0.5.0): The lowest-risk, highest-value integration point. Create stream tables for `predicate_stats` and `graph_stats` when pg_trickle is detected. This validates the integration pattern with minimal complexity.

2. **Add SPARQL views** (v0.9.0): The `pg_triple.create_sparql_view()` function is the user-facing killer feature. It combines pg_triple's SPARQL→SQL translation with pg_trickle's IVM to give users always-fresh materialized SPARQL query results.

3. **Materialize inference** (v0.8.0): RDFS/OWL inference via `WITH RECURSIVE` stream tables is a differentiator no other PostgreSQL-based triple store offers. pg_trickle's recursive CTE IVM support makes this feasible.

4. **Defer ExtVP automation** (post-1.0): While stream tables are the right mechanism for ExtVP, the query workload analysis needed to *decide which* semi-joins to pre-compute is complex. Start with manual `create_sparql_view()` and automate later.

5. **Keep pg_trickle optional**: Core triple store functionality must never depend on pg_trickle. The integration should be a "power-user" layer that enhances performance and enables advanced features.

---

## 10. Summary

pg_trickle is a natural complement to pg_triple. Where pg_triple provides the storage model (dictionary encoding + vertical partitioning) and query language (SPARQL→SQL), pg_trickle provides the *reactivity layer* — keeping derived views, statistics, inference materializations, and cached query results incrementally up-to-date as the underlying graph changes.

The shared technology stack (Rust, pgrx 0.17, PostgreSQL 18) eliminates integration friction. pg_trickle's strong SQL coverage — including JOINs, aggregates, EXISTS, and `WITH RECURSIVE` — aligns precisely with the SQL patterns that pg_triple's SPARQL translator generates.

The recommended integration path is progressive: start with live statistics (low risk, high value), add SPARQL views (user-facing feature), then layer in inference materialization and eventually automated ExtVP. At every stage pg_trickle remains optional, and the core triple store stands alone.
