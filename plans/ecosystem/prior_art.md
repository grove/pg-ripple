# Prior Art Analysis: Logica, Mentat, Omnigres

> All three projects are released under the **Apache License 2.0**.
> This document records design insights drawn from reading their public source code and documentation.
> No code is copied; the analysis is used solely to inform pg_ripple's architecture.

---

## 1. Logica (EvgSkv/logica)

**Repository**: https://github.com/EvgSkv/logica  
**License**: Apache 2.0  
**What it is**: An open-source declarative logic programming language (Datalog family) that compiles to SQL targeting PostgreSQL, DuckDB, BigQuery, and SQLite. Successor to Google's internal Yedalog language. Active project; recently gained a C++ parser and type inference.

### 1.1 Relevance

Logica is the most direct prior art for `src/datalog/compiler.rs`. It solves exactly the problem pg_ripple must solve: compile Datalog rules (including aggregation and stratified negation) to correct, readable SQL.

### 1.2 Lessons

#### 1.2.1 Aggregation as a first-class rule IR node

Logica's name stands for **Logic with aggregation**. Aggregate accumulators (`+= 1`, `ArgMax=`, `ArgMaxK`) are syntactically integral to rule heads, not post-processing annotations. The compiler emits `GROUP BY` and ordered window-function SQL for these, not `HAVING` hacks.

**Implication for `src/datalog/`**: The Rule IR (`rule.rs` or `compiler.rs`) needs an explicit `HeadAtom` variant for aggregate heads — separate from the `HeadAtom::Plain` case. Mixing the two in a single enum arm and branching on a boolean flag is the path to bugs.

```rust
pub enum HeadAtom {
    Plain { predicate: i64, args: Vec<Term> },
    Aggregate {
        predicate: i64,
        key_args: Vec<Term>,    // GROUP BY columns
        agg_arg: Term,          // accumulated value
        op: AggOp,              // Sum | Min | Max | ArgMax | Count
    },
}
```

#### 1.2.2 Negation strata compile to named CTEs, not inline subqueries

Logica emits a `WITH` chain where each stratum is a named CTE. Stratum N uses `NOT EXISTS (SELECT 1 FROM stratum_N_minus_1 WHERE ...)` against the CTE defined immediately above it — not a correlated subquery reaching back into the base tables.

**Implication**: `src/datalog/compiler.rs` should accumulate strata as a `Vec<(String, String)>` (CTE name, CTE body SQL) and emit a single `WITH name1 AS (...), name2 AS (...), ... SELECT * FROM nameN` statement. This keeps the generated SQL both readable and optimizer-friendly (PostgreSQL materialises `WITH` by default in PG17- and can be hinted in PG18).

#### 1.2.3 Disjunction: additive vs. distinct bodies

Logica distinguishes:
- **Additive disjunction** — multiple rules with the same head accumulate rows: `UNION ALL` + outer `SELECT DISTINCT` only when needed.
- **Explicit `distinct` keyword** — forces deduplication in the emitted SQL.

Without this distinction every disjunctive rule generates an unnecessary `UNION` (set union, O(n log n)) instead of the cheaper `UNION ALL`.

**Implication for `src/datalog/parser.rs`**: Track a `dedup: bool` flag per rule head. Default to `false` (additive); set to `true` on `DISTINCT` in syntax or when negation forces it.

#### 1.2.4 C++ parser performance note

Logica recently added a C++ parser alongside the Python one for speed. For pg_ripple's use case (rules parsed once and cached in `_pg_ripple.rules`) this is not needed — but the move signals that Logica's Python parser is slow enough to matter at scale. pg_ripple's Rust `nom`/`pest` parser avoids this concern from day one.

---

## 2. Mentat (mozilla/mentat)

