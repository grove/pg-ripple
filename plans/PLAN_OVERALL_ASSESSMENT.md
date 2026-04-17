# pg_ripple Overall Assessment

**Date**: 2026-04-17 · **Assessed release**: v0.20.0 · **Next milestone**: v1.0.0 — Production Release

> Scope: Full-stack code, test, build, and spec-conformance review of the pg_ripple PostgreSQL extension and its `pg_ripple_http` companion. Source of truth: [AGENTS.md](../AGENTS.md), [ROADMAP.md](../ROADMAP.md), [plans/implementation_plan.md](implementation_plan.md), and [CHANGELOG.md](../CHANGELOG.md).

---

## Executive Summary

pg_ripple is an ambitious, genuinely impressive PostgreSQL 18 extension that has shipped the overwhelming majority of its roadmap in 20 tightly-scoped releases. The architecture — dictionary-encoded vertical partitioning, per-predicate B-tree/BRIN dual storage, HTAP delta/main split, SHACL validation, Datalog with RDFS/OWL RL built-ins, JSON-LD framing, federation with pooling/caching, and pg_trickle-backed incremental views — is coherent, well-factored, and largely idiomatic Rust. Test discipline is strong (64 pg_regress files, three kill-9 crash-recovery harnesses, a migration-chain test, and a BSBM benchmark at 100M triples). The code largely lives up to AGENTS.md's security posture: dictionary-encoding happens before SQL generation, `unsafe {}` blocks carry `// SAFETY:` comments, federation endpoints are allow-listed.

That said, the v0.20.0 "100% W3C conformance" claim rests on a test suite that uses `>= 0 AS label_no_error` style assertions for large chunks of the built-in function surface — those features are not actually implemented in [src/sparql/sqlgen.rs](../src/sparql/sqlgen.rs). The FILTER expression translator recognises only a handful of SPARQL functions and **silently drops** the rest, which is a correctness hazard, not merely a feature gap. There are also several material correctness issues in the HTAP merge path, the dictionary cache lifecycle, and property-path cycle handling that should be closed before a production v1.0.0 tag.

### Top 3 Critical Issues

1. **FILTER expressions with unsupported built-ins silently return extra rows** (not fewer). The translator at [src/sparql/sqlgen.rs:1600–1750](../src/sparql/sqlgen.rs) matches a small set of `Expression` variants and returns `None` for anything else, and callers short-circuit with `?` — dropping the whole filter. Queries using `STR`, `LANG`, `DATATYPE`, `isIRI`, `isLiteral`, `isBlank`, `UCASE`, `LCASE`, `STRLEN`, `SUBSTR`, `CONCAT`, `REPLACE`, `IF`, `COALESCE`, `ABS`, `CEIL`, `FLOOR`, `ROUND`, `NOW`, `YEAR…TZ`, `MD5…SHA512`, `UUID`, `STRUUID`, `LANGMATCHES`, `ENCODE_FOR_URI`, `BNODE`, `IRI`, `STRDT`, `STRLANG`, and numeric type predicates produce wrong answers rather than errors. This is the single biggest correctness exposure in the engine.
2. **Backend-local dictionary cache is not invalidated on ROLLBACK** ([src/dictionary/mod.rs:60–138](../src/dictionary/mod.rs)). A backend that inserts a new term, aborts, then re-encodes the same term will retrieve the rolled-back `id` from the thread-local LRU and plant a phantom ID into a VP table — silent corruption. There is no `RegisterXactCallback` / `xact_end_callback` wired up anywhere in the crate (grep-verified).
3. **HTAP merge has a view/rename atomicity window and a tombstone/delete race** ([src/storage/merge.rs](../src/storage/merge.rs)). Between `ALTER TABLE vp_N_main_new RENAME TO vp_N_main` and the subsequent `CREATE OR REPLACE VIEW vp_N`, concurrent queries can hit a dropped or mismatched relation. Separately, the merge SELECT over `(main LEFT JOIN tombstones)` is not row-locked, so a tombstone inserted after the merge's snapshot but truncated in the merge's finaliser can cause a deleted triple to resurrect in the freshly-built `main`.

### Top 3 Performance Concerns

1. **No semi-naive evaluation in Datalog** (inferred from [src/datalog/compiler.rs](../src/datalog/compiler.rs) generating monolithic recursive CTEs). Large recursive rule programs re-evaluate the full derived relation on each iteration — fine for RDFS closure on small graphs, but quadratic-ish on DBpedia/Wikidata-scale workloads.
2. **Shared-memory encode cache uses direct-mapped slots with unconditional overwrite** ([src/shmem.rs:420–440](../src/shmem.rs)) — 4 × 1024 = 4096 slots. On any dataset with ≳5k hot terms the birthday-collision rate causes cache thrashing; every thrashed lookup falls through to an SPI round-trip to `_pg_ripple.dictionary`. Replace with a small set-associative cache (4-way) or clock eviction.
3. **`translate_service()` batching relies on structural Debug-string pattern equality** for the self-join eliminator ([src/sparql/sqlgen.rs:430–450](../src/sparql/sqlgen.rs)). `format!("{tp}")` as the dedup key is fragile and also over-eliminates — two identical triple patterns intentionally written for a Cartesian semantics are collapsed. Correctness and performance are both at risk; a structural key is required.

### Top 3 Recommended Features

1. **Complete SPARQL 1.1 built-in function surface** — this is the single highest-leverage change for real conformance and is a v1.0.0 blocker. Fits a dedicated v0.21.0 release.
2. **`sh:path` with path expressions, `sh:closed` / `sh:ignoredProperties`, `sh:hasValue`, `sh:nodeKind`, `sh:languageIn`, `sh:uniqueLang`, `sh:lessThan`/`sh:greaterThan`, `sh:qualifiedValueShape`** — the current SHACL implementation is only the "simple" subset and fails on many real-world schemas (Schema.org shapes, Shapes Constraint Language recipes).
3. **SPARQL Explain / `EXPLAIN (ANALYSE, FORMAT SPARQL)` surface** — a first-class "show me the generated SQL and plan" entrypoint already exists partially in `jsonld_frame_to_sparql`; generalising it (`pg_ripple.explain_sparql(query TEXT, format TEXT)`) would massively improve debuggability and unlock third-party tuning, with low implementation cost.

