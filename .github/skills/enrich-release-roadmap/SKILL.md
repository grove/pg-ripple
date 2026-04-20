---
name: enrich-release-roadmap
description: "Enrich a pg_ripple release roadmap with prioritised items across six quality pillars: correctness, stability, performance, scalability, ease-of-use, and test coverage. Use when planning a new release, fleshing out a milestone, or reviewing what gaps exist before tagging a version."
argument-hint: "Target release version (e.g. 0.36.0)"
---

# Enrich Release Roadmap

Proposes a rich, prioritised set of roadmap items for a target pg_ripple
release, grouped under six quality pillars, ready to paste into `ROADMAP.md`.

## When to Use

- Planning a new milestone / release
- Fleshing out a skeletal roadmap section
- Pre-release gap review (correctness, stability, performance, scalability, ease-of-use, coverage)

## Inputs to Gather First

Before generating proposals, read the following files so you have current context:

1. `ROADMAP.md` — existing entry for the target release (theme, listed items)
2. `plans/implementation_plan.md` — architecture and implementation order (if relevant to target)
3. `CHANGELOG.md` — last 2-3 releases to avoid duplicating shipped work
4. `AGENTS.md` — coding conventions and constraints that all items must respect

## Procedure

### Step 1 — Collect Context

Read the four files above in parallel. Identify:
- Items already listed for the target release
- Items recently shipped (do not re-propose)
- Known deferred items (explicitly call these out if proposing them again)

### Step 2 — Propose Items per Pillar

For each of the six pillars below, propose 3-6 items. For every item provide:

| Field | Format |
|-------|--------|
| **ID** | Pillar prefix + number, e.g. `CORR-1`, `STAB-2`, `PERF-3`, `SCAL-4`, `UX-5`, `TEST-6` |
| **Title** | ≤ 10-word action phrase |
| **Effort** | `XS` / `S` / `M` / `L` / `XL` |
| **Priority** | `P0` (must-have) · `P1` (high) · `P2` (nice-to-have) |
| **Description** | 2-4 sentences covering what, why, and how to verify |
| **Dependencies** | Other item IDs or prior-release features it builds on |
| **Schema change?** | Yes / No (SQL migration required) |

---

### Pillar 1 — CORRECTNESS

Guiding question: *"Could a user silently get the wrong answer from their SPARQL query?"*

Focus areas:
- SPARQL algebra translator producing incorrect SQL
- RDF-star graph model edge cases (quoted triples, nested quoting)
- Dictionary encoding collisions (XXH3-128 false positives)
- Datalog stratification correctness (negation, recursion)
- SHACL validation against malformed RDF
- CONSTRUCT/DESCRIBE/ASK query result accuracy
- Blank node scoping and renaming in property paths
- Join cardinality under star patterns (multiple predicates, same subject)
- `owl:sameAs` canonicalization and inference cycles
- Property path cycles and depth bounds

---

### Pillar 2 — STABILITY

Guiding question: *"Could this crash, corrupt, or leave the system in a broken state?"*

Focus areas:
- `unwrap()` / `panic!()` elimination in SQL-reachable paths
- Background merge worker crash recovery (partial BRIN builds, leftover tombstones)
- VP table OID lookup and dynamic SQL safety
- Dictionary cache eviction and race conditions
- SPI error propagation and SQLSTATE classification
- Extension upgrade migration safety (catalog drift, column additions, VP table renames)
- HTAP merge atomicity (delta/main/tombstone consistency)
- Error message quality and actionability
- Memory pressure and OOM graceful degradation

---

### Pillar 3 — PERFORMANCE

Guiding question: *"Are we leaving measurable query speed on the table?"*

Focus areas:
- SPARQL→SQL translation overhead (filter pushdown, join ordering)
- VP table scan efficiency (BRIN vs. B-tree trade-offs)
- Datalog semi-naive evaluation performance (magic sets, tabling cost)
- Dictionary cache hit rates under high cardinality literals
- Predicate promotion threshold tuning (rare vs. dedicated VP tables)
- HTAP merge scheduling (delta size, throughput, P50/P99 latency)
- Bulk load performance (`pg_ripple_bulk_load_triples`)
- Index-aware query planner hinting
- Property path depth limits and backtracking prevention

---

### Pillar 4 — SCALABILITY

Guiding question: *"Does this hold up at 10× the current scale?"*