**Repository**: https://github.com/mozilla/mentat  
**License**: Apache 2.0  
**Status**: Archived (unmaintained since September 2018)  
**What it is**: A Datomic-inspired, Datalog-queryable quad store implemented in Rust over SQLite. Structurally the closest antecedent to pg_ripple: same language (Rust), same data model (entity–attribute–value quads), same approach (Datalog→SQL via a multi-phase query compiler). Originally written in ClojureScript as "Datomish"; ported to Rust for embedding in Firefox and mobile.

### 2.1 Relevance

Mentat's query pipeline is a worked example of every hard problem pg_ripple's `src/sparql/` must solve. The codebase pre-dates pgrx and targets SQLite rather than PostgreSQL, but the algebraic structure is directly applicable.

### 2.2 Lessons

#### 2.2.1 Four-phase query pipeline with a typed intermediate IR

Mentat's query pipeline has four distinct Rust types:

```
parsed query (mentat_query types)
    │  (algebrizer — schema-aware)
    ▼
AlgebraicQuery (join tree, constraints, projection spec)
    │  (projector — output shape)
    ▼
QueryOutput (column bindings, type coercions)
    │  (translator — SQL AST)
    ▼
mentat_query_sql IR (ColumnOrExpression, GroupBy, …)
    │  (emitter)
    ▼
SQL text + bound parameters
```

pg_ripple's current plan collapses this to two steps: `spargebra Algebra → SQL`. The missing layer is a **`JoinPlan` IR** — a typed Rust struct tree representing joins, filters, bound variables, and projections before any SQL string is built. Optimization passes (star-pattern collapsing, filter pushdown, OPTIONAL→LEFT JOIN decisions) should operate on `JoinPlan`, not on SQL strings or on `spargebra`'s algebra directly.

Suggested module split for `src/sparql/`:

```
src/sparql/
    mod.rs           — public #[pg_extern] entrypoints
    parser.rs        — spargebra call, error mapping
    algebrizer.rs    — spargebra Algebra → JoinPlan IR (schema-aware)
    plan.rs          — JoinPlan types (JoinNode, FilterNode, ProjectionSpec)
    optimizer.rs     — rewrites on JoinPlan (star collapse, pushdown, SHACL hints)
    projector.rs     — JoinPlan + SELECT list → SqlQuery IR
    emitter.rs       — SqlQuery IR → SQL string + bound params
    decode.rs        — SPI result rows → SPARQL result set (decode i64 → IRI/literal)
```

#### 2.2.2 Schema-aware algebrization: SHACL hints drive join rewrites

Mentat's algebrizer is handed the live attribute schema (value types, cardinality, uniqueness) and uses it during query planning:

- `cardinality/one` attribute → omit `DISTINCT` from the emitted SQL
- `unique/identity` attribute → downgrade `LEFT JOIN` to `INNER JOIN` when that predicate is used as a join key

pg_ripple already plans to use SHACL hints the same way (see AGENTS.md "SHACL hints"). Mentat shows the exact mechanism: the algebrizer reads the attribute catalog **before** building the join tree, and the hint is embedded into the `JoinNode` at construction time — it does not require a separate rewrite pass. Both approaches are valid; the key point is that the catalog lookup must happen during algebrization, before the SQL is emitted.

#### 2.2.3 `InternSet` for encoding-time constant deduplication

When translating FILTER expressions, the same IRI constant may appear multiple times across `VALUES` bindings, FILTER literals, and graph patterns. Mentat uses an `InternSet<T>` (a `HashMap<T, usize>` + `Vec<T>`) to deduplicate constants during planning and emit them as numbered bind parameters rather than repeating SPI dictionary lookups.

**Implication**: Add an `EncodingCache` (or reuse the LRU dictionary cache) that is scoped to a single query translation. During `algebrizer.rs`, accumulate all constant IRIs/literals into the cache; emit a batch `encode_constants(&[&str]) -> Vec<i64>` SPI call once, then bind all parameters from the resulting `Vec<i64>`. This converts O(n) SPI round-trips to O(1).