### Overall Maturity and Production-Readiness Score

| Axis | Score (0–5) | Notes |
|---|---|---|
| Architecture | 4.5 | Clean, well-motivated, well-documented; VP + HTAP + dictionary split is correct |
| Core storage correctness | 3.5 | Merge race windows, dictionary cache/rollback bug |
| SPARQL correctness | 3.0 | BGP/OPT/UNION/MINUS/aggregates work; built-in function surface is thin; FILTER silent-drop is serious |
| SPARQL spec coverage | 3.0 | W3C test pass rate is real but uses "no-error" placeholders for a large swathe |
| SHACL coverage | 2.5 | Missing `sh:closed`, `sh:hasValue`, `sh:nodeKind`, property paths in `sh:path`, language constraints |
| Datalog | 3.5 | Stratification + OWL RL built-ins present; naive evaluation only; some RL rules missing |
| Tests | 4.0 | 64 pg_regress files, crash-recovery harness, migration chain, BSBM — very good coverage |
| Security | 3.5 | SQL-injection discipline is solid; privilege model (no REVOKEs) and HTTP rate-limit stub are gaps |
| Documentation | 4.0 | User guide, reference, release notes all present; occasional drift from code |
| **Overall** | **3.5** | **Beta-quality; not yet 1.0. Closing the Critical list and the SPARQL built-ins will get it there.** |

---

## Issues & Bugs

