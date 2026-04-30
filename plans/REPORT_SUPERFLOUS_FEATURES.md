# Report: Superfluous Features & Removal Candidates for v1.0

> **Audience:** Maintainers preparing the v1.0.0 cut.
> **Scope:** A prioritized inventory of code, modules, plans, docs, and
> developer artefacts that look like good removal candidates because they are
> stubbed, experimental, niche, duplicated, or simply dead weight.
> **Authority:** This is an opinion paper — every item is a *candidate* and
> needs an explicit decision before deletion.

## Method

1. Walked every top-level directory and `src/` module.
2. Cross-referenced [src/feature_status.rs](src/feature_status.rs) for the
   honest status of each capability (`stub`, `experimental`, `planner_hint`,
   `planned`, `degraded`, `implemented`).
3. Grepped `#[allow(dead_code)]` annotations across the crate.
4. Compared the [ROADMAP.md](ROADMAP.md) section structure against the
   [plans/](plans/) directory and the contents of [docs/src/](docs/src/) and
   [blog/](blog/).
5. Looked for tracked developer artefacts (build logs, transient outputs)
   that should never have been committed.

## Summary

| Tier | Theme | LOC removable (approx.) | Candidates |
|---|---|---|---|
| **Tier 0** | Tracked developer junk | ~5 KB | 8 root files |
| **Tier 1** | Stubbed / planner-hint / dead-code modules | ~3 500 | 4–5 modules |
| **Tier 2** | Experimental late-cycle features built on optional deps | ~7 000 | bidi, kge, llm, citus, flight, replication |
| **Tier 3** | Niche feature modules with thin user demand | ~1 500 | r2rml, prov, temporal, tenant, ql_rewrite, sparqldl |
| **Tier 4** | Plans / docs / blog / examples backlog | ~8 000 lines | 11 PLAN_OVERALL_ASSESSMENT files, cypher plans, speculative plans, stale blogs |
| **Tier 5** | Adjacent ecosystem artefacts | n/a | dbt-pg-ripple, Helm chart, CloudNativePG, GraphRAG vocab |
| **Tier 6** | Small organisational cleanups | small | duplicated docs, vendored vocab in `sql/` |

Total addressable surface area: **~20 000 lines of code + ~100 markdown files
+ several adjacent components.** Trimming the top two tiers alone reduces
extension binary surface significantly and cuts the SQL API by dozens of
`#[pg_extern]` symbols.

---

## Tier 0 — Tracked developer junk (delete now, no risk)

These are tracked files that look like one-off debugging artefacts. They are
*not* in `.gitignore` but should be. Deleting them is mechanical.

| File | Why it should go |
|---|---|
| [build_output.txt](build_output.txt) | Cached `cargo build` stdout from 2026-04-21. |
| [cargo_check_output.txt](cargo_check_output.txt) | Cached `cargo check` output. |
| [clippy_all.txt](clippy_all.txt) | Cached `clippy` output. |
| [clippy_output.txt](clippy_output.txt) | Empty (0 bytes). |
| [check_test_output.sh](check_test_output.sh) | 204-byte ad-hoc helper script, not referenced by `justfile` or CI. |
| [sbom.json](sbom.json), [sbom_diff.md](sbom_diff.md) | SBOM artefacts; should be regenerated in CI, not committed. |
| [DEEP_ANALYSIS_PROMPT.md](DEEP_ANALYSIS_PROMPT.md) | Internal prompt scaffold for the agent, not user-facing. Move to `.github/skills/` or delete. |

**Action:** Delete and add the patterns to `.gitignore`. Generate `sbom.json`
in CI as a build artefact attached to GitHub releases.

---

## Tier 1 — Stubbed, dead-code, or planner-hint-only features

These features advertise a capability but the implementation is either
explicitly marked `#[allow(dead_code)]`, a `planner_hint` (not an executor),
or a `planned` placeholder. They are the highest-leverage cuts because they
represent *false advertising* on the v1.0 surface.

### 1.1 [src/sparql/sparqldl.rs](src/sparql/sparqldl.rs) — SPARQL-DL routing
- File starts with `#![allow(dead_code)]`.
- Targets OWL T-Box queries, but no end-to-end execution is wired through the
  optimizer.
- Documented as experimental in [ROADMAP.md](ROADMAP.md) v0.58.0 but never
  promoted to `implemented` in [src/feature_status.rs](src/feature_status.rs).