#### 2.2.4 Projection is complex enough to deserve its own module

`mentat_query_projector` is one of the larger Mentat crates. SPARQL SELECT's projection semantics are independently complex: `SELECT *` expansion, expression-valued projections (`(CONCAT(?a, ?b) AS ?c)`), result row typing (every projected column needs a type tag for correct literal decoding), and `DISTINCT` interaction with `ORDER BY`.

`src/sparql/projector.rs` should be planned as a non-trivial module from v0.3.0, not bolted onto the emitter.

#### 2.2.5 Batch-flush pattern in the `tolstoy` sync layer

Mentat's `tolstoy` sync module (described as structurally analogous to a transaction log replayer) operates as: accumulate committed transaction metadata → batch-flush to a remote store → record high-water mark → repeat. The HTAP merge worker in `src/storage/merge.rs` has exactly this structure: read the delta partition in batches → insert into main partition → update the merge cursor — and should be implemented with the same three-phase loop rather than a streaming cursor.

---

## 3. Omnigres

**Repository**: https://github.com/omnigres/omnigres  
**License**: Apache 2.0  
**What it is**: A collection of 30+ deeply integrated C/C++ PostgreSQL extensions that turn a single Postgres instance into a full application platform (HTTP server, background workers, typed shared memory, reactive queries, ledgering, etc.). The `omni` core provides an extension hypervisor; individual extensions are independently adoptable.

### 3.1 Relevance

Omnigres is not a triple store or a reasoning engine — it is the most sophisticated example of production-grade pgrx/C extension architecture available. The three sub-systems most relevant to pg_ripple are `omni_worker` (background worker pool), `omni_shmem` (named shared memory slots), and `pg_yregress` (YAML-based regression testing).

### 3.2 Lessons

#### 3.2.1 `omni_worker` message-queue pattern for the HTAP merge worker

`omni_worker` provides a worker pool where backends post `WorkItem` messages to a shared queue; worker processes claim items and execute them within their own backend context. The architecture solves the hard problems pg_ripple's merge worker also faces:

- **Crash-safe queue**: messages are durable until acknowledged; a crashed worker leaves the item for a sibling to retry.
- **Backpressure**: if all workers are busy the queue depth is visible to the poster; pg_ripple's merge worker should similarly expose a `pg_ripple.merge_queue_depth` GUC-limited metric.
- **Graceful shutdown**: workers listen for `SIGTERM` via `BackgroundWorker::wait_latch` and finish their current batch before exiting.

**Implication for `src/storage/merge.rs`**: Implement the merge worker loop as:

```
loop {
    sleep_on_latch(merge_interval_ms);
    if shutdown_requested() { break; }
    for each vp table with delta rows:
        batch_merge_delta_to_main(table_oid, batch_size);
        update_merge_cursor(table_oid);
    update_latch();
}
```

The latch should be poke-able from the transaction commit hook (via `pgstat_report_activity` or a lightweight notification semaphore) so that a large bulk load triggers an immediate merge cycle rather than waiting for the next timer tick.

#### 3.2.2 Named versioned shared memory slots for the dictionary cache

`omni_shmem` gives each extension a named, version-tagged slot in the shared memory segment. Versioning is critical: if the cache struct layout changes (e.g., `dictionary_cache_size` GUC changes at restart, or a new hash collision list field is added), the slot version is bumped and all backends reattach cleanly rather than reading stale memory with a different layout.

pg_ripple's `PgSharedMem` dictionary cache is currently planned as a single flat allocation. Two concrete risks:

1. **GUC resize across restart**: if `pg_ripple.dictionary_cache_size` is changed in `postgresql.conf` and the server restarts, the shmem segment size changes. Without versioning, old backends that haven't restarted will read a truncated or oversized buffer.
2. **Future struct evolution**: adding collision chain pointers or a bloom filter to the cache struct changes its binary layout silently.