> **Roadmap Quick-Reference** — every issue below maps to a planned release that resolves it.
>
> | Issue | Resolution milestone |
> |---|---|
> | C-1 | [v0.21.0 — SPARQL Built-in Functions & Query Correctness](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness) |
> | C-2 | [v0.22.0 — Storage Correctness & Security Hardening](../ROADMAP.md#v0220--storage-correctness--security-hardening) |
> | C-3 | [v0.22.0 — Storage Correctness & Security Hardening](../ROADMAP.md#v0220--storage-correctness--security-hardening) |
> | C-4 | [v0.22.0 — Storage Correctness & Security Hardening](../ROADMAP.md#v0220--storage-correctness--security-hardening) |
> | H-1, H-2, H-3, H-4, H-6, H-7, H-12 (partial), H-14, H-15 | [v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening) |
> | H-5, H-8, H-9, H-10, H-11, H-12 (partial) | [v0.21.0](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness) |
> | H-12, H-13 | [v0.25.0 — GeoSPARQL & Architectural Polish](../ROADMAP.md#v0250--geosparql--architectural-polish) |
> | M-1, M-2, M-3, M-4, M-5, M-11, M-18 | [v0.23.0 — SHACL Core Completion & SPARQL Diagnostics](../ROADMAP.md#v0230--shacl-core-completion--sparql-diagnostics) |
> | M-6, M-7 | [v0.21.0](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness) |
> | M-12, M-13, M-14, M-15 | [v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening) |
> | M-16, M-17 | [v0.24.0 — Semi-naive Datalog & Performance Hardening](../ROADMAP.md#v0240--semi-naive-datalog--performance-hardening) |
> | M-8, M-9, M-10, M-19, L-2 through L-7 | [v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish) |
> | P-1 | [v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening) |
> | P-2, P-3, P-4, P-6, A-2, A-6 | [v0.24.0](../ROADMAP.md#v0240--semi-naive-datalog--performance-hardening) |
> | P-5, A-1 | [v0.21.0](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness) / [v0.23.0](../ROADMAP.md#v0230--shacl-core-completion--sparql-diagnostics) |
> | A-3, A-4, A-5, S-4, S-8 | [v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish) |
> | S-1, S-2, S-3, S-5 | [v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening) |

### Critical

- **C-1 · SPARQL FILTER silent-drop on unsupported expressions** — *resolved in [v0.21.0](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness)*
  - **Location**: [src/sparql/sqlgen.rs](../src/sparql/sqlgen.rs) — `translate_expr()` (~line 1600+) returns `Option<String>`; the few `Expression::FunctionCall(Function::Contains | StrStarts | StrEnds | Regex, …)` arms are the only built-in functions handled. All others fall through to `None`, and callers use `?` to short-circuit, dropping the filter from the `WHERE` clause.
  - **Impact**: Any SPARQL query whose FILTER uses `STR`, `LANG`, `DATATYPE`, `IF`, `COALESCE`, `isIRI`, `isLiteral`, `isBlank`, `isNumeric`, `LANGMATCHES`, `UCASE`, `LCASE`, `STRLEN`, `SUBSTR`, `CONCAT`, `REPLACE`, `ENCODE_FOR_URI`, `ABS`, `CEIL`, `FLOOR`, `ROUND`, `RAND`, `NOW`, `YEAR`, `MONTH`, `DAY`, `HOURS`, `MINUTES`, `SECONDS`, `TIMEZONE`, `TZ`, `MD5`, `SHA1`, `SHA256`, `SHA384`, `SHA512`, `UUID`, `STRUUID`, `IRI`, `BNODE`, `STRDT`, `STRLANG`, arithmetic on xsd:decimal/xsd:double, or date arithmetic returns **too many rows**, not an error.
  - **Root cause**: Short-circuit `?` propagation in filter translation conflates "unsupported" with "always false".
  - **Suggested fix**: When `translate_expr()` returns `None`, either (a) emit `FALSE` into the WHERE clause to be conservative, (b) raise a structured parse-time error that lists the unsupported feature, or (c) implement the function. Option (b) is the minimum for 1.0; (c) is the real fix. Decode-and-operate at query time against dictionary values is acceptable for the long-tail functions.

- **C-2 · Backend-local dictionary cache survives ROLLBACK** — *resolved in [v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
  - **Location**: [src/dictionary/mod.rs:60–138](../src/dictionary/mod.rs) — `ENCODE_CACHE` / `DECODE_CACHE` are `thread_local!` `LruCache`s with no transaction lifecycle hook.
  - **Impact**: `BEGIN; INSERT … new term 't'; ROLLBACK; INSERT … 't' again;` — the second INSERT retrieves the pre-rollback phantom dictionary ID from cache and stores it in the VP table, referencing a dictionary row that no longer exists. Subsequent `decode_id` returns NULL; joins against that ID are unconditionally empty. This is **silent data corruption**.
  - **Root cause**: No `RegisterXactCallback` / `RegisterSubXactCallback` is installed (grep-verified: no hits in `src/**` for those symbols).
  - **Suggested fix**: Register an xact-end callback during `_PG_init` that drains both caches on `XACT_EVENT_ABORT` and `XACT_EVENT_PARALLEL_ABORT`. The shmem cache is also at risk under the same scenario and needs the same hook — probably simpler to invalidate by stamping a per-backend epoch and comparing on read.

- **C-3 · HTAP merge view-rename race** — *resolved in [v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
  - **Location**: [src/storage/merge.rs](../src/storage/merge.rs) around the `ALTER TABLE vp_{pred_id}_main_new RENAME TO vp_{pred_id}_main` + subsequent `CREATE OR REPLACE VIEW vp_{pred_id}` block.
  - **Impact**: Readers that resolved the view's dependency on the old `vp_N_main` before the rename can error with `relation does not exist` or — worse under READ COMMITTED — silently miss rows that had been in `_main` before the merge.
  - **Root cause**: DDL is executed sequentially without holding an `ACCESS EXCLUSIVE` on the view between rename and view-rebuild. `BackgroundWorker::transaction()` uses the default isolation.
  - **Suggested fix**: Keep the view definition stable across merges (the view should reference `vp_N_main` as a name, which automatically re-resolves after rename; the brittle step is recreating it). Alternatively, swap main in place (`TRUNCATE vp_N_main; INSERT INTO vp_N_main SELECT …`) inside `SERIALIZABLE` — slower but atomic. Add a regression test that issues a SPARQL query in one backend while `just merge` runs in another, with an injected 500 ms sleep between rename and view-rebuild.

- **C-4 · HTAP delete/merge race: resurrected rows** — *resolved in [v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
  - **Location**: [src/storage/merge.rs](../src/storage/merge.rs) — the `SELECT m.* FROM main m LEFT JOIN tombstones t … WHERE t.s IS NULL` snapshot + the subsequent `TRUNCATE vp_N_tombstones` step.
  - **Impact**: A `DELETE` that commits between the merge-start snapshot and the TRUNCATE has its tombstone removed but never landed in the new `main` (because the merge snapshot was older). The deleted triple reappears.
  - **Root cause**: Merge reads tombstones without `FOR UPDATE` and truncates them unconditionally at the end.
  - **Suggested fix**: Only TRUNCATE tombstones whose `i` (SID) is ≤ the max SID observed in the merge snapshot. Add a tombstone regression test that interleaves `delete_triple` with an in-progress merge.

### High

- **H-1 · Dictionary cache-insert on shmem cache uses direct-mapped slots with unconditional overwrite** — [src/shmem.rs:420–440](../src/shmem.rs). Birthday collisions cause thrashing at a few thousand hot terms. Fix with 4-way set-associative buckets. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **H-2 · Bloom-filter clear can false-negative after merge** — [src/shmem.rs:241–260](../src/shmem.rs). `clear_predicate_delta_bit()` clears both bits unconditionally; if any other predicate hashes to the same bit, queries for *that* predicate will skip the delta scan and return stale reads. Fix by keeping a per-bit reference count instead of a boolean. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **H-3 · Promotion of rare predicates is not atomic with concurrent inserts** — [src/storage/mod.rs:350–368](../src/storage/mod.rs). `INSERT INTO vp_{id}_delta SELECT … FROM vp_rare WHERE p = $1` then `DELETE FROM vp_rare WHERE p = $1` in separate statements. Rows inserted between them orphan in `vp_rare` under a predicate that now has a dedicated VP table. Fix: single CTE `WITH moved AS (DELETE … RETURNING …) INSERT …`. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **H-4 · Promotion resets `_pg_ripple.predicates.triple_count` to 0** — same location. `ensure_vp_table` inserts with `triple_count = 0` and `ON CONFLICT DO UPDATE` without resetting to the actual count. Planner stats suffer; join reordering in [optimizer.rs](../src/sparql/optimizer.rs) reads from here. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **H-5 · Property-path cycle detection tracks only the node, not the path** — [src/sparql/property_path.rs:100–130](../src/sparql/property_path.rs). `CYCLE o SET _is_cycle USING _cycle_path` detects revisits of `o`, which in a DAG with shared ancestors falsely marks non-cyclic rows as cyclic and drops them. Fix: `CYCLE (s, o) SET _is_cycle` or switch to PG18’s proper path-tracking pattern. → *[v0.21.0](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness)*
- **H-6 · `DISTINCT ON` dedup in merge preserves oldest SID, but the view still unions `main + delta` without the same dedup** — [src/storage/merge.rs:240–255](../src/storage/merge.rs). Duplicates inserted across the merge boundary remain visible to SPARQL. Apply dedup in the view definition too, or rely on semantic uniqueness guaranteed by insert-side checks. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **H-7 · `rebuild_subject_patterns` reads the view while it is being recreated** — [src/storage/merge.rs:376–405](../src/storage/merge.rs). Query the `predicates` catalog first and target only dedicated VPs; skip `vp_rare` (which otherwise gets double-counted as its own VP + for every predicate sharing it). → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **H-8 · ORDER BY missing `NULLS FIRST/LAST`** — [src/sparql/sqlgen.rs:1853–1862](../src/sparql/sqlgen.rs). SPARQL 1.1 §15.1 requires `ASC` to place unbound last and `DESC` to place unbound first, but the generator emits bare `ASC` / `DESC`. PostgreSQL’s defaults are the opposite. Append `NULLS LAST` to `ASC` and `NULLS FIRST` to `DESC`. → *[v0.21.0](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness)*
- **H-9 · `REDUCED` is translated as `DISTINCT`** — [src/sparql/sqlgen.rs:1886](../src/sparql/sqlgen.rs). Spec-allowed conservative choice, but combined with the fragile self-join eliminator (below) this compounds into query slowdowns. Low-risk to change; either emit `DISTINCT` only (current) and document, or leave neither and document. → *[v0.21.0](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness)*
- **H-10 · Self-join elimination uses Debug string as dedup key** — [src/sparql/sqlgen.rs:430–450](../src/sparql/sqlgen.rs). Two structurally different triple patterns whose Debug output collides are silently elided; star-join patterns that happen to format identically would be skipped. Use structural key `(subject, predicate, object)` tuple after encoding. → *[v0.21.0](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness)*
- **H-11 · `OPTIONAL` over `GROUP BY`/`Aggregate` is a Cartesian product** — [src/sparql/sqlgen.rs:1710–1770](../src/sparql/sqlgen.rs). LeftJoin shared-variable detection assumes ≤1 right-side row per left row. Wrap aggregate sub-patterns as lateral subselects keyed on the shared vars. → *[v0.21.0](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness)*
- **H-12 · Federation cache key is XXH3-64** — [src/sparql/federation.rs:146–165](../src/sparql/federation.rs). 64-bit birthday bound (~2.1 G queries) is thin for a long-running cache. Use the full 128-bit hash or include the full query text as the key. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **H-13 · Federation partial-result recovery parser is a substring heuristic** — [src/sparql/federation.rs:330–360](../src/sparql/federation.rs). `rfind("},")` + appending `]}}}` can truncate a valid row containing a literal `"},"`. Either parse as a streaming JSON parser or refuse partial recovery for responses larger than N KB. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **H-14 · `pg_ripple_http` rate-limit GUC is parsed but never enforced** — [pg_ripple_http/src/main.rs:29, 79, 112](../pg_ripple_http/src/main.rs). `AppState.rate_limit` is dead code. Required for internet-facing deployment. Bolt on `tower_governor` or a simple in-memory token-bucket keyed on `X-Forwarded-For`. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **H-15 · `pg_ripple_http` error responses echo PostgreSQL error text verbatim** — [pg_ripple_http/src/main.rs:310, 346, 378, 408, 449](../pg_ripple_http/src/main.rs). Leaks schema names, GUC values, file paths. Redact in production; emit a stable error code + opaque trace id and log the full text server-side. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*

### Medium

- **M-1 · Division by zero in Datalog arithmetic is compiled straight to SQL `/`** — [src/datalog/compiler.rs:699](../src/datalog/compiler.rs). Aborts the ruleset with an opaque `division by zero` rather than a `datalog arithmetic: division by zero in rule X` message. Wrap rhs in `NULLIF(rhs, 0)` and null-propagate. → *[v0.23.0](../ROADMAP.md#v0230--shacl-core-completion--sparql-diagnostics)*
- **M-2 · Datalog unbound variables compile to `NULL`** — [src/datalog/compiler.rs:651](../src/datalog/compiler.rs). `WHERE x = NULL` always fails. Raise a compile-time error instead of silently producing a rule that matches nothing. → *[v0.23.0](../ROADMAP.md#v0230--shacl-core-completion--sparql-diagnostics)*
- **M-3 · Datalog stratifier: negation-through-cycle detection is single-edge** — [src/datalog/stratify.rs:138–160](../src/datalog/stratify.rs). A `:- NOT B`, `B :- NOT C`, `C :- NOT A` cycle may pass the check. Compute SCCs on the full dependency graph and reject any SCC with a negation edge. → *[v0.23.0](../ROADMAP.md#v0230--shacl-core-completion--sparql-diagnostics)*
- **M-4 · JSON-LD framing embedder can panic on empty result** — [src/framing/embedder.rs:109](../src/framing/embedder.rs). `roots.into_iter().next().unwrap()` panics if the compiled CONSTRUCT produces zero rows. Replace with `.ok_or_else()` returning an empty JSON-LD document. → *[v0.23.0](../ROADMAP.md#v0230--shacl-core-completion--sparql-diagnostics)*
- **M-5 · JSON-LD embedder has no per-node visited set** — [src/framing/embedder.rs](../src/framing/embedder.rs). Depth-limited (32) but can thrash on near-cyclic graphs; for correctness, track a `HashSet<NodeId>` per frame-descent branch as per W3C §4.1.3. → *[v0.23.0](../ROADMAP.md#v0230--shacl-core-completion--sparql-diagnostics)*
- **M-6 · `GROUP_CONCAT` ignores the `distinct` flag** — [src/sparql/sqlgen.rs:1169–1173](../src/sparql/sqlgen.rs). Emits `STRING_AGG(x, sep)` instead of `STRING_AGG(DISTINCT x, sep)`. Simple fix. → *[v0.21.0](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness)*
- **M-7 · Zero-length `p*` emits reflexive rows for all subjects** — [src/sparql/property_path.rs:155–185](../src/sparql/property_path.rs). The base relation is scanned twice (once for `1-hop`, once for `0-hop`); the 0-hop clause should be restricted to subjects that actually appear. Also a correctness concern: `p*` should return identities only for nodes in the query scope, not all nodes. → *[v0.21.0](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness)*
- **M-8 · Bulk load has no parse-error rollback** — [src/bulk_load.rs:1–50](../src/bulk_load.rs). Warns and continues past malformed triples; partial loads are committed. Offer a `strict := true` option that rolls back. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **M-9 · Blank-node document scoping uses wall-clock nanoseconds** — [src/bulk_load.rs:235–250](../src/bulk_load.rs). Two concurrent loads starting within <1 ns (possible under contention with coarser monotonic clocks on some platforms) can collide. Use the existing `_pg_ripple.statement_id_seq` or a dedicated `load_generation_seq`. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **M-10 · Export-Turtle literal escaping assumes N-Triples form is valid Turtle** — [src/export.rs:178](../src/export.rs). Mostly true, but Unicode escapes and some control-character forms differ; round-trip tests should cover `\uXXXX` + non-ASCII. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **M-11 · Turtle comment handling in SHACL module misses `/* … */`** — [src/shacl/mod.rs:165–200](../src/shacl/mod.rs). Not a Turtle feature strictly, but some authors include SPARQL-style block comments. Low priority. → *[v0.23.0](../ROADMAP.md#v0230--shacl-core-completion--sparql-diagnostics)*
- **M-12 · No validation bounds on GUC `pg_ripple.vp_promotion_threshold`** — [src/lib.rs:400–450](../src/lib.rs). Setting to 1 explodes the catalog; setting to i64::MAX disables VP-dedicated storage. Add `min = 10`, `max = 10_000_000`. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **M-13 · `pg_ripple_http` auth token comparison is non-constant-time** — [pg_ripple_http/src/main.rs:146–159](../pg_ripple_http/src/main.rs). Low real threat for HTTPS-only deployments, but trivial to fix with `constant_time_eq`. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **M-14 · Missing REVOKE on `_pg_ripple` schema** — [sql/pg_ripple--0.1.0.sql](../sql/pg_ripple--0.1.0.sql) and subsequent migrations. Default PostgreSQL behaviour for a schema created by superuser in the extension protects against `USAGE` for non-owners, but explicit `REVOKE ALL ON SCHEMA _pg_ripple FROM PUBLIC; REVOKE ALL ON ALL TABLES IN SCHEMA _pg_ripple FROM PUBLIC;` defends against later privilege drift. Add to the 0.20.0→1.0.0 migration. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **M-15 · Signal handling in merge worker doesn’t explicitly reset latch before sleep** — [src/worker.rs](../src/worker.rs). On back-off after errors the worker `std::thread::sleep`s while the latch is still set; next `wait_latch` returns immediately. Busy-loop on persistent failure. Fix: `reset_latch()` before `sleep`. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **M-16 · BRIN index on `s` (dictionary id) has weak correlation** — [src/storage/merge.rs:71–75](../src/storage/merge.rs). BRIN is effective for monotonic columns; dictionary IDs are monotonic across time but `s` within a predicate is effectively random. Consider moving BRIN to `i` (SID) and keeping B-tree on `(s, o)` / `(o, s)`. → *[v0.24.0](../ROADMAP.md#v0240--semi-naive-datalog--performance-hardening)*
- **M-17 · Datalog RL rule set is incomplete** — [src/datalog/builtins.rs:116–145](../src/datalog/builtins.rs). Missing `cax-sco` transitivity closure on subClassOf (partial), `cls-avf` chained allValuesFrom, `prp-ifp` inverseFunctional inferences, `prp-spo1` subPropertyOf in derived chains. Document the gap or close it. → *[v0.24.0](../ROADMAP.md#v0240--semi-naive-datalog--performance-hardening)*
- **M-18 · `SHACL` missing core constraints** — [src/shacl/mod.rs](../src/shacl/mod.rs). No `sh:hasValue`, `sh:closed`, `sh:ignoredProperties`, `sh:nodeKind`, `sh:languageIn`, `sh:uniqueLang`, `sh:lessThan`, `sh:greaterThan`, `sh:qualifiedValueShape`, and `sh:path` accepts only a single IRI — no inverse / alternative / sequence / `*`/`+`/`?`. → *[v0.23.0](../ROADMAP.md#v0230--shacl-core-completion--sparql-diagnostics)*
- **M-19 · Test suite marks `label_no_error` passes for unsupported SPARQL functions** — the “100% W3C conformance” number in CHANGELOG reflects these. Replace with proper feature detection or enumerate explicitly-deferred tests in a skip-list. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*

### Low

- **L-1 · `unsafe { pg_sys::MyProcPid }` etc. — all have SAFETY comments.** No findings; noted as a positive.
- **L-2 · CDC trigger payload uses raw i64s** — [src/cdc.rs:70–85](../src/cdc.rs). Subscribers need to call `decode_id` per field. Document in the user guide or add a `decode := true` variant that emits N-Triples payloads. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **L-3 · `ureq` pinned to v2** — [Cargo.toml](../Cargo.toml). v3 is the current major and has API changes. Not urgent; track for v1.x. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **L-4 · `oxttl`/`oxrdf` mentioned in [AGENTS.md](../AGENTS.md) but not in [Cargo.toml](../Cargo.toml)** — either adopt them for RDF-star (better spec fidelity than `rio_turtle` for star) or correct AGENTS.md. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **L-5 · GUC defaults not documented in their pgrx `.set_description()` strings** — [src/lib.rs:600–650](../src/lib.rs). Adds minor friction for operators. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **L-6 · Export helpers load the full graph into memory despite “streaming” naming** — [src/export.rs:135–200](../src/export.rs). Rework to use a cursor over VP tables keyed on SID ranges. → *[v0.24.0](../ROADMAP.md#v0240--semi-naive-datalog--performance-hardening)*
- **L-7 · Inline-encoding decoder doesn’t assert `is_inline(id)` before bit-unpacking** — [src/dictionary/inline.rs:120](../src/dictionary/inline.rs). Hard to trigger in practice (callers already discriminate) but a defensive `assert!` costs nothing. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*

---

## Performance Bottlenecks

- **P-1 · Direct-mapped shmem encode cache** — covered in H-1 / performance top 3. Change to 4-way set-associative. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **P-2 · N+1 SPI round-trips on cold dictionary lookups** — [src/dictionary/mod.rs](../src/dictionary/mod.rs). `insert_triple` hot path does 3 encodes × (shmem miss + local miss) = 3 SPI calls per triple. `batch_insert_encoded` already fixes this for bulk loads; the single-insert path does not. Consider batching `insert_triple` callers via a PL/pgSQL façade or exposing a `pg_ripple.insert_triples(TEXT[])` entry point. → *[v0.24.0](../ROADMAP.md#v0240--semi-naive-datalog--performance-hardening)*
- **P-3 · Datalog naive evaluation** — [src/datalog/compiler.rs](../src/datalog/compiler.rs). Large rule programs regenerate the full derived relation on every iteration. Implement semi-naive: maintain ΔR per iteration and only join against new deltas. → *[v0.24.0](../ROADMAP.md#v0240--semi-naive-datalog--performance-hardening)*
- **P-4 · Property-path `p*` / `p+` compile to `WITH RECURSIVE` without an upper-bound `LIMIT`** — [src/sparql/property_path.rs](../src/sparql/property_path.rs). On highly connected graphs (Wikidata), 10M-row intermediate states are possible. Add a configurable `pg_ripple.property_path_max_depth` GUC (suggested default: 64). → *[v0.24.0](../ROADMAP.md#v0240--semi-naive-datalog--performance-hardening)*
- **P-5 · Star-join patterns could collapse into a single scan with N joins on `(s)` alone** — the optimizer already reorders BGP, but the self-join elimination bug (H-10) may defeat it in cases where it should work. The SHACL `sh:maxCount 1` hint is documented in AGENTS.md as enabling `DISTINCT` elimination; verify it is applied by `translate_select`. → *[v0.23.0](../ROADMAP.md#v0230--shacl-core-completion--sparql-diagnostics)* (SHACL hints verified)*
- **P-6 · View-over-UNION-ALL query path materialises whole partitions** — every SPARQL query against `vp_N` reads `(main EXCEPT tombstones) UNION ALL delta`. `EXCEPT` forces a sort or hash; consider an `anti-join` rewrite (`main m LEFT JOIN tombstones t ON … WHERE t.s IS NULL`) and materialise the view via a function that returns a refcursor. → *[v0.24.0](../ROADMAP.md#v0240--semi-naive-datalog--performance-hardening)*

---

## Architectural Concerns

- **A-1 · The SPARQL translator is monolithic.** `src/sparql/sqlgen.rs` is ~2100 lines; expression translation, aggregate lowering, BGP reordering, path-to-CTE compilation, and plan-cache integration all share a single `Ctx`. Splitting expression translation into `src/sparql/expr.rs` (with a proper `SqlExpr` IR rather than raw strings) would enable a typed pass for built-ins and un-block C-1. → *[v0.21.0](../ROADMAP.md#v0210--sparql-built-in-functions--query-correctness)*
- **A-2 · No “decode at the end” boundary.** The SPARQL engine decodes results inside `sparql::execute` using per-row SPI lookups. For large result sets this dominates latency. A single batch-decode pass keyed on the distinct set of ids in the result would be a 5–20× speed-up depending on result size. Infrastructure exists (`batch_decode`) — wire it through. → *[v0.24.0](../ROADMAP.md#v0240--semi-naive-datalog--performance-hardening)*
- **A-3 · Merge worker shares backend-local caches across transactions.** The bg-worker does not invalidate its encode cache on cycle boundaries; if schema migrations rewrite dictionary rows (unlikely today, but a future feature), the merge worker will see stale IDs. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **A-4 · Views (v0.18) and framing views (v0.17) call into pg_trickle but don’t version-lock it.** Consider a feature-detection probe at extension load and a clear error if pg_trickle is newer than the tested version. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **A-5 · `_pg_ripple.predicates.table_oid` stores the view OID, which is stable until a view recreation.** C-3 is the immediate concern; even without it, an explicit `schema_name.table_name` TEXT column would be more robust than OID for a catalog that must survive migrations. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **A-6 · No query-planner/cost-model for SPARQL.** BGP reordering is implemented in [src/sparql/optimizer.rs](../src/sparql/optimizer.rs) and uses `predicates.triple_count` as selectivity, but there’s no join-order DP or cardinality model. This is acceptable for a query engine that delegates to PostgreSQL’s planner, but exposing `EXPLAIN SPARQL` (recommendation F-3) would let users work around pathological plans. → *[v0.24.0](../ROADMAP.md#v0240--semi-naive-datalog--performance-hardening)*

---

## Feature Gaps & Limitations

### Missing from SPARQL 1.1 Spec

- Built-in functions (see C-1 for the full list).
- Proper `ORDER BY` NULL semantics.
- `GROUP_CONCAT(DISTINCT …)`.
- Proper `REDUCED` semantics (currently equivalent to `DISTINCT`).
- `SERVICE SILENT` error-swallowing (check [src/sparql/federation.rs](../src/sparql/federation.rs) — not visibly implemented).
- Negated property sets (`!(p1|p2|…)`) — not visible in [src/sparql/property_path.rs](../src/sparql/property_path.rs).
- Custom aggregate function extension point.

### Missing from RDF 1.1 / SHACL / Datalog Specs

- SHACL: `sh:closed`, `sh:ignoredProperties`, `sh:hasValue`, `sh:nodeKind`, `sh:languageIn`, `sh:uniqueLang`, `sh:lessThan`, `sh:greaterThan`, `sh:qualifiedValueShape`, property paths in `sh:path`, recursive shape activation, severity propagation through nested shapes.
- Datalog: some OWL RL rules (`cax-sco` full closure, `cls-avf`, `prp-ifp`, `prp-spo1` in chains).
- RDF 1.1: generalised N-Triples-star round-trip via `oxttl` (currently `rio_turtle`, which has star support but divergent edge cases).

### Performance Limitations

- Recursive rule programs scale quadratically without semi-naive.
- Direct-mapped shmem cache thrashes beyond a few thousand hot terms.
- Large SPARQL result sets decode row-by-row.
- `p*`/`p+` paths unbounded.

---

## Security Findings

- **S-1 (High) · `pg_ripple_http` rate limit is a stub.** See H-14. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **S-2 (Medium) · No REVOKE on `_pg_ripple` schema.** See M-14. Defence-in-depth; relies on PostgreSQL’s default behaviour that is arguably adequate. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **S-3 (Medium) · HTTP error responses leak PG error text.** See H-15. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **S-4 (Medium) · Federation SSRF mitigation is allow-list only.** See [src/sparql/federation.rs:79–90](../src/sparql/federation.rs). `is_endpoint_allowed()` correctly checks `federation_endpoints`, but there is no scheme-validation on registration — a malicious `file://` or `gopher://` URL registered by a superuser would be accepted. `ureq` refuses non-HTTP, but belt-and-braces is cheap. Add scheme check at registration. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*
- **S-5 (Low) · HTTP auth token comparison is not constant-time.** See M-13. → *[v0.22.0](../ROADMAP.md#v0220--storage-correctness--security-hardening)*
- **S-6 (Positive) · Dictionary-encoding discipline for SQL injection is consistent.** All VP-table SQL receives encoded `i64` constants; dynamic table names use `format!("_pg_ripple.vp_{p_id}_delta")` where `p_id: i64`. No user-controlled string flows into a table or column identifier.
- **S-7 (Positive) · `unsafe {}` blocks all carry `// SAFETY:` comments** — verified in `src/worker.rs`, `src/shmem.rs`, `src/lib.rs`.
- **S-8 (Low) · File-path bulk loaders (`load_turtle_file` etc.) are gated on superuser.** Path-traversal and symlink following are not additionally validated; since only the superuser can call these, the risk is contained, but a `pg_read_server_files` role check would follow current PG best practice. → *[v0.25.0](../ROADMAP.md#v0250--geosparql--architectural-polish)*

---

## Recommendations for New Features

### High-Impact Features

1. **Complete the SPARQL 1.1 built-in function surface (v0.21.0).**
   - Rationale: turns "conformant on the test suite" into actually conformant. Addresses C-1.
   - User value: queries using standard SPARQL idioms stop silently over-returning.
   - Complexity: medium. ~40 functions; most compile to PostgreSQL equivalents (`LOWER`, `UPPER`, `NOW()`, `MD5`, `ABS`, `CEIL`, …). Type-checking and language-tag-aware variants are the tricky parts.
   - Dependencies: a typed `SqlExpr` IR (see A-1). Recommend carving that out first.
   - Estimated slot: **v0.21.0** (6–8 pw).

2. **SHACL Core completion (v0.23.0).**
   - Rationale: real-world SHACL schemas hit the gaps in M-18 immediately.
   - User value: Schema.org, SHACL-AF, standard shapes libraries work out of the box.
   - Complexity: medium. Most constraints are table scans with a predicate; `sh:closed` needs a dedicated "unknown predicate" detection pass.
   - Dependencies: none significant.
   - Estimated slot: **v0.23.0** (6–8 pw, bundled with SPARQL Diagnostics).

3. **SPARQL Explain / Query-Plan Introspection (v0.23.0).**
   - Rationale: users can't diagnose slow queries. Exposes the SQL generator's output + PG's EXPLAIN ANALYZE in one call.
   - User value: operator-grade debuggability; third parties can write plan-based linters.
   - Complexity: low. `pg_ripple.explain_sparql(query TEXT, format TEXT DEFAULT 'text')` wrapping `translate_select` + PG EXPLAIN.
   - Dependencies: none.
   - Estimated slot: **v0.23.0** (2–3 pw).

4. **Semi-naive Datalog evaluation (v0.24.0).**
   - Rationale: makes Datalog scale to Wikidata-size graphs.
   - User value: 10–100× faster OWL RL closure on larger datasets.
   - Complexity: medium-high. Rework [src/datalog/compiler.rs](../src/datalog/compiler.rs) to emit ΔR maintenance queries and a fixpoint loop.
   - Dependencies: correct negation-through-cycle stratification (M-3) first.
   - Estimated slot: **v0.24.0** (6–8 pw).

5. **GeoSPARQL 1.1 subset (v0.25.0).**
   - Rationale: PostGIS is already in the PostgreSQL ecosystem. A minimal GeoSPARQL adapter (WKT literals, `geo:sfIntersects`, `geo:sfContains`, `geof:distance`) lights up a large class of applications (LinkedGeoData, Wikidata geo queries).
   - User value: geo-aware RDF stores are a known pain point; pg_ripple could lead.
   - Complexity: medium if we require PostGIS at load time; lower if we ship custom xsd:wktLiteral handling and delegate to PostGIS opportunistically.
   - Estimated slot: **v0.25.0** (6–8 pw).

### Nice-to-Have Features

1. **SPARQL-star Update + quoted triples in CONSTRUCT templates.** [src/sparql/sqlgen.rs](../src/sparql/sqlgen.rs). SPARQL 1.2 / SPARQL-star has clear spec; partial support already via `KIND_QUOTED_TRIPLE`. Polishing this is low-effort and differentiates pg_ripple.
2. **RDF Patch / LD Patch import.** Companion to the existing Turtle/N-Triples/N-Quads loaders; useful for incremental syncs.
3. **GraphQL-over-RDF read-only adapter.** Thin layer on top of `pg_ripple_http` translating GraphQL SelectionSets to SPARQL SELECT. Very high demo value.
4. **Neo4j Bolt protocol read adapter.** Serve Cypher read queries by transpiling to SPARQL (work done in [plans/cypher/transpilation_reassessment.md](cypher/transpilation_reassessment.md)). A sidecar, not core.
5. **Schema-aware stats worker.** Periodic `ANALYZE` of VP tables with type-aware histograms (integer literals as numeric histograms, IRIs as n-distinct).
6. **Import from OWL ontology files + materialise class hierarchy.** Many users want RDFS closure without learning Datalog.
7. **`pg_ripple.canary()` function.** Runs a battery of internal self-checks (merge worker liveness, cache hit rate, catalog consistency) — great for ops dashboards.

---

## Maturity Assessment

### Current State (v0.20.0)

- Foundations (storage, dictionary, HTAP, VP) are in place and well-tested.
- SPARQL SELECT / ASK / CONSTRUCT / DESCRIBE / UPDATE are functional; property paths, aggregates, federation, views all work; FILTER is where the cliff is.
- SHACL covers the "obvious" constraint set; advanced constraints and path expressions are deferred.
- Datalog works end-to-end including RDFS and a useful subset of OWL RL.
- JSON-LD import/export/framing are implemented including W3C §4.1 embedding.
- `pg_ripple_http` is a functioning SPARQL Protocol 1.1 endpoint with metrics, auth, and CORS.
- Test surface is broad: 64 pg_regress files, crash recovery, migration chain, BSBM-100M.

### Path to v1.0.0

Critical blockers (must close):

1. C-1 — FILTER silent-drop / built-ins.
2. C-2 — dictionary cache rollback.
3. C-3, C-4 — merge race windows.
4. H-1, H-2 — shmem cache/bloom correctness.
5. H-3, H-4 — promotion atomicity + catalog accuracy.
6. H-14 — rate limit in HTTP service.
7. M-14 — explicit REVOKEs on `_pg_ripple`.

Highly recommended (should close):

- H-5, H-8 through H-12 (SPARQL correctness).
- M-18 (SHACL core completion).
- Replacement of `label_no_error` conformance shims with real assertions.

### Production Readiness

- **Stability**: Good for analytics workloads; the HTAP merge race windows (C-3, C-4) are real-world hazards for high-write OLTP-style usage. Low-traffic workloads are fine.
- **Performance**: Good single-query latency. Cache thrashing and naive Datalog become bottlenecks at scale.
- **Compliance**: Needs the C-1 fix to actually deserve the "100% W3C conformance" label.
- **Documentation**: Excellent in breadth and prose; occasional drift from code (oxttl, `rate_limit`).
- **Security**: Solid foundations; operational hardening (rate limit, REVOKE, constant-time compare, PG-error redaction) is the remaining work.

---

## Next Steps

1. **Open a "v1.0.0 blocker" tracking issue** linking to C-1 through C-4, H-1 through H-4, H-14, M-14. Assign an owner per item.
2. **Land C-2 (dictionary rollback) this week.** It is a correctness bug that needs an `xact_end_callback` and a pg_regress test that asserts a rolled-back `INSERT` cannot pollute the cache. Low effort, high impact.
3. **Carve out a `SqlExpr` IR for SPARQL expressions** (A-1) and migrate the existing Contains/StrStarts/etc. translators into it. Makes C-1 tractable as a steady backlog instead of a 40-function megaPR.
4. **Replace `label_no_error` placeholders** in `w3c_sparql_query_conformance.sql` with real assertions — skip tests for functions that aren't implemented yet and track them in a visible skip-list.
5. **Add a chaos test** that issues SPARQL SELECT in one backend while `just merge` runs in another (C-3, C-4 regression).
6. **Schedule a quick v0.20.1 security polish release**: REVOKEs on `_pg_ripple`, URL-scheme check on federation registration, constant-time HTTP auth compare, PG error-text redaction in HTTP responses. These are low-risk, user-visible wins.

---

## Appendix: Detailed Findings

### Test Coverage Analysis

- **Strong**: BGP, named graphs, aggregates, property paths (happy path), RDF-star, federation (with perf / timeout / injection), SHACL happy/malformed, Datalog RDFS/OWL RL/arithmetic/constraints/negation/views, JSON-LD framing, HTAP merge, crash recovery, upgrade path.
- **Gaps**:
  - No explicit test that a rolled-back dictionary insert cannot leak cached IDs.
  - No concurrency test interleaving `delete_triple` with an in-progress merge.
  - No explicit test for rare-predicate promotion racing with bulk inserts.
  - No test for SHACL recursion, `sh:closed`, `sh:hasValue`, property paths in `sh:path`.
  - No test for every SPARQL 1.1 built-in function (see C-1); the "100% W3C" file uses weak `label_no_error` assertions.
  - No test for `ORDER BY` with NULL bindings verifying spec-correct placement.
  - No privilege-escalation tests (unprivileged user attempting `DELETE` on `_pg_ripple.dictionary`).

### Performance Benchmarks

- Present: `benchmarks/insert_throughput.sql`, `benchmarks/ci_benchmark.sh`, `benchmarks/bsbm/*` at scale factors up to 30 (~100M).
- Missing: a "query latency under concurrent writes" benchmark (directly exercises HTAP delta scan + bloom + merge). Recommend a `bench-concurrent-qw.sh` that runs `pgbench` with a query mix on one side and `insert_triple` on the other.

### Specification Compliance Matrix (high-level)

| Spec | Coverage | Notes |
|---|---|---|
| SPARQL 1.1 Query | ~75% real / 100% test-suite | Built-in functions are the gap |
| SPARQL 1.1 Update | ~90% real | USING/WITH, ADD/COPY/MOVE implemented; good shape |
| SPARQL 1.1 Federation | ~80% | `SERVICE SILENT` unclear; pool + cache + batching strong |
| SPARQL 1.1 Protocol | ~90% | Content negotiation solid; rate-limit stub is the only glaring gap |
| RDF 1.1 | ~95% | Star round-trip might have edge cases vs oxttl |
| Turtle / TriG / N-Triples / N-Quads | ~100% | Happy path solid; escaping edge cases (M-10) |
| JSON-LD 1.1 Framing | ~90% | Embedder correctness caveats (M-4, M-5) |
| SHACL Core | ~60% | Missing closed shapes, hasValue, nodeKind, languageIn, uniqueLang, ordering, qualified counts |
| SHACL Advanced Features (AF) | partial | Rule execution and DAG monitoring present |
| Datalog + OWL RL | ~85% | A few rules missing; naive evaluation |

### Code Quality Metrics (impression-level)

- Module boundaries are clean; `src/{sparql,storage,dictionary,shacl,datalog,framing,export}` each own their domain.
- `sqlgen.rs` is the largest and least-decomposed module; splitting it (A-1) is the highest-leverage refactor.
- Error handling is consistent via [src/error.rs](../src/error.rs) (PT001–PT799 taxonomy).
- No `panic!` in hot paths; `unwrap`/`expect` are present but mostly behind invariants enforced at SPI boundaries (except M-4).
- Rust 2024 edition, pgrx 0.17 pinned, rust-version 1.88 — current and consistent.