Focus areas:
- Billion-triple RDF datasets (VP table count, memory usage)
- High-cardinality literals (dictionary size, cache pressure)
- Complex property paths (recursive depth, cycle detection overhead)
- Datalog rule count and stratification overhead
- Parallel merge worker utilisation across partitioned storage
- Named graph isolation and multi-tenant isolation
- Federation query pushdown efficiency
- Vector index integration with SPARQL (hybrid search scaling)

---

### Pillar 5 — EASE OF USE

Guiding question: *"Can a new user be productive in under 30 minutes?"*

Focus areas:
- SQL API ergonomics (function naming, defaults, return types)
- SPARQL error messages (include failing triple pattern, expected bindings)
- Documentation gaps (GETTING_STARTED, SPARQL_GUIDE, TROUBLESHOOTING, CONFIGURATION)
- Turtle / N-Triples / JSON-LD import examples and edge cases
- Runbook coverage (vacuum tuning, partition management, performance debugging)
- Playground / quickstart Docker experience
- pgAdmin / DBeaver integration examples
- Observability (pg_stat_statements, pg_ripple_stats, Prometheus export)
- Migration guides (other RDF stores, native PG arrays)
- PGXN packaging and installation verification

---

### Pillar 6 — TEST COVERAGE

Guiding question: *"What scenario could regress silently because no test covers it?"*

Focus areas:
- SPARQL W3C test suite compliance (DAWG, etc.)
- RDF-star conformance tests
- Datalog edge cases (negation, aggregates, OWL RL builtins)
- SHACL shape complexity (nested patterns, cross-shape references)
- Property path complexity (cycles, length constraints, inverse paths)
- Dictionary collision stress tests (large cardinality)
- HTAP merge atomicity under concurrent writes
- Federation endpoint resilience (timeout, 503, partial failures)
- Bulk load correctness (triple ordering, duplicate elimination)
- Upgrade migration tests (schema drift, constraint additions)
- pgbench / TPC-H style query regression gates

---

### Step 3 — Write the Release Theme

Write a single paragraph (≤ 5 sentences) summarising the spirit of the
release: what user-visible problem it solves, what internal foundations it
lays, and what the headline capability is.

### Step 4 — Flag Conflicts & Risks

In a "Conflicts & Risks" subsection, call out:
- Items that contradict each other
- Items that depend on features not yet shipped
- Items that require a SQL migration (schema freeze risk)
- Items that touch the dictionary encoder, SPARQL translator, or datalog engine (high regression risk — require thorough property tests)

---

## Output & Application

Do NOT present the enrichment for review or ask for confirmation.
Apply the changes directly:

1. **Replace** the target release section in `ROADMAP.md` with the enriched
   version (preserve existing items, add pillar sections after them).
   Follow the same style as existing enriched release sections in the file.
2. **Update** the effort summary table row for the target release to reflect
   the new effort estimate.
3. **Commit and push** with message:
   `roadmap: enrich v<X.Y.Z> with six-pillar quality items`

The enriched section must follow this structure:

```markdown
## v<X.Y.Z> — <Theme Title>

> **Release Theme**
> <one-paragraph theme>

### <Existing subsections with original items preserved>

### Correctness
| ID | Title | Effort | Priority |
|----|-------|--------|----------|
| CORR-1 | ... | S | P0 |
...

<brief description per item>

### Stability
...

### Performance
...

### Scalability
...

### Ease of Use
...

### Test Coverage
...

### Conflicts & Risks
...
```

---

## Constraints (Always Respect)

These come from `AGENTS.md` and must not be violated by any proposed item:

- Safe Rust only; `unsafe` is permitted solely at required FFI boundaries — always add a `// SAFETY:` comment
- Expose SQL functions via `#[pg_extern]`; never write raw `PG_FUNCTION_INFO_V1` C macros
- Use `pgrx::SpiClient` for all SQL executed inside extension code
- Shared memory state uses `pgrx::PgSharedMem` — size driven by GUC `pg_ripple.dictionary_cache_size`
- Background workers use `pgrx::BackgroundWorker` with `BGWORKER_SHMEM_ACCESS`
- All batch dictionary operations use `ON CONFLICT DO NOTHING … RETURNING` rather than SELECT-then-INSERT
- Integer joins everywhere: SPARQL→SQL translation must encode all bound terms to `i64` *before* generating SQL
- Filter pushdown: encode FILTER constants at translation time; never decode and re-encode at runtime
- Self-join elimination: detect star patterns and collapse into single scan with multiple joins
- Property paths: compile to `WITH RECURSIVE … CYCLE` — always use PG18's `CYCLE` clause for hash-based cycle detection
- No dynamic SQL string concatenation for table names — always look up the OID and use `format_ident!`-style quoting