**Implication for `src/dictionary/cache.rs`**: Store a magic number and a layout version as the first 16 bytes of the shmem slot. At attach time, check the version; if mismatched, clear and reinitialise. The version should be a compile-time constant derived from the struct size.

#### 3.2.3 `pg_yregress` YAML test format

Omnigres uses YAML test files where each test case is a document with `query` and `result` fields. The regression runner compares results structurally (not as raw text), making tests resilient to column-order changes and whitespace differences.

The pg_ripple regression suite currently plans `.sql` + `.expected` pairs via `cargo pgrx regress`. This works for v0.1–v0.3 where outputs are simple. From v0.3.0 (SPARQL Basic) onwards, SPARQL result sets are JSON tables whose column order is non-deterministic. YAML-structured tests that compare result sets as unordered bags of rows would eliminate most false failures.

**Options** (in order of effort):
1. **Adopt `pg_yregress` directly** — requires adding it as a dev dependency; works immediately since pg_ripple targets PostgreSQL 18.
2. **Write a thin YAML harness in Rust** — a small `tests/regress.rs` that reads `tests/sparql/*.yaml` and drives `pgrx::Spi`.
3. **Sort output in `.expected` files** — low effort; works until result metadata (variable names, types) also needs structural comparison.

For v0.3.0–v0.5.0, option 3 is pragmatic. Option 1 should be evaluated at v0.6.0 when SHACL validation reports introduce nested JSON output.

#### 3.2.4 Incremental adoption: GUC-gated subsystem initialization

Omnigres's design principle — each extension is independently adoptable, paying no cost for unused features — directly applies to pg_ripple's optional subsystems:

| Subsystem | Current plan | Omnigres-inspired change |
|---|---|---|
| Datalog reasoner | Always initialized in `_PG_init` | Only if `pg_ripple.enable_reasoner = on` (default off until v0.9.0) |
| HTAP merge worker | Always registered | Only if `pg_ripple.htap_enabled = on` |
| SHACL validator | Background worker always present | Only if at least one shape graph is loaded |

Lazy initialization reduces startup latency for databases that only need basic SPARQL and storage.

---

## 4. Summary: Changes to pg_ripple Architecture

The following table maps each lesson to a concrete change in the implementation plan or codebase structure.

| Source | Lesson | Target module | Roadmap version |
|---|---|---|---|
| Logica | `HeadAtom` enum with explicit `Aggregate` variant | `src/datalog/plan.rs` | v0.9.0 |
| Logica | Strata compile to named CTE chain, not inline subqueries | `src/datalog/compiler.rs` | v0.9.0 |
| Logica | Track `dedup` flag per rule head to choose `UNION ALL` vs `UNION` | `src/datalog/parser.rs` | v0.9.0 |
| Mentat | Four-phase pipeline with typed `JoinPlan` IR | `src/sparql/plan.rs` (new) | v0.3.0 |
| Mentat | `algebrizer.rs` reads SHACL/attribute catalog before building join tree | `src/sparql/algebrizer.rs` | v0.6.0 |
| Mentat | `EncodingCache` for batch constant encoding per query | `src/sparql/algebrizer.rs` | v0.3.0 |
| Mentat | `projector.rs` as its own module from the start | `src/sparql/projector.rs` (new) | v0.3.0 |
| Mentat | Batch-flush three-phase loop for delta→main merge | `src/storage/merge.rs` | v0.5.0 |
| Omnigres | Latch-poke from commit hook to trigger merge cycle early | `src/storage/merge.rs` | v0.5.0 |
| Omnigres | Magic number + layout version at head of shmem slot | `src/dictionary/cache.rs` | v0.5.0 |
| Omnigres | YAML structured regression tests for SPARQL result sets | `tests/sparql/*.yaml` | v0.6.0 |
| Omnigres | GUC-gated lazy initialization of reasoner, merge worker, SHACL | `src/lib.rs` | v0.9.0 |