- **Recommendation:** delete the module, remove the OWL-routing branch from
  the planner, and document SPARQL-DL as out of scope for v1.0.

### 1.2 [src/sparql/wcoj.rs](src/sparql/wcoj.rs) — Worst-Case Optimal Joins
- `feature_status` entry: `"wcoj" → "planner_hint"` with the description
  *"a true Leapfrog Triejoin executor is not implemented"*.
- ~10×–100× speedup claim in the v0.36.0 release notes is not delivered by
  this code; only join-order reordering is.
- **Recommendation:** either retitle to `bgp_cycle_reorder` (small, honest) or
  remove and merge the small reorder hint into the existing
  [src/sparql/optimizer.rs](src/sparql/optimizer.rs). Delete the
  `sparql_wcoj.sql` regress test or rename it.

### 1.3 SHACL SPARQL **Rule** routing
- `feature_status` entry: `"shacl_sparql_rule" → "planned"` ("parsed and
  stored but not executed through the derivation kernel").
- The parsing path consumes lines in `src/shacl/` for a feature that does
  nothing.
- **Recommendation:** strip the parser branch for `sh:SPARQLRule` and emit a
  `feature_unsupported` notice at registration time.

### 1.4 SPARQL 1.2
- `feature_status` entry: `"sparql_12" → "planned"`. Blocked on upstream
  `spargebra`. The `sparql-12` feature flag is enabled on `spargebra`/`sparopt`
  in [Cargo.toml](Cargo.toml#L23-L24) for no functional benefit.
- **Recommendation:** drop the `sparql-12` feature flag until the grammar
  ships upstream. Delete [plans/sparql12_tracking.md](plans/sparql12_tracking.md)
  or move it to `plans/future-directions.md`.

### 1.5 Datalog dead-code surface
- `grep -n '#\[allow(dead_code)\]' src/datalog/` returns 20+ hits across
  [cache.rs](src/datalog/cache.rs), [stratify.rs](src/datalog/stratify.rs),
  [compiler.rs](src/datalog/compiler.rs), [magic.rs](src/datalog/magic.rs),
  [demand.rs](src/datalog/demand.rs), [coordinator.rs](src/datalog/coordinator.rs),
  [parallel.rs](src/datalog/parallel.rs), [lattice.rs](src/datalog/lattice.rs).
- Some are legitimate forward declarations; many are stale.
- **Recommendation:** audit each `dead_code` allow and either wire it up,
  expose it via `#[pg_extern]`, or delete it. Probable yield: 500–1 000 LOC.

---

## Tier 2 — Late-cycle experimental features (highest weight to remove)

Each item below was added in the v0.5x – v0.7x cycle, is marked
`experimental` in [src/feature_status.rs](src/feature_status.rs), depends on
an optional extension that is not always installed, and adds a large surface
to maintain across migrations, SHACL/Datalog interactions, and HTTP routes.

### 2.1 Bidirectional integration primitives — [src/bidi.rs](src/bidi.rs) *(2 491 LOC)*
- Largest single source file in the crate.
- Implements 15 BIDI-* features (conflict policies, normalize expressions,
  upsert, diff, delete, ref, loop-safe subscriptions, CAS, linkback, outbox,
  inbox, wire format, observability, performance budget).
- Heavy dependency on pg-trickle (`outbox`/`inbox`).
- **Recommendation:** move `pg_ripple_bidi` to a separate companion crate (or
  defer to v1.1.0). The base extension does not need to ship 15 BIDI knobs to
  reach v1.0. Spec already lives at
  [docs/spec/rdf-bidi-integration-v1.md](docs/spec/rdf-bidi-integration-v1.md)
  so the design is preserved.

### 2.2 Knowledge-Graph Embeddings — [src/kge.rs](src/kge.rs) *(486 LOC)*
- Whole module gated by `#![allow(dead_code)]`.
- Requires pgvector; degrades silently when missing.
- TransE/RotatE training inside a PostgreSQL extension is an unusual and
  unscalable place for SGD; users with embedding workloads run them outside
  PG.
- **Recommendation:** remove the module, drop the
  `_pg_ripple.kge_embeddings` table from the v1.0.0 migration, and document
  as out-of-scope. Keep the simpler `embeddings` table used by hybrid search.

### 2.3 LLM bridge — [src/llm/mod.rs](src/llm/mod.rs) *(966 LOC)*
- Entries `llm_sparql_repair`, `sparql_nl_to_sparql`, and the `suggest_sameas`
  alignment pipeline are all `experimental` and degrade silently with no LLM
  endpoint.
- A 1k-LOC HTTP/JSON LLM client inside a database extension is a maintenance
  burden (TLS pinning, prompt-injection handling, model drift, rate-limit
  semantics).
- **Recommendation:** extract to a separate crate or to `pg_ripple_http` and
  leave the database extension to receive the rewritten SPARQL via SPI. Cuts
  the dep surface meaningfully.

### 2.4 Citus integration — [src/citus.rs](src/citus.rs) *(1 333 LOC)*
- Five Citus features in [src/feature_status.rs](src/feature_status.rs); only
  `citus_brin_summarise` is `implemented`. The other four
  (`citus_service_pruning`, `citus_hll_distinct`, `citus_nonblocking_promotion`,
  `citus_rls_propagation`, `citus_multihop_pruning`) are `experimental` with
  notes saying *"Full multi-node infrastructure required for end-to-end
  testing"*.
- The corresponding regress tests (`citus_*.sql`) cover SQL-only smoke paths,
  not real cluster behaviour.
- **Recommendation:** keep the BRIN-summarise glue and the small Citus
  detection helper; move the rest behind a `pg_ripple_citus` companion crate
  loaded only when Citus is detected. Strip the 4 unproven features from the
  v1.0 surface.

### 2.5 Arrow Flight bulk export — [src/flight.rs](src/flight.rs) + `pg_ripple_http/src/arrow_encode.rs`
- `feature_status` entry: `"arrow_flight" → "experimental"`. Requires
  `pg_ripple.arrow_flight_secret`, HMAC ticket plumbing, and a streaming
  encoder that replicates work the upcoming PG18 logical decoding bridge
  already does.
- **Recommendation:** defer to v1.1. Keep the `parquet` dep only if BYOG
  GraphRAG export still needs it; otherwise drop the `parquet` workspace
  dependency too.

### 2.6 RDF logical replication — [src/replication.rs](src/replication.rs)
- Implements an `apply_worker` subscriber to a PG logical replication slot.
- Conflicts with the v0.54.0 PG18 logical-decoding RDF bridge work (the
  `replication.rs` worker is the "previous" approach).
- **Recommendation:** consolidate into the PG18 replication path or drop one
  of the two. Two parallel replication implementations doubles support cost.

### 2.7 Live SPARQL subscriptions — [src/subscriptions.rs](src/subscriptions.rs)
- `feature_status`: `"sparql_subscription" → "experimental"`. Pushes full
  serialised SPARQL results through `pg_notify` (with an 8 KB cap and
  `{"changed":true}` fallback). Cute, but not production-shaped.
- **Recommendation:** keep behind a GUC turned `off` by default for v1.0; or
  extract to `pg_ripple_http` SSE only.

### 2.8 CDC bridge — [src/cdc_bridge_api.rs](src/cdc_bridge_api.rs) + [src/storage/cdc_bridge.rs](src/storage/cdc_bridge.rs)
- pg-trickle outbox bridge, CDC lifecycle events. Overlaps significantly with
  Tier 2.1 (bidi outbox) and Tier 2.6 (replication). Pick one.

### 2.9 JSON Mapping — [src/json_mapping.rs](src/json_mapping.rs)
- `feature_status`: `"json_mapping" → "experimental"`.
- Function set overlaps with [src/framing/](src/framing/) (JSON-LD framing)
  and the JSON-LD ingest path.
- **Recommendation:** fold the useful pieces into `framing` and remove the
  separate `register_json_mapping()` registry.

---

## Tier 3 — Niche feature modules with thin demonstrated demand

These modules each implement a real spec/feature, but with low usage signal
and overlap with simpler primitives that already exist.

| Module | LOC | Why it's a candidate |
|---|---|---|
| [src/r2rml.rs](src/r2rml.rs) | 290 | R2RML Direct Mapping — niche; covered better by `pg-trickle` + JSON mapping for most users. Plan ([plans/r2rml_virtual.md](plans/r2rml_virtual.md)) is unimplemented. |
| [src/prov.rs](src/prov.rs) | 203 | Bulk-load PROV-O emissions; `prov_enabled` GUC defaults `off`. Could be a recipe in the cookbook. |
| [src/temporal.rs](src/temporal.rs) | 194 | `point_in_time(ts)` GUC + `validFrom`/`validThrough` rewrite. Niche; SID-based time travel is fragile across promotions. |
| [src/tenant.rs](src/tenant.rs) | 220 | `create_tenant` is a thin wrapper over `grant_graph_access` from v0.55.0. |
| [src/sparql/ql_rewrite.rs](src/sparql/ql_rewrite.rs) | ~? | OWL 2 QL rewriter. The product positions itself around OWL 2 RL via Datalog; QL adds a second profile path. |
| [src/sparql/embedding.rs](src/sparql/embedding.rs) | ~? | OpenAI-compatible embedding client living *inside* the SPARQL crate. Cross-cutting concern; probably belongs in `pg_ripple_http` or a tool. |

**Recommendation:** for each, run a one-week telemetry/usage poll among
existing pilots; if no concrete deployment depends on it, retire to a
companion crate or delete.

---

## Tier 4 — Plans, docs, blog, examples (low risk, large surface)

### 4.1 Stale assessment plans
There are **eleven** numbered overall-assessment files:

- [plans/PLAN_OVERALL_ASSESSMENT.md](plans/PLAN_OVERALL_ASSESSMENT.md)
- [plans/PLAN_OVERALL_ASSESSMENT_2.md](plans/PLAN_OVERALL_ASSESSMENT_2.md)
- [plans/PLAN_OVERALL_ASSESSMENT_3.md](plans/PLAN_OVERALL_ASSESSMENT_3.md)
- [plans/PLAN_OVERALL_ASSESSMENT_4.md](plans/PLAN_OVERALL_ASSESSMENT_4.md)
- [plans/PLAN_OVERALL_ASSESSMENT_6.md](plans/PLAN_OVERALL_ASSESSMENT_6.md)
- [plans/PLAN_OVERALL_ASSESSMENT_7.md](plans/PLAN_OVERALL_ASSESSMENT_7.md)
- [plans/PLAN_OVERALL_ASSESSMENT_8.md](plans/PLAN_OVERALL_ASSESSMENT_8.md)
- [plans/PLAN_OVERALL_ASSESSMENT_9.md](plans/PLAN_OVERALL_ASSESSMENT_9.md)
- [plans/PLAN_OVERALL_ASSESSMENT_10.md](plans/PLAN_OVERALL_ASSESSMENT_10.md)
- [plans/PLAN_OVERALL_ASSESSMENT_11.md](plans/PLAN_OVERALL_ASSESSMENT_11.md)
- [plans/PLAN_DOCUMENTATION_GAPS_1.md](plans/PLAN_DOCUMENTATION_GAPS_1.md)

The most recent (`_11`) is the only one with active follow-ups; the rest are
historical. **Recommendation:** archive #1–#10 under
`plans/archive/assessments/` (or delete) and keep only the active one.

### 4.2 Speculative / never-shipped plans
Candidates to delete or move under `plans/future-directions.md` as
single-paragraph stubs:

- [plans/cypher.md](plans/cypher.md) and [plans/cypher-gql-transpiler.md](plans/cypher-gql-transpiler.md) and [plans/cypher/](plans/cypher/) — Cypher/GQL is *not* in the roadmap.
- [plans/storage-tiering-slatedb-duckdb.md](plans/storage-tiering-slatedb-duckdb.md) — speculative; out of v1.0 scope.
- [plans/link_prediction.md](plans/link_prediction.md), [plans/neuro-symbolic-record-linkage.md](plans/neuro-symbolic-record-linkage.md), [plans/graphrag.md](plans/graphrag.md), [plans/vector_sparql_hybrid.md](plans/vector_sparql_hybrid.md) — already implemented or speculative.
- [plans/postgresql-triplestore-deep-dive.md](plans/postgresql-triplestore-deep-dive.md), [plans/postgresql-native-partitioning.md](plans/postgresql-native-partitioning.md), [plans/r2rml_virtual.md](plans/r2rml_virtual.md) — research notes; move to `docs/src/research/`.
- [plans/agent_skills.md](plans/agent_skills.md) — agent operations note; should live under `.github/skills/` or AGENTS.md.
- [plans/ecosystem/](plans/ecosystem/) — competitive landscape notes; great content but doesn't belong in `plans/`.

### 4.3 Blog posts
[blog/](blog/) contains **35** posts (~352 KB). Several promote features
listed for removal in Tiers 1–3:

- [blog/leapfrog-triejoin.md](blog/leapfrog-triejoin.md) — promotes WCOJ as
  a real executor; misleading per [src/feature_status.rs](src/feature_status.rs).
- [blog/probabilistic-datalog.md](blog/probabilistic-datalog.md) — there is
  no probabilistic Datalog in the codebase.
- [blog/neuro-symbolic-entity-resolution.md](blog/neuro-symbolic-entity-resolution.md) — depends on KGE/LLM modules in Tier 2.
- [blog/r2rml-relational-to-graph.md](blog/r2rml-relational-to-graph.md) — depends on R2RML (Tier 3.1).
- [blog/citus-shard-pruning-sparql.md](blog/citus-shard-pruning-sparql.md) — depends on `citus_service_pruning` (experimental in Tier 2.4).
- [blog/temporal-time-travel-queries.md](blog/temporal-time-travel-queries.md) — depends on `temporal.rs` (Tier 3).
- [blog/cdc-knowledge-graphs.md](blog/cdc-knowledge-graphs.md), [blog/ivm-pg-trickle-integration.md](blog/ivm-pg-trickle-integration.md), [blog/semantic-hub-trickle-relay.md](blog/semantic-hub-trickle-relay.md), [blog/bidi-relay-throughput.md](blog/) — coupled to bidi/CDC/replication.

**Recommendation:** every blog post that describes a feature being trimmed
must be deleted in the same commit as the code, or kept with an
"Implementation status" disclaimer at the top.

### 4.4 Examples
[examples/](examples/) (17 files) contains several files coupled to the
Tier 2 modules — `cdc_subscription.sql`, `citus_rebalance_with_trickle.sql`,
`replication_setup.sql`, `llm_workflow.sql`, `sparql_repair.sql`,
`graphrag_round_trip.sql`, `probabilistic_rules.sql`. Delete with their
parent module if those modules are removed.

### 4.5 Docs
[docs/src/](docs/src/) is 33 199 lines across 153 markdown files. No
specific page deletions are recommended without inspecting them, but the
general rule is: every Tier 1–3 module deletion must drag down its
`docs/src/reference/*.md` page and any `docs/src/features/*.md` chapter.

---

## Tier 5 — Adjacent ecosystem artefacts

These live alongside the extension; each one is *a separate product* that the
core team has signed up to maintain.

### 5.1 [clients/dbt-pg-ripple/](clients/dbt-pg-ripple/) — dbt adapter
- Tiny (one adapter, one test). No traction signal.
- **Recommendation:** move to its own repo (`grove/dbt-pg-ripple`) and let it
  evolve independently.

### 5.2 [charts/pg_ripple/](charts/pg_ripple/) — Helm chart
- 7 templates. Helm charts go stale fast and require their own release
  cadence.
- **Recommendation:** move to a `grove/pg-ripple-helm` repo and reference it
  from the docs.

### 5.3 [docker/Dockerfile.cnpg](docker/Dockerfile.cnpg) — CloudNativePG image
- Ships an OpenShift/K8s-targeted image alongside the standalone
  [Dockerfile](Dockerfile).
- **Recommendation:** keep only one image in v1.0; CNPG can be a separate
  release artefact.

### 5.4 GraphRAG ontology files in `sql/`
[sql/graphrag_enrichment_rules.pl](sql/graphrag_enrichment_rules.pl),
[sql/graphrag_ontology.ttl](sql/graphrag_ontology.ttl),
[sql/graphrag_shapes.ttl](sql/graphrag_shapes.ttl) — these are *vocab*, not
extension SQL. They live in `sql/` because that directory is `EXTENSION`'s
install dir, but PG's extension installer copies the entire directory.
**Recommendation:** move to [sql/vocab/](sql/vocab/) (which already exists)
or to [examples/](examples/).

### 5.5 [sql/basic_crud.sql](sql/basic_crud.sql), [sql/dictionary.sql](sql/dictionary.sql)
- Standalone demo scripts in the extension SQL directory. Same problem as
  5.4: PG installs them. **Recommendation:** move to [examples/](examples/).

---

## Tier 6 — Small organisational cleanups

| Item | Action |
|---|---|
| Tracked [pg_regress_results/regression.diffs](pg_regress_results/regression.diffs) | The directory is in `.gitignore` but `regression.diffs` and `regression.out` are still tracked. Remove from history. |
| Many top-level `.md` files (`AGENTS.md`, `RELEASE.md`, `CONTRIBUTING.md`, `ROADMAP.md`, `README.md`, `CHANGELOG.md`, `DEEP_ANALYSIS_PROMPT.md`) | Move dev-facing docs (`AGENTS.md`, `RELEASE.md`, `DEEP_ANALYSIS_PROMPT.md`) under `.github/` or `docs/internal/`. |
| `src/stats.rs` (4 KB) vs `src/stats_admin.rs` (20 KB) | Two files, one concept. Merge. |
| `src/views.rs` + `src/views_api.rs` and similar `_api.rs` siblings | Eight `_api.rs` modules contain only `#[pg_extern]` shims. Consider rolling them into their feature module behind a `mod api;` to halve the file count. |
| 81 SQL migration files in `sql/` | Compact early migrations (`pg_ripple--0.1.0--*.sql` … `pg_ripple--0.5.x--*.sql`) into a single `pg_ripple--0.x--1.0.0.sql` "from-scratch" path for v1.0. Keep the granular paths for users on intermediate versions, but stop accruing one per release. |
| 128 GUCs registered in [src/gucs/registration.rs](src/gucs/registration.rs) (1 815 lines) | Audit for unused / "experimental-only" GUCs and move them under a `pg_ripple.experimental.*` namespace. |
| 17 `roadmap/` versions with "full" plus "summary" markdown | Once tagged, the "plan" version of a release becomes historical; move tagged plans into `roadmap/archive/`. |
| [tests/jena/](tests/jena/) (7.5 MB) | Conformance vendor data. Keep as a submodule or download in CI rather than in the repo. |
| [tests/w3c/](tests/w3c/) (5.0 MB) | Same — fetch via [scripts/fetch_w3c_tests.sh](scripts/fetch_w3c_tests.sh) on demand. |

---

## Suggested ordering

1. **Day 0 (no risk):** Tier 0 + Tier 6 (junk files, vendored test fixtures).
2. **Day 1 (no public-API impact):** Tier 1 (stubs, dead code, planner-hint
   relabelling, `sparql-12` feature flag).
3. **Day 2 (deprecate-then-remove):** Tier 2.5 (Arrow Flight), 2.6
   (replication.rs duplicate), 2.7 (subscriptions), 2.8 (CDC bridge), 2.9
   (json_mapping). Mark deprecated in v0.79.x; delete in v1.0.0.
4. **v1.0.0 cut:** Tier 2.1 (bidi → companion crate), 2.2 (KGE → delete),
   2.3 (LLM → companion crate), 2.4 (Citus → companion crate).
5. **Post-1.0 housekeeping:** Tier 3 modules; Tier 5 ecosystem extractions;
   Tier 4 doc/blog/example sweeps.

---

## Risks and counter-arguments

- **Marketing surface vs. production surface.** Several Tier 2/3 features
  exist primarily to support blog posts and the "what makes pg_ripple
  different" pitch. Cutting them shrinks the marketing story; keep this
  constraint in mind when removing per item.
- **Reversibility.** Modules deleted in v1.0 can be restored from git
  history, but their *catalog tables* (e.g. `_pg_ripple.kge_embeddings`)
  cannot be silently re-added later without a migration. Move the
  table-creation DDL to the migration that drops the module so the rollback
  path is tested.
- **Citus and pg-trickle integration.** Both extensions are first-party
  ecosystem siblings; users may legitimately expect them in the box. Splitting
  them into companion crates is preferable to deletion.
- **HTTP companion (`pg_ripple_http`) coupling.** Several Tier 2 features
  have HTTP route handlers in
  [pg_ripple_http/src/routing.rs](pg_ripple_http/src/routing.rs); deletions
  must remove the corresponding routes in lockstep, or the COMPAT-01 version
  gate will report missing endpoints.

---

## Concrete first PR (proposed)

A single pull request with no behavioural impact and an immediately visible
LOC reduction:

1. Delete Tier 0 files and update [.gitignore](.gitignore).
2. Delete Tier 1.1 ([src/sparql/sparqldl.rs](src/sparql/sparqldl.rs)) and
   the `sparql_12` feature flag in [Cargo.toml](Cargo.toml).
3. Archive Tier 4.1 (move the ten old `PLAN_OVERALL_ASSESSMENT_*.md` files
   under `plans/archive/assessments/`).
4. Move Tier 5.4–5.5 SQL fixtures out of `sql/`.
5. Remove tracked [pg_regress_results/regression.diffs](pg_regress_results/regression.diffs)
   and [pg_regress_results/regression.out](pg_regress_results/regression.out).

Net outcome: smaller install footprint, clearer `plans/` directory, no
behaviour change.

---

*Generated as a planning artefact; not authoritative. Discuss each tier
before acting.*
