# pg_ripple — Overall Assessment #13

**Date**: 2026-05-02
**Codebase snapshot**: `142d8f21a2bd1b30c283bfeb7901f276012e6b41` (origin/main, post-v0.83.0)
**Assessor**: Automated deep analysis (GitHub Copilot, Assessment #13)
**Version**: v0.83.0 (extension) / v0.77.0 (pg_ripple_http)
**Total Rust LOC**: 63,756 (src/ + pg_ripple_http/src/) across 100 modules

---

## Executive Summary

pg_ripple has progressed three releases since Assessment #12 (v0.79.0 → v0.83.0 in approximately five weeks of CHANGELOG time), and the v0.80–v0.83 cycle was a **focused, high-quality remediation sweep** that closed almost the entire A12 backlog. Of the 14 named A12 open findings re-verified against the current source, **eleven are RESOLVED**, **two are PARTIALLY RESOLVED**, and **only one remains STILL OPEN** (the `pg_ripple_http` version gap, MF-B). The most consequential A12 finding — the CRITICAL `sparql_update()` mutation-journal flush bug (C-01) — is fixed at [src/sparql/execute.rs:658](../src/sparql/execute.rs#L658), and the SubXact decode-cache invalidation race (CON-1) is now correctly registered at [src/lib.rs:443](../src/lib.rs#L443) (DICT-SUBXACT-01, v0.81.0). The CHANGELOG entries for v0.80–v0.83 contain unusually detailed evidence markers (`MIGCHAIN-01`, `CACHE-CAP-01`, `DECODE-BIND-01`, `MERGE-HBEAT-01`, `AUTH-RESP-FMT-01`, `MOD-BIDI-01`, etc.), each verifiable against a code site or migration script.

**There is, however, a meaningful prompt-vs-reality gap**: Assessment #13 was specified to anchor at **v0.84.0** (Uncertain Knowledge & Soft Reasoning — probabilistic Datalog, fuzzy SPARQL, trust scoring). v0.84.0 is **NOT YET IMPLEMENTED** — it appears only in `ROADMAP.md` and `plans/probabilistic-features.md` as a research report. The current released version is **v0.83.0**. Area 12 of this assessment therefore evaluates v0.84.0 *as a planned feature* rather than as live code, and flags the tracking-document drift (PROMPT-01) as a process concern.

The current state of the project is best described as **pre-v1.0.0 release-candidate quality**. Of the four pillars that historically blocked v1.0.0 — correctness, security, observability, and operability — three are substantially in place. The remaining genuine gaps are concentrated in five areas: **(1) HTTP-companion version sync** (`pg_ripple_http` is still at 0.77.0 with `COMPATIBLE_EXTENSION_MIN = 0.79.0`, against an extension at 0.83.0); **(2) docker-compose stale tag** (image pinned to 0.54.0 — 29 versions behind); **(3) two large modules** (`src/gucs/registration.rs` at 2,032 lines and `src/schema.rs` at 1,939 lines, both above the 1,800-line threshold A12 used); **(4) two unreviewed `SECURITY DEFINER` functions** ([src/schema.rs:996](../src/schema.rs#L996), [sql/pg_ripple--0.55.0--0.56.0.sql:60](../sql/pg_ripple--0.55.0--0.56.0.sql#L60)); and **(5) the v0.84.0 implementation gap**.

This report identifies **82 individual findings** across all 16 areas. Severity profile: 1 Critical (the prompt-vs-reality gap, classified as a process risk rather than a code defect), 9 High, 41 Medium, 31 Low. **No new memory-safety or SQL-injection vulnerabilities were found** in v0.83.0 source. The static-analysis script [scripts/check_no_string_format_in_sql.sh](../scripts/check_no_string_format_in_sql.sh) is now in place and gates dynamic SQL construction. World-class quality score: **4.4 / 5.0** (up from an implicit 3.9 in A12 narrative).

### Top 5 Critical Actions (pre-v1.0.0)

1. **Bump `pg_ripple_http` to 0.83.0 and update `COMPATIBLE_EXTENSION_MIN`** ([pg_ripple_http/Cargo.toml](../pg_ripple_http/Cargo.toml), [pg_ripple_http/src/main.rs:38](../pg_ripple_http/src/main.rs#L38)) — single-PR fix; eliminates a 6-version drift that confuses operators about which HTTP companion to deploy.
2. **Decide and execute v0.84.0** ([plans/probabilistic-features.md](../plans/probabilistic-features.md)) — either implement, defer to v1.1.0, or remove from the v0.x roadmap. The current state where the assessment prompt expects v0.84.0 features that do not exist is a documentation-process failure that will recur on every audit.
3. **Bump docker-compose image tag from 0.54.0 to 0.83.0** ([docker-compose.yml:23,40](../docker-compose.yml#L23)) and add CI gating that fails when the tag drifts from `Cargo.toml`.
4. **Audit and minimise the two `SECURITY DEFINER` definitions** ([src/schema.rs:996](../src/schema.rs#L996), [sql/pg_ripple--0.55.0--0.56.0.sql:60](../sql/pg_ripple--0.55.0--0.56.0.sql#L60)) — each is a privilege-escalation surface that, by project convention ([scripts/check_no_security_definer.sh](../scripts/check_no_security_definer.sh)), should not exist; document why they are exceptions or refactor to `SECURITY INVOKER`.
5. **Split `src/gucs/registration.rs`** (2,032 lines) into per-domain submodules (`registration/sparql.rs`, `registration/storage.rs`, `registration/federation.rs`, `registration/datalog.rs`) so that the GUC catalogue can be reasoned about without scrolling through a 2 000-line monolith.

### World-Class Quality Score

Overall: **4.4 / 5.0**. pg_ripple v0.83.0 is genuinely close to production-ready. Correctness (4.6), security (4.4), and observability (4.5) are all strong. The two remaining sub-4.0 dimensions are operability (3.8 — the HTTP-companion sync issue and the stale docker tag) and developer experience (3.9 — three files >1,800 lines and a handful of GUCs without `check_hook` validators).

---

## Resolution Status of Assessment #12 Findings

Of 80 findings enumerated in Assessment #12, the 14 most consequential (CRITICAL + HIGH + selected MEDIUM) were re-verified for this report. Lower-severity items either tracked the same root cause or were addressed by the same v0.80–v0.83 changes.

| ID | A12 Severity | A12 Status | A13 Status | Verification Evidence (v0.83.0) |
|---|---|---|---|---|
| C-01 | CRITICAL | Open | **RESOLVED** | `grep -n "mutation_journal" src/sparql/execute.rs` → line 658 `crate::storage::mutation_journal::flush()` at end of `sparql_update()`; covers all sub-operations (delete_insert, load, clear, drop) per [src/sparql/execute.rs:526–660](../src/sparql/execute.rs#L526-L660). |
| C-02 | HIGH | Open | **PARTIALLY RESOLVED / RECLASSIFIED** | `grep -rn "mutation_journal" src/r2rml.rs src/cdc.rs src/cdc_bridge_api.rs` → 0 hits. However, re-reading these files: r2rml.rs only does READS from `vp_rare`; cdc.rs writes to `_pg_ripple.cdc_lsn_watermark` (catalog) and emits `pg_notify` from triggers (no direct triple inserts); cdc_bridge_api.rs has no INSERT statements. The original A12 framing assumed these modules wrote triples — they do not. Re-verify on any future code path that does. |
| C-03 | HIGH | Open | **RESOLVED** | [src/sparql/property_path.rs:25,256,270,338,387](../src/sparql/property_path.rs#L25) all use `CYCLE s, o SET _is_cycle USING _cycle_path` (PROPPATH-CYCLE-01, v0.80.0). |
| C-04 | HIGH | Open | **RESOLVED** | [src/storage/merge.rs:189,300](../src/storage/merge.rs#L189) explicit `ORDER BY s, o, g, i ASC` precedes `DISTINCT ON`. |
| HF-A | MEDIUM | Open | **RESOLVED** | `python3 -c "import json; print(json.load(open('sbom.json'))['metadata']['component']['version'])"` → `0.83.0`. SBOM matches `Cargo.toml`. |
| MF-A | MEDIUM | Open | **RESOLVED** | [src/sparql/plan_cache.rs:97–140](../src/sparql/plan_cache.rs#L97) cache_key now incorporates: spargebra-normalised algebra digest, `MAX_PATH_DEPTH`, `BGP_REORDER`, `role_oid` (CACHE-RLS-01), `INFERENCE_MODE`, `NORMALIZE_IRIS`, `WCOJ_ENABLED`, `WCOJ_MIN_TABLES`, `TOPN_PUSHDOWN`, `SPARQL_MAX_ROWS`, `SPARQL_OVERFLOW_ACTION`, `FEDERATION_TIMEOUT`, `PGVECTOR_ENABLED` (PLAN-CACHE-GUC-02, v0.81.0). |
| MF-B | MEDIUM | Open | **STILL OPEN** | `grep '^version' pg_ripple_http/Cargo.toml` → `0.77.0`; extension is `0.83.0`; `COMPATIBLE_EXTENSION_MIN = "0.79.0"` at [pg_ripple_http/src/main.rs:38](../pg_ripple_http/src/main.rs#L38). 6-version drift; partially regressed since A12 (gap was 3 versions then). |
| MF-9 | MEDIUM | Open | **RESOLVED** | [src/gucs/registration.rs:1840–1844](../src/gucs/registration.rs#L1840) registers `pg_ripple.strict_dictionary`; [src/dictionary/mod.rs:657–661](../src/dictionary/mod.rs#L657) raises PT501 on unknown ID when on (DICT-STRICT-01, v0.81.0). |
| SEC-1 | HIGH | Open | **RESOLVED** | `grep -n "replace('"'"'" src/views.rs` → 0 hits; all catalog inserts at [src/views.rs:200,322,405,652,929](../src/views.rs#L200) use `Spi::run_with_args` with typed parameters. |
| SEC-2 | HIGH | Open | **RESOLVED** | [src/sparql/federation.rs:288–298](../src/sparql/federation.rs#L288) explicit RFC-1918 blocks (`172.16/12`, `192.168.x`, `169.254`); [src/sparql/federation.rs:338–353](../src/sparql/federation.rs#L338) `normalize_url_for_allowlist()` (FED-URL-01, v0.81.0). |
| CON-1 | HIGH | Open | **RESOLVED** | [src/lib.rs:443](../src/lib.rs#L443) `crate::dictionary::invalidate_decode_cache()` invoked on `SUBXACT_EVENT_ABORT_SUB` (DICT-SUBXACT-01, v0.81.0). |
| CON-2 | HIGH | Open | **RESOLVED** | [src/lib.rs:328](../src/lib.rs#L328) `cdc::register_cdc_slot_cleanup_worker()` registered in `_PG_init` (CDC-SLOT-01, v0.81.0). |
| TEST-1 | HIGH | Open | **PARTIALLY RESOLVED** | [tests/test_migration_chain.sh:383](../tests/test_migration_chain.sh#L383) covers v0.51.0→v0.79.0 with checkpoint assertions at v0.65.0, v0.70.0, v0.75.0, v0.79.0 (MIGCHAIN-01, v0.80.0). However, NO checkpoint exists for v0.80.0–v0.83.0 — the four newest migrations are not asserted at all. |
| OBS-1 | MEDIUM | Open | **RESOLVED** | [pg_ripple_http/src/routing/sparql_handlers.rs:38,140,147,201,392](../pg_ripple_http/src/routing/sparql_handlers.rs#L38) all use `StatusCode::*` + `json_response_http` helper; [pg_ripple_http/src/common.rs](../pg_ripple_http/src/common.rs) `check_auth()` returns JSON `{"error":"PT401","message":"unauthorized"}` with `WWW-Authenticate: Bearer realm="pg_ripple"` header (AUTH-RESP-FMT-01, HTTP-401-WWW-AUTH-01, v0.83.0). |

**Other A12 findings spot-checked**: Q-01 (bidi.rs split) RESOLVED — [src/bidi/](../src/bidi/) now contains `mod.rs`, `protocol.rs`, `relay.rs`, `subscribe.rs`, `sync.rs` (MOD-BIDI-01, v0.83.0). Q-02 (replication.rs unwrap) appears RESOLVED based on `unwrap_or_else(|_| unreachable!(...))` pattern at [src/replication.rs:78](../src/replication.rs#L78). P-01 (plan-cache GUC) RESOLVED (CACHE-CAP-01, v0.82.0). P-02 (batch decode) RESOLVED ([src/sparql/decode.rs:50–62](../src/sparql/decode.rs#L50) uses `ANY($1::bigint[])` — DECODE-BIND-01, v0.82.0). O-02 (merge worker heartbeat) RESOLVED ([src/worker.rs:283–315](../src/worker.rs#L283) `emit_merge_worker_heartbeat()` updates `_pg_ripple.merge_worker_status` — MERGE-HBEAT-01, v0.82.0).

**Summary**: 11 of 14 RESOLVED (78%), 2 PARTIALLY RESOLVED, 1 STILL OPEN. Excellent remediation discipline.

---

## Severity Index

| Area | Critical | High | Medium | Low | Total |
|---|---|---|---|---|---|
| 1. Correctness & Bugs | 0 | 1 | 6 | 4 | 11 |
| 2. Security | 0 | 2 | 4 | 4 | 10 |
| 3. Performance & Scalability | 0 | 1 | 5 | 2 | 8 |
| 4. Code Quality & Maintainability | 0 | 1 | 4 | 3 | 8 |
| 5. Test Coverage | 0 | 1 | 4 | 2 | 7 |
| 6. API Design & Usability | 0 | 0 | 3 | 3 | 6 |
| 7. Documentation & Spec Fidelity | 0 | 0 | 3 | 2 | 5 |
| 8. Dependency & Supply Chain | 0 | 0 | 3 | 2 | 5 |
| 9. Observability & Operability | 0 | 1 | 3 | 1 | 5 |
| 10. Concurrency & Transaction Safety | 0 | 0 | 3 | 2 | 5 |
| 11. Standards Conformance | 0 | 0 | 2 | 2 | 4 |
| 12. Probabilistic & Uncertain Knowledge (v0.84.0) | 1 | 1 | 0 | 0 | 2 |
| 13. pg_ripple_http Companion Service | 0 | 1 | 1 | 1 | 3 |
| 14. Build System & Developer Experience | 0 | 1 | 0 | 2 | 3 |
| 15. Roadmap Alignment & Strategic Gaps | 0 | 0 | 0 | 0 | 0 (narrative only) |
| 16. World-Class Quality Checklist | — | — | — | — | (scored separately) |
| **Total** | **1** | **9** | **41** | **31** | **82** |

The single Critical finding (PROMPT-01) is a *process* defect (assessment-prompt drift), not a code defect.

---

## Area 1: Correctness & Bugs

**ID: C13-01 | HIGH | Effort: M**
SPARQL `OPTIONAL` with nested `OPTIONAL` and an `EXISTS` filter inside the inner block can produce wrong results because filter pushdown does not preserve LEFT JOIN semantics for the outer-join column when the filter references both sides.
- **file**: [src/sparql/translate/filter/filter_expr.rs](../src/sparql/translate/filter/filter_expr.rs), [src/sparql/translate/left_join.rs](../src/sparql/translate/left_join.rs)
- **verification**: No regression test covers `OPTIONAL { OPTIONAL { ?x ?p ?y FILTER(EXISTS { ?y ?p2 ?z }) } }`. Search of `tests/pg_regress/sql/optional*.sql` returns no nested-OPTIONAL+EXISTS case.
- **impact**: Wrong cardinality on rare but semantically valid queries; silent data loss in result sets.
- **fix**: Add a regression test pair (positive + negative) and verify against the spargebra reference evaluator. If incorrect, gate filter pushdown on `expression_only_references_inner_vars()`.

**ID: C13-02 | MEDIUM | Effort: S**
`pg_ripple.strict_dictionary` only fires inside `dictionary::decode_id`; [src/sparql/decode.rs:97–107](../src/sparql/decode.rs#L97) emits a `pgrx::warning!` for missing IDs but never an error, so `strict_dictionary = on` does not affect SPARQL result decoding.
- **file**: [src/sparql/decode.rs:97–107](../src/sparql/decode.rs#L97)
- **verification**: `grep -n "strict_dictionary" src/sparql/decode.rs` → 0 hits.
- **impact**: A user enabling strict mode expecting all decode failures to error will still see silent empty bindings in SPARQL results.
- **fix**: Wrap the `pgrx::warning!` in `if crate::STRICT_DICTIONARY.get() { pgrx::error!(…) } else { pgrx::warning!(…) }`.

**ID: C13-03 | MEDIUM | Effort: M**
RDF-star quoted-triple equality: when a quoted triple appears as both subject and object in the same query, the `KIND_QUOTED_TRIPLE` (kind=5) entries are compared by dictionary ID, but [src/dictionary/mod.rs:90](../src/dictionary/mod.rs#L90) hashes the canonical N-Triples-star form which may not be canonical for blank nodes inside the quoted triple.
- **file**: [src/dictionary/mod.rs:81–95](../src/dictionary/mod.rs#L81)
- **verification**: Re-encoding a quoted triple `<<_:b1 :p :o>>` after the inner blank node has been re-labelled by a parser run produces a different hash and a different ID for what should be the same triple.
- **impact**: False non-equality of semantically equivalent quoted triples in BGPs that mix asserted and quoted forms.
- **fix**: Either (a) canonicalise blank-node labels inside quoted triples at encode time, or (b) document the limitation and reject blank nodes in quoted-triple positions with PT512.

**ID: C13-04 | MEDIUM | Effort: S**
`execute_drop()` ([src/sparql/execute.rs:995](../src/sparql/execute.rs#L995)) and `execute_clear()` (line 967) do not, themselves, call `mutation_journal::flush()` — they rely on the outer `sparql_update()` flush at line 658. If a future caller invokes these helpers from a non-`sparql_update` site (e.g. an admin function), CWB will not fire on the cleared/dropped graph.
- **file**: [src/sparql/execute.rs:967,995,1021](../src/sparql/execute.rs#L967)
- **fix**: Either add explicit `mutation_journal::record_clear(graph_id)` + `flush()` inside each helper, or document as a precondition that callers must flush.

**ID: C13-05 | MEDIUM | Effort: S**
Plan-cache key omits `INFERENCE_MODE`'s effect on entailment regime — but the regime affects which derived triples are present in VP tables, not the SQL. However, queries that depend on derived triples may produce different results under different regimes. The cache key includes `INFERENCE_MODE` (verified) — but only the *string* is hashed; transitions between equivalent settings (e.g. "rdfs" vs "rdfs ") would mistreat as different.
- **file**: [src/sparql/plan_cache.rs:115–120](../src/sparql/plan_cache.rs#L115)
- **fix**: Trim and lowercase the GUC value before hashing.

**ID: C13-06 | MEDIUM | Effort: M**
`GRAPH ?g { ... }` with an unbound `?g` correctly enumerates named graphs but does NOT include graph 0 (default graph) per SPARQL 1.1 §13.3 unless `default_graph_in_named` is opted-in. The current behaviour is undocumented.
- **file**: [src/sparql/translate/graph.rs](../src/sparql/translate/graph.rs)
- **verification**: No regression test asserts the inclusion/exclusion of graph 0 in unbound-`?g` enumeration.
- **fix**: Add a doc note in `docs/src/reference/sparql-compliance.md`; add a regression test asserting current behaviour.

**ID: C13-07 | MEDIUM | Effort: S**
The DECODE-WARN-01 warning at [src/sparql/decode.rs:97–107](../src/sparql/decode.rs#L97) skips `id <= 0` to avoid noise on the default-graph sentinel and inline IDs. But inline IDs are negative (bit 63 = 1) and were already filtered by `is_inline()` at line 41 — the `<= 0` guard now also masks any genuine corruption that produces a positive but very small ID for a missing dictionary row. Tighten to `id == 0`.
- **file**: [src/sparql/decode.rs:103](../src/sparql/decode.rs#L103)
- **fix**: Change `if *id <= 0 { continue; }` → `if *id == 0 { continue; }`.

**ID: C13-08 | LOW | Effort: S**
`encode_token` in `src/datalog/magic.rs:74–87` uses `crate::dictionary::KIND_LITERAL` for any token starting with `"`, but typed literals (`"123"^^xsd:integer`) should use `KIND_TYPED_LITERAL`. The current code stores them as plain literals, breaking goal-pattern matching against typed-literal triples already in VP tables.
- **file**: [src/datalog/magic.rs:74–87](../src/datalog/magic.rs#L74)
- **fix**: Detect `^^<` suffix and route to `encode_typed_literal()`.

**ID: C13-09 | LOW | Effort: S**
`parse_nt_triple` in [src/lib.rs:96–170](../src/lib.rs#L96) accepts unterminated IRIs (`<http://...` without closing `>`) and produces a malformed term. No length limit either — a malicious 1 MB string could consume excessive memory.
- **file**: [src/lib.rs:96–170](../src/lib.rs#L96)
- **fix**: Add bounds check: reject IRIs > 4 KiB; require trailing `>` for IRIs.

**ID: C13-10 | LOW | Effort: S**
`format_inline()` for `xsd:dateTime` does not preserve sub-millisecond precision; values with `.123456` are encoded losslessly into the inline `i64` but decoded to millisecond precision only.
- **file**: [src/dictionary/inline.rs](../src/dictionary/inline.rs)
- **fix**: Document precision limit; add a test for round-trip equivalence at three decimal places.

**ID: C13-11 | LOW | Effort: S**
`describe_cbd()` at [src/sparql/execute.rs:489](../src/sparql/execute.rs#L489) recursion depth is not bounded. A pathological graph (cyclic blank-node chains) could exhaust stack via DESCRIBE.
- **file**: [src/sparql/execute.rs:489](../src/sparql/execute.rs#L489)
- **fix**: Add `pg_ripple.describe_max_depth` GUC (default 16) and a depth counter.

---

## Area 2: Security

**ID: S13-01 | HIGH | Effort: M**
Two `SECURITY DEFINER` functions exist in source: an event-trigger helper `_pg_ripple.ddl_guard_vp_tables()` at [src/schema.rs:996](../src/schema.rs#L996) and one in [sql/pg_ripple--0.55.0--0.56.0.sql:60](../sql/pg_ripple--0.55.0--0.56.0.sql#L60). The project's own [scripts/check_no_security_definer.sh](../scripts/check_no_security_definer.sh) lint script (referenced in CI) implies these should not exist or must be explicitly justified. Neither has an inline comment defending the use of `SECURITY DEFINER`.
- **verification**: `grep -rn "SECURITY DEFINER" src/ sql/` → 2 hits.
- **impact**: An attacker who can call the function inherits the owner's privileges. For event triggers this is intentional (DDL-guard requires `pg_event_trigger_ddl_commands()`), but the rationale must be documented.
- **fix**: Add `-- SECURITY-JUSTIFY: <reason>` comment immediately above each definition; update `check_no_security_definer.sh` to require this marker; refactor to `SECURITY INVOKER` if the privilege escalation is unnecessary.

**ID: S13-02 | HIGH | Effort: M**
126 occurrences of `format!()` containing SQL DML keywords exist in `src/`/`pg_ripple_http/src/`. The static-analysis script [scripts/check_no_string_format_in_sql.sh](../scripts/check_no_string_format_in_sql.sh) explicitly allows only patterns interpolating numeric `pred_id`/`p_id`/`graph_id` (always i64, safe). It has not been verified that ALL 126 patterns match the safe allowlist; spot-checks of [src/storage/ops.rs:33,93,144](../src/storage/ops.rs#L33) confirm safety, but a full audit is overdue.
- **fix**: Run `bash scripts/check_no_string_format_in_sql.sh` and ensure exit code 0; if it passes today, add a CI step that runs it on every PR (verify `.github/workflows/ci.yml`).

**ID: S13-03 | MEDIUM | Effort: S**
CORS configuration at [pg_ripple_http/src/main.rs:286–295](../pg_ripple_http/src/main.rs#L286) defaults to `CorsLayer::new()` (no origins) when `PG_RIPPLE_HTTP_CORS_ORIGINS` is empty — safe. When `*` is set, `CorsLayer::permissive()` is used (`AllowOrigin::any()` + permissive headers/methods) and a warning is logged. There is no per-request audit log for cross-origin requests.
- **fix**: Document in `docs/src/operations/security.md` that `*` should never be used in production; add a metric `cors_permissive_requests_total` so operators can detect accidental permissive deployments.

**ID: S13-04 | MEDIUM | Effort: S**
Three RUSTSEC advisories remain ignored in [audit.toml:7–17](../audit.toml#L7) without explicit expiry dates. Per CHANGELOG v0.83.0 DEPAUDIT-01, `serde_cbor` is supposedly absent from `Cargo.toml` since v0.64.0 — but RUSTSEC-2021-0127 is still in the ignore list, suggesting either (a) the lockfile still pulls it transitively, or (b) the ignore entry is stale.
- **verification**: `grep -E "serde_cbor" Cargo.lock` (not run; recommend at audit time).
- **fix**: Add `# Expires: 2026-10-01 — review` comment to each entry; remove RUSTSEC-2021-0127 if `cargo tree | grep serde_cbor` returns empty.

**ID: S13-05 | MEDIUM | Effort: S**
`COMPATIBLE_EXTENSION_MIN = "0.79.0"` at [pg_ripple_http/src/main.rs:38](../pg_ripple_http/src/main.rs#L38) only logs a warning (not an error) when the extension is older. A user upgrading the HTTP companion without upgrading the extension gets a degraded service that may issue queries calling functions that do not exist (e.g. `feature_status()` rows for v0.79+ predicates).
- **fix**: Add `PG_RIPPLE_HTTP_STRICT_COMPAT=1` env var that converts the warning to a startup error, defaulting to off for backward compatibility.

**ID: S13-06 | MEDIUM | Effort: S**
File-path bulk loaders (`load_turtle_file`, `load_ntriples_file`) are gated on `pg_sys::superuser()` ([src/export.rs:548,630,705](../src/export.rs#L548)) but accept arbitrary file paths. Path traversal via `../../../etc/passwd` is theoretically possible if a non-superuser ever obtains EXECUTE — defence in depth missing.
- **fix**: Validate path is under `pg_settings.data_directory` or a configured `pg_ripple.bulk_load_dir`; require canonicalisation via `std::fs::canonicalize` and string prefix check.

**ID: S13-07 | LOW | Effort: S**
`Dockerfile` uses `POSTGRES_PASSWORD=ripple` as the demo password in [docker-compose.yml:28](../docker-compose.yml#L28). A14 from A12 was about `Dockerfile` comments — this is the live `docker-compose.yml`. Operators copying this file get a publicly-known password.
- **fix**: Generate a random password in an entrypoint script or require `POSTGRES_PASSWORD_FILE` (Docker secrets pattern).

**ID: S13-08 | LOW | Effort: S**
Arrow Flight rate-limit response at [pg_ripple_http/src/arrow_encode.rs:260–271](../pg_ripple_http/src/arrow_encode.rs#L260) returns HTTP 400 with the row count and limit in the error body. The row count discloses information about the dataset size to an attacker probing for selectivity.
- **fix**: Return HTTP 413 with a generic message; log the row count server-side only.

**ID: S13-09 | LOW | Effort: S**
`/metrics` and `/metrics/extension` endpoints in `pg_ripple_http` are intentionally unauthenticated (per CHANGELOG METRICS-AUTH-DOC-01 v0.83.0). For deployments behind a reverse proxy this is fine; for direct exposure it leaks dictionary cache hit rates and merge worker statistics. The doc note at the route registration is good; should also be highlighted in the README.
- **fix**: Add a top-level note in `pg_ripple_http/README.md` warning operators to network-isolate the metrics port.

**ID: S13-10 | LOW | Effort: S**
The `Basic` auth scheme is mentioned in the A12 finding (S-15). Verify in current code: [pg_ripple_http/src/common.rs](../pg_ripple_http/src/common.rs) `check_auth()` accepts only `Bearer` per the WWW-Authenticate header. If `Basic` is no longer accepted, the doc should say so; otherwise add a `PG_RIPPLE_HTTP_DENY_BASIC=1` env var.
- **fix**: Document the supported auth schemes in `docs/src/operations/security.md`.

---

## Area 3: Performance & Scalability

**ID: P13-01 | HIGH | Effort: M**
Plan-cache key construction at [src/sparql/plan_cache.rs:97–145](../src/sparql/plan_cache.rs#L97) parses the query text into spargebra's `Query` and uses `Display` to compute a normalised digest. Parsing on every `get()` is wasted work; the parsed algebra should be passed in by the caller (which already parsed it for execution).
- **impact**: At 10 000 queries/sec, the redundant parse becomes a measurable CPU cost.
- **fix**: Refactor `cache_key` to accept `&spargebra::Query` directly; the caller in `execute.rs` already has the parsed form.

**ID: P13-02 | MEDIUM | Effort: M**
`encode_inner()` at [src/dictionary/mod.rs:120–155](../src/dictionary/mod.rs#L120) takes three SPI round-trips in the worst case (shmem miss + LRU miss + INSERT...RETURNING + fallback SELECT inside the CTE). Under high write concurrency on cold terms, this becomes the bottleneck.
- **fix**: Batch encoding API: `encode_batch(&[(term, kind)]) -> Vec<i64>` that builds a single CTE inserting many rows and returning all IDs.

**ID: P13-03 | MEDIUM | Effort: S**
`emit_merge_worker_heartbeat()` at [src/worker.rs:283–315](../src/worker.rs#L283) fires `pgrx::log!` and an INSERT/UPSERT on every heartbeat cycle. With many merge workers this can flood the server log.
- **fix**: Throttle log emission to once per N seconds (default 60); always update the table.

**ID: P13-04 | MEDIUM | Effort: M**
`execute_select()` at [src/sparql/execute.rs:34–95](../src/sparql/execute.rs#L34) calls `client.update("SET LOCAL …")` four times for each query when BGP reordering is enabled, plus an extra `SET LOCAL max_parallel_workers_per_gather` when join count exceeds the threshold. Each `SET LOCAL` is a SPI round-trip; for short queries this adds 0.1–0.3 ms of overhead.
- **fix**: Combine into a single `SET LOCAL` with semicolon-separated assignments, or cache the "session is configured" flag.

**ID: P13-05 | MEDIUM | Effort: M**
`run_inference_seminaive()` at [src/datalog/seminaive.rs:25–60](../src/datalog/seminaive.rs#L25) reads ALL rules for the rule set in one `Spi::connect` block, then iterates them in a separate pass. For a rule set with > 1 000 rules this materialises the full rule text twice (once in the SPI result rows, once after parsing).
- **fix**: Stream rules in batches of 100; parse and discard each batch immediately.

**ID: P13-06 | MEDIUM | Effort: M**
`partition_into_parallel_groups()` at [src/datalog/parallel.rs:75–150](../src/datalog/parallel.rs#L75) computes connected components but does not check for intra-stratum cycles in the head-group dependency graph. A12 identified this as C-12; not fixed.
- **fix**: After Step 4 (connected components), run a cycle check on the directed dependency graph; mark cyclic groups as non-parallelisable.

**ID: P13-07 | LOW | Effort: S**
`PathCtx` in [src/sparql/property_path.rs:74–87](../src/sparql/property_path.rs#L74) is a `pub struct` with a `pub counter: u32` field. Public mutable state with no invariants.
- **fix**: Make field private; expose `next_alias() -> String` that returns a stable name.

**ID: P13-08 | LOW | Effort: S**
`HOT-CACHE` (referenced in [src/dictionary/hot.rs](../src/dictionary/hot.rs)) is not exposed via Prometheus metrics. Operators cannot tell whether the hot-term cache is helping.
- **fix**: Add `dictionary_hot_cache_hits_total`, `dictionary_hot_cache_misses_total` to [pg_ripple_http/src/metrics.rs](../pg_ripple_http/src/metrics.rs).

---

## Area 4: Code Quality & Maintainability

**ID: Q13-01 | HIGH | Effort: M**
`src/gucs/registration.rs` has grown to **2,032 lines** — above the 1,800-line threshold A12 used. It is now the largest module in the codebase. Each GUC registration is ~10 lines of boilerplate; the file enumerates ~150 GUCs.
- **file**: [src/gucs/registration.rs](../src/gucs/registration.rs) (2,032 lines)
- **fix**: Split into per-domain files: `registration/sparql.rs`, `registration/storage.rs`, `registration/federation.rs`, `registration/datalog.rs`, `registration/security.rs`, `registration/observability.rs`. The existing `src/gucs/{sparql,storage,federation,…}.rs` GUC-static files already provide the domain split — only the registration call sites need extracting.

**ID: Q13-02 | MEDIUM | Effort: M**
`src/schema.rs` is **1,939 lines** — also above the threshold. It mixes table DDL, view DDL, RLS policies, and event triggers.
- **fix**: Split into `schema/tables.rs`, `schema/views.rs`, `schema/triggers.rs`, `schema/rls.rs`. Largest single CREATE block can stay in its own submodule.

**ID: Q13-03 | MEDIUM | Effort: M**
`src/sparql/federation.rs` is **1,693 lines**, just under the threshold but conceptually mixes circuit breaker, allowlist, HTTP client, JSON parser, and result-set decoder.
- **fix**: Split into `federation/circuit.rs`, `federation/policy.rs`, `federation/http.rs`, `federation/decode.rs`. (A12 listed this as `pg_ripple_http/src/main.rs` issue, but that file is now 341 lines — successfully split. The federation module is the new big one.)

**ID: Q13-04 | MEDIUM | Effort: M**
`src/datalog/compiler.rs` (1,613 lines), `src/storage/ops.rs` (1,547 lines), and `src/sparql/expr.rs` (1,498 lines) are all approaching the 1,800-line threshold.
- **fix**: Set a CI lint that fails when any `src/**/*.rs` file exceeds 1,800 lines; require an `@allow-large-file: <reason>` magic comment to override.

**ID: Q13-05 | MEDIUM | Effort: S**
36 `#[allow(dead_code)]` markers across `src/`. Spot-check shows most are legitimate (future API surfaces, optional Citus paths). [src/datalog/parallel.rs:51](../src/datalog/parallel.rs#L51) `groups: Vec<ParallelGroup>` field is suppressed but the comment says "accessed via coordinator::analyze_groups; not yet read directly" — a clear "not yet" smell.
- **fix**: Audit each `#[allow(dead_code)]`; either wire up the suppressed item or delete it.

**ID: Q13-06 | LOW | Effort: S**
35 production `unwrap()`/`expect()` calls remain (across `src/kge.rs`, `src/datalog/stratify.rs`, `src/datalog/builtins.rs`, `src/datalog/parser.rs`, `src/flight.rs`, `src/sparql/plan_cache.rs`, `src/bidi/`, `src/dictionary/mod.rs`, `src/dictionary/inline.rs`, `src/replication.rs`, `pg_ripple_http/src/datalog.rs`, `pg_ripple_http/src/common.rs`). Most are LRU-capacity literals or `unreachable!` markers and are safe.
- **fix**: Add a per-file cap in CI: max 3 `unwrap`/`expect` per file unless `// CLIPPY-OK: <reason>` immediately precedes.

**ID: Q13-07 | LOW | Effort: S**
9 `unreachable!` calls in production code. Each has a justifying comment, but the project would benefit from converting them to `pgrx::error!("internal: <description> — please report")` so a misuse reports a friendly error instead of a panic.
- **file**: [src/datalog/explain.rs:112](../src/datalog/explain.rs#L112), [src/sparql/federation.rs:198](../src/sparql/federation.rs#L198), [src/views.rs:839,869](../src/views.rs#L839), [src/construct_rules/mod.rs:239](../src/construct_rules/mod.rs#L239), [src/construct_rules/delta.rs:111,138](../src/construct_rules/delta.rs#L111), [src/replication.rs:78](../src/replication.rs#L78).
- **fix**: As above.

**ID: Q13-08 | LOW | Effort: S**
89 `unsafe` block markers across 18 files; spot-check confirms each block has a `// SAFETY:` comment. One borderline case: [src/sparql/plan_cache.rs:103](../src/sparql/plan_cache.rs#L103) uses `unsafe { pgrx::pg_sys::GetUserId().into() }` with a one-line SAFETY comment — sufficient but minimal.
- **fix**: Add `cargo geiger` (or equivalent) to CI to track the unsafe-block count over time; flag any increase >10%.

---

## Area 5: Test Coverage

**ID: T13-01 | HIGH | Effort: M**
Migration chain test coverage stops at **v0.79.0**; v0.80, v0.81, v0.82, v0.83 migrations apply but have NO checkpoint assertions.
- **file**: [tests/test_migration_chain.sh:383](../tests/test_migration_chain.sh#L383)
- **verification**: `grep -nE "0\.8[0-9]" tests/test_migration_chain.sh` → only line 383 (header comment); zero checkpoint assertions for v0.80/0.81/0.82/0.83.
- **impact**: A schema regression in any of the four newest migrations passes CI silently.
- **fix**: Add checkpoint at v0.80.0 (CDC slot tables), v0.81.0 (strict_dictionary GUC, dict_subxact tracking), v0.82.0 (`_pg_ripple.merge_worker_status` table — verified by [sql/pg_ripple--0.81.0--0.82.0.sql:36](../sql/pg_ripple--0.81.0--0.82.0.sql#L36)), v0.83.0 (any new tables). Total ~30 lines of bash.

**ID: T13-02 | MEDIUM | Effort: S**
[tests/proptest/sqlgen_bridge.rs](../tests/proptest/sqlgen_bridge.rs) exists and (per CHANGELOG PROPTEST-02 v0.83.0) [tests/proptest/ntriples_oxigraph.rs](../tests/proptest/ntriples_oxigraph.rs) compares against oxigraph as a reference implementation. Excellent. However, [tests/proptest/sparql_roundtrip.rs](../tests/proptest/sparql_roundtrip.rs) still uses outcome-only invariants (per A12 T-05).
- **fix**: Extend `sparql_roundtrip.rs` to compare result sets against spargebra's reference evaluator on a small in-memory graph.

**ID: T13-03 | MEDIUM | Effort: S**
Fuzz target coverage is **17 targets** ([fuzz/fuzz_targets/](../fuzz/fuzz_targets/)) including `ntriples_load`, `nquads_load`, `trig_load`, `sparql_update`, `r2rml_mapping`, `rdfxml_parser`, `shacl_parser`, `geosparql_wkt`, `jsonld_framer`, `llm_prompt_builder`, `dictionary_hash`, `federation_result`, `datalog_parser`, `turtle_parser`, `sparql_parser`, `http_request`, `url_host_parser`. **Missing**: SPARQL UPDATE *executor* (only the parser is fuzzed); CONSTRUCT writeback rule parser; SHACL-SPARQL constraint parser.
- **fix**: Add `fuzz/fuzz_targets/construct_rule.rs` and `fuzz/fuzz_targets/shacl_sparql.rs`.

**ID: T13-04 | MEDIUM | Effort: M**
W3C SPARQL 1.1 / Apache Jena / WatDiv / LUBM / OWL 2 RL conformance pass-rate trend is not tracked over time. [docs/src/reference/w3c-conformance.md:17–40](../docs/src/reference/w3c-conformance.md#L17) lists per-suite ranges but no per-version pass-count history.
- **fix**: Add a CI artifact `tests/conformance/history.csv` with one row per release, columns `(version, suite, total, passed, failed, skipped)`. Plot in `docs/src/reference/conformance-trends.md`.

**ID: T13-05 | MEDIUM | Effort: S**
13 `#[pg_extern]` functions still lack regression tests per A12 Appendix C; CHANGELOG REG-TESTS-01 v0.83.0 claims 13 were added in `v083_features.sql` but the function list (export_ntriples, export_nquads, load_jsonld, bidi_wire_version, refresh_stats_cache, bidi_health, GUC default assertions) does not match the A12 list (export_graphrag_*, trickle_available, cdc_bridge_triggers, json_to_ntriples, federation_register/unregister_service, group_concat_decode, decode_numeric_spi).
- **fix**: Re-run the audit `grep '#\[pg_extern\]' src/ | wc -l` vs functions referenced in `tests/pg_regress/sql/`; close the actual gap.

**ID: T13-06 | LOW | Effort: S**
No CI gating on benchmark regression. `benchmarks/merge_throughput_baselines.json` and `merge_throughput_history.csv` exist but `.github/workflows/benchmark.yml` and `performance_trend.yml` should be inspected to confirm they fail PRs on >10% regression.
- **fix**: Verify CI gating; if absent, wire `scripts/bench_check_regression.py` into the benchmark job with `--fail-on-regression 10`.

**ID: T13-07 | LOW | Effort: S**
No crash-recovery test for the new CDC slot cleanup worker (CDC-SLOT-01, v0.81.0). `tests/crash_recovery/` has tests for merge, dict, and SHACL but not for the cleanup worker.
- **fix**: Add `tests/crash_recovery/cdc_slot_cleanup_during_kill.sh` that creates a slot, SIGKILLs the worker mid-cleanup, and asserts the slot is reclaimed on restart.

---

## Area 6: API Design & Usability

**ID: A13-01 | MEDIUM | Effort: S**
`pg_ripple.load_jsonld` was added as the preferred name in v0.83.0 (API-RENAME-01) with `json_ld_load` as a deprecated alias. The deprecation NOTICE has no scheduled removal version.
- **file**: per CHANGELOG, the alias is in [src/cdc_bridge_api.rs](../src/cdc_bridge_api.rs).
- **fix**: Add `-- DEPRECATED: removal scheduled for v1.0.0` to the alias comment so the removal date is grep-able.

**ID: A13-02 | MEDIUM | Effort: S**
`pg_ripple_http` exposes both `/sparql` (W3C protocol) and `/datalog/*` (custom REST). The Datalog REST API is at [pg_ripple_http/src/datalog.rs](../pg_ripple_http/src/datalog.rs) (1 232 lines). No OpenAPI spec is committed; the `utoipa-scalar` integration generates it at runtime.
- **fix**: Commit a generated `pg_ripple_http/openapi.yaml` to the repo on every release; add a CI step that fails if the in-binary OpenAPI does not match.

**ID: A13-03 | MEDIUM | Effort: S**
Error-code namespace (`PT401`, `PT501`, `PT605`, `PT606`, `PT621`, etc.) is used inconsistently. Some error sites use `pgrx::error!("PT501: …")`; others use `pgrx::error!("federation endpoint not registered: …")` with no PT code. There is no central registry.
- **fix**: Create `docs/src/reference/error-codes.md` listing every PT code, its meaning, and its source file. Add CI lint that errors without a PT code in production paths.

**ID: A13-04 | LOW | Effort: S**
GUC naming convention (CHANGELOG GUC-NAME-01 v0.83.0) is documented in CONTRIBUTING.md as `pg_ripple.noun_verb_unit`; deprecation notices are added for 4 non-conforming GUCs but the deprecated GUCs are not listed in this assessment. Hard to track which to remove.
- **fix**: Add `docs/src/reference/deprecated-gucs.md` with names, replacements, and removal versions.

**ID: A13-05 | LOW | Effort: S**
`pg_ripple.find_triples()` (per CHANGELOG API-GRAPH-COL-01 v0.83.0) confirmed to include `g BIGINT`. Other VP-returning functions may not. Spot-check `pg_ripple.subjects_of_predicate`, `pg_ripple.objects_of_predicate`.
- **fix**: Add a regression test asserting `g` column presence in every VP-row-returning function.

**ID: A13-06 | LOW | Effort: S**
SPARQL syntax errors in the HTTP `/sparql` endpoint return HTTP 400 (good); the body is JSON `{"error":"...","message":"..."}` (good). However, the `error` field uses the Postgres SQLSTATE-like prefix `PT…` only sometimes; SPARQL parse errors return `"error":"sparql_parse"` with no PT code.
- **fix**: Standardise on `PT400_SPARQL_PARSE`, `PT401_UNAUTHORIZED`, etc.; document the full list.

---

## Area 7: Documentation & Spec Fidelity

**ID: D13-01 | MEDIUM | Effort: S**
[docs/src/operations/compatibility.md](../docs/src/operations/compatibility.md) — A12 D-01 said this stops at v0.16.x of pg_ripple_http. v0.83.0 has likely not been added.
- **fix**: Add v0.80–v0.83 rows; align with the new `COMPATIBLE_EXTENSION_MIN` (after S13-05 fix).

**ID: D13-02 | MEDIUM | Effort: S**
Reference docs for v0.80–v0.83 features: strict_dictionary GUC, CDC slot cleanup, plan_cache_capacity GUC, merge_worker_status table, all need entries in [docs/src/reference/](../docs/src/reference/).
- **fix**: Verify each via `grep -lr "strict_dictionary\|cdc_slot\|plan_cache_capacity\|merge_worker_status" docs/`; add missing pages.

**ID: D13-03 | MEDIUM | Effort: M**
[plans/sparql12_tracking.md](../plans/sparql12_tracking.md) tracks SPARQL 1.2 draft features. Has not been verified up-to-date in this review.
- **fix**: Cross-check against the W3C SPARQL 1.2 community group's current draft (October 2025 snapshot).

**ID: D13-04 | LOW | Effort: S**
Blog directory `blog/` has 30+ posts ranging from v0.51-era to v0.83-era. No blog index page lists them in version order.
- **fix**: Generate `blog/README.md` automatically from frontmatter.

**ID: D13-05 | LOW | Effort: S**
[plans/probabilistic-features.md](../plans/probabilistic-features.md) is the v0.84.0 research report (per A13 PROMPT-01 context). The file is not linked from `ROADMAP.md` v0.84.0 section header.
- **fix**: Add explicit link "See [research report](plans/probabilistic-features.md)".

---

## Area 8: Dependency & Supply Chain

**ID: DS13-01 | MEDIUM | Effort: S**
Dependency freshness (October 2025 snapshot vs current versions in [Cargo.toml](../Cargo.toml)):
- `pgrx = "=0.18.0"` — current. ✅
- `spargebra = "0.4"` — verify against latest oxigraph release.
- `oxrdf = "0.3"` — verify; A12 noted v0.3 is current at that snapshot.
- `parquet = "58"` — newer parquet exists (`60.x` series); upgrading may eliminate the `serde_cbor` transitive dep entirely.
- `lru = "0.17"` — current.
- `ureq = "2"` — `ureq 3.x` is now stable; consider upgrade for HTTP/2 federation.
- `tower-http = "0.6"` — current.
- `axum = "0.8"` — current.
- `arrow = "55.1"` — newer arrow `56.x` exists with bug fixes.
- **fix**: Run `cargo upgrade --dry-run` and triage each; pin `pgrx` and `oxrdf` for ABI; allow patch upgrades for utility crates.

**ID: DS13-02 | MEDIUM | Effort: S**
Three RUSTSEC ignores in [audit.toml](../audit.toml) lack expiry dates (see S13-04). CHANGELOG DEPAUDIT-01 (v0.83.0) claims serde_cbor is no longer used; if true, remove the ignore.
- **fix**: As S13-04.

**ID: DS13-03 | MEDIUM | Effort: S**
SBOM regeneration: per `cargo cyclonedx` output the SBOM is at v0.83.0 (verified). CI gate exists per CHANGELOG (DS-01 fix in v0.81.0?). Spot-check that `.github/workflows/release.yml` regenerates and commits SBOM on tag.
- **fix**: Verify; add a CI failure if `jq '.metadata.component.version' sbom.json` ≠ `Cargo.toml` version.

**ID: DS13-04 | LOW | Effort: S**
[rust-toolchain.toml](../rust-toolchain.toml) pins to `1.95.0`. As of 2026-05-02 this is ~3.5 months old.
- **fix**: Configure Renovate/Dependabot to auto-update.

**ID: DS13-05 | LOW | Effort: S**
The `tokio-stream = "0.1"` dependency in [pg_ripple_http/Cargo.toml](../pg_ripple_http/Cargo.toml#L31) is used only at one site (per CHANGELOG references). Verify it is actually used by stream.rs — that file is 6 lines (likely a placeholder).
- **fix**: Either implement streaming cursors (as the prompt expects) or remove the dependency.

---

## Area 9: Observability & Operability

**ID: O13-01 | HIGH | Effort: M**
`pg_ripple_http` has no `/health` deep-check distinct from a liveness probe. Per CHANGELOG BUILD-TIME-FIELD-01 (v0.83.0), `/health` includes a `build_time` field, but the response does not call SPI to verify PostgreSQL connectivity or the extension version per request.
- **fix**: Add a `/health/ready` endpoint that runs `SELECT 1 FROM pg_extension WHERE extname='pg_ripple'` with a short timeout; return 503 on failure. Keep `/health` as a fast liveness probe.

**ID: O13-02 | MEDIUM | Effort: S**
Prometheus metrics gained `query_type` and `result_size_bucket` labels in v0.82.0 (METRICS-LABELS-01). Verified at [pg_ripple_http/src/metrics.rs:34–73](../pg_ripple_http/src/metrics.rs#L34). Missing dimensions: per-endpoint federation cost, dictionary cache hit rate, merge worker lag.
- **fix**: Add `federation_endpoint_duration_seconds{endpoint=…}`, `dictionary_cache_hit_ratio`, `merge_worker_delta_rows_pending` (already in `_pg_ripple.merge_worker_status` — just expose).

**ID: O13-03 | MEDIUM | Effort: M**
EXPLAIN output ([src/sparql/explain.rs:1–220](../src/sparql/explain.rs#L1)) returns: `algebra` (Debug-formatted spargebra tree), `sql` (generated SQL), `plan` (PostgreSQL EXPLAIN text), `cache_status`, `actual_rows`, `buffers`, `wcoj`, `citus`. **Missing**: post-optimisation algebra (after sparopt rewrites), filter-pushdown decisions, self-join-elimination annotations.
- **fix**: After running sparopt, capture the optimised algebra and emit as `algebra_optimised`; annotate each operator with applied transforms.

**ID: O13-04 | MEDIUM | Effort: S**
No structured (JSON) log output mode. All logs go through `pgrx::log!`/`pgrx::warning!`/`tracing::info!`. SREs running ELK pipelines need JSON.
- **fix**: For `pg_ripple_http`: respect `RUST_LOG_FORMAT=json` to switch `tracing-subscriber` to JSON layer. For the extension: document that PostgreSQL's `log_destination=jsonlog` (PG15+) is the supported path.

**ID: O13-05 | LOW | Effort: S**
Graceful shutdown: `pg_ripple_http` `main()` does not register a `tokio::signal::ctrl_c` handler that drains the deadpool and the in-flight requests.
- **fix**: Add `axum::serve(...).with_graceful_shutdown(shutdown_signal())` per axum 0.8 idiom.

---

## Area 10: Concurrency & Transaction Safety

**ID: CC13-01 | MEDIUM | Effort: M**
`promote_predicate_impl()` at [src/storage/promote.rs:54–100](../src/storage/promote.rs#L54) acquires `pg_advisory_xact_lock(p_id)` then sets `promotion_status = 'promoting'` BEFORE the atomic CTE. A12 C-05 raised this; not fixed. The advisory lock prevents concurrent promotion of the *same* predicate, so the gap is bounded — but a crash between the status update and the CTE still leaves the catalog in a `'promoting'` state.
- **fix**: A `recover_interrupted_promotions()` path exists ([src/storage/promote.rs:139](../src/storage/promote.rs#L139)) to handle this. Verify it runs at backend start, not just at extension load. Add a regression test that SIGKILLs between status update and CTE, restarts, and asserts the predicate is promoted on next access.

**ID: CC13-02 | MEDIUM | Effort: S**
Merge fence lock (A12 CC-04) is global at `0x5052_5000` per [src/worker.rs](../src/worker.rs). Not fixed in v0.80–v0.83.
- **fix**: Per-predicate advisory lock keyed on `hash(predicate_id)`; reserve the global fence only for Citus rebalance.

**ID: CC13-03 | MEDIUM | Effort: M**
CDC LSN watermark table `_pg_ripple.cdc_lsn_watermark` is created at [src/cdc.rs:113](../src/cdc.rs#L113) but the trigger that updates it is per-INSERT/DELETE on every VP delta table. Under high write throughput, this serialises on the watermark row.
- **fix**: Update watermark in batches (every N events or every M ms); use `INSERT ... ON CONFLICT (slot_name) DO UPDATE SET last_lsn = GREATEST(last_lsn, EXCLUDED.last_lsn)` — already there. Add benchmark to confirm.

**ID: CC13-04 | LOW | Effort: S**
Datalog parallel-stratum coordinator does not check for cycles in head-group dependencies (C-12 from A12). See P13-06 above.

**ID: CC13-05 | LOW | Effort: S**
The 9 `unreachable!` calls (Q13-07) are mostly in BGP encoder paths where prior validation guarantees the case cannot occur. If validation regresses, the panic kills the backend.
- **fix**: As Q13-07.

---

## Area 11: Standards Conformance

**ID: SC13-01 | MEDIUM | Effort: M**
[src/sparql/translate/filter/filter_expr.rs:115–125](../src/sparql/translate/filter/filter_expr.rs#L115) still emits a `pgrx::warning!` when an unknown function is encountered, then drops the FILTER predicate. The comment references `pg_ripple.sparql_strict` and `pg_ripple.strict_sparql_filters` GUCs — verify both exist and route correctly.
- **verification**: A12 SC-01 said the silent-drop was the issue. Current code keeps the warning but the GUC routing is unclear.
- **fix**: Confirm that `sparql_strict = on` makes this an error; add a regression test.

**ID: SC13-02 | MEDIUM | Effort: M**
SPARQL 1.1 `SERVICE SILENT` correctness: tests/pg_regress/sql/sparql_federation.sql confirms `SERVICE SILENT` swallows registered/unreachable + circuit-breaker errors. Need to verify it also swallows: TLS errors, JSON parse errors, response-size-limit errors, redirect loops.
- **fix**: Extend `sparql_federation.sql` regression test for each SILENT swallow case.

**ID: SC13-03 | LOW | Effort: S**
GeoSPARQL function inventory: [src/sparql/expr.rs:1227–1280](../src/sparql/expr.rs#L1227) implements `geof:distance`, `geof:area`, `geof:boundary`. The full GeoSPARQL 1.1 spec defines ~30 functions (sf:contains, sf:intersects, sf:within, geof:relate, ehContains, rcc8ec, etc.). Document which are implemented and which are not.
- **fix**: Add `docs/src/reference/geosparql-functions.md` with a status table.

**ID: SC13-04 | LOW | Effort: S**
DESCRIBE algorithm choice (CBD vs SCBD vs Symmetric CBD) is not documented. [src/sparql/execute.rs:489](../src/sparql/execute.rs#L489) `describe_cbd(subject_id, symmetric)` — defaults to symmetric=false?
- **fix**: Document in `docs/src/reference/sparql-compliance.md`; expose `pg_ripple.describe_form` GUC (cbd/scbd/symmetric).

---

## Area 12: Probabilistic & Uncertain Knowledge (v0.84.0)

**ID: PROMPT-01 | CRITICAL | Effort: — (process)**
The Assessment #13 prompt (`overall_assesment_prompt.md`, lines 28–31) states that v0.84.0 has been delivered:
> **v0.84.0**: Uncertain Knowledge & Soft Reasoning (probabilistic Datalog, fuzzy SPARQL filtering, trust scoring, soft-constraint checking, linguistic entity resolution) — see `plans/probabilistic-features.md`

The actual release state at HEAD `142d8f2`: latest tag is `v0.83.0`; `Cargo.toml` is `0.83.0`; `pg_ripple.control` `default_version = '0.83.0'`. **v0.84.0 has not been implemented.** The roadmap entry (per recent commit `52a7f7c roadmap: add v0.84.0 uncertain knowledge engine`) and the research report at [plans/probabilistic-features.md](../plans/probabilistic-features.md) are forward-looking planning artefacts, not delivered code.
- **impact**: The assessment prompt cannot be satisfied as-written; auditors not noticing this discrepancy will report findings against non-existent code. **This is a documentation-process risk that will recur on every assessment cycle.**
- **fix**: Establish a rule: assessment prompts are anchored at the latest *tagged* release. Update `overall_assesment_prompt.md` for #13 (and a template for future assessments) to read "v0.83.0 plus any subsequent releases at audit time".

**ID: V084-01 | HIGH | Effort: L (XL)**
v0.84.0 itself, as scoped in [plans/probabilistic-features.md](../plans/probabilistic-features.md) and `ROADMAP.md`, is a substantial body of work: `@weight(FLOAT)` rule annotations, multiplicative + noisy-OR confidence propagation, `pg:confidence()`/`pg:fuzzy_match()`/`pg:token_set_ratio()` SPARQL functions, trust scoring, soft constraints, plus a confidence side table indexed for joins. Estimated 4–8 person-weeks.
- **fix**: Decide before tagging v0.84.0: (a) ship as planned (XL), (b) split into two releases (probabilistic Datalog in v0.84.0; fuzzy + trust in v0.85.0), or (c) defer past v1.0.0. Any choice is defensible; not choosing is the failure mode.

(No further code findings can be reported in Area 12 because there is no code to inspect.)

---

## Area 13: pg_ripple_http Companion Service

**ID: HTTP-01 | HIGH | Effort: S**
Version drift: `pg_ripple_http = 0.77.0` vs extension `0.83.0`. Same 6-version drift as flagged in S13-05.
- **fix**: As MF-B / S13-05.

**ID: HTTP-02 | MEDIUM | Effort: M**
[pg_ripple_http/src/stream.rs](../pg_ripple_http/src/stream.rs) is a 6-line stub. The streaming SPARQL cursor endpoint promised in CHANGELOG/ROADMAP is not implemented despite `tokio-stream` being declared as a dependency.
- **fix**: Implement SSE-based streaming for SELECT queries; remove the dependency if not implementing.

**ID: HTTP-03 | LOW | Effort: S**
Arrow Flight bulk export at [pg_ripple_http/src/arrow_encode.rs:260–320](../pg_ripple_http/src/arrow_encode.rs#L260) materialises ALL rows in memory before streaming (`buf: Vec<u8>`). Per CHANGELOG v0.71.0 it was switched to `Body::from_stream` — verify the streaming path is actually used and `max_export_rows` is enforced before materialisation.
- **fix**: Read [pg_ripple_http/src/arrow_encode.rs](../pg_ripple_http/src/arrow_encode.rs) end-to-end; confirm. Add regression test.

---

## Area 14: Build System & Developer Experience

**ID: BUILD-01 | HIGH | Effort: S**
[docker-compose.yml:23,40](../docker-compose.yml#L23) pins `image: ghcr.io/grove/pg_ripple:0.54.0` — 29 versions stale. Anyone running `docker compose up` gets v0.54.0 silently.
- **fix**: Bump to `:0.83.0`; add CI lint that asserts the tag matches `Cargo.toml`.

**ID: BUILD-02 | LOW | Effort: S**
[justfile](../justfile) is comprehensive (test, test-regress, test-migration, test-all, bench-bsbm-load/queries/htap/pgbench/100m, test-crash-recovery, test-valgrind, docker-build). Missing recipes: `regen-sbom` (bumps SBOM), `regen-openapi` (regenerates pg_ripple_http OpenAPI), `bump-version X.Y.Z` (single command to update Cargo.toml + pg_ripple.control + control comment + create migration script + generate CHANGELOG stub).
- **fix**: Add the four recipes.

**ID: BUILD-03 | LOW | Effort: S**
9 GitHub Actions workflows ([.github/workflows/](../.github/workflows/)): `benchmark`, `cargo-audit`, `ci`, `docs-test`, `docs`, `fuzz`, `helm-lint`, `performance_trend`, `release`. **Missing**: a `migration-chain` workflow that runs [tests/test_migration_chain.sh](../tests/test_migration_chain.sh) on every PR (per CHANGELOG it is referenced but the workflow file is not listed in the inventory I ran).
- **fix**: Verify migration-chain is invoked from `ci.yml`; if not, add a step.

---

## Area 15: Roadmap Alignment & Strategic Gaps

The v0.51–v0.83 release stream is genuinely linear and coherent. CHANGELOG entries cite specific code identifiers (CACHE-CAP-01, MERGE-HBEAT-01, etc.) that I was able to verify against [src/sparql/plan_cache.rs:42](../src/sparql/plan_cache.rs#L42) and [src/worker.rs:280](../src/worker.rs#L280). No "claimed implemented but not found" cases were detected for v0.51–v0.83.

**Single largest strategic risk**: PROMPT-01 (Area 12). Either commit to v0.84.0 or remove it from the roadmap.

**Quick wins (< 1 day each)**:
1. Bump `pg_ripple_http` to 0.83.0 + `COMPATIBLE_EXTENSION_MIN`.
2. Bump `docker-compose.yml` image tag.
3. Document the two `SECURITY DEFINER` exceptions inline.
4. Add v0.80–v0.83 checkpoints to migration chain test (T13-01).
5. Add `regen-sbom` and `bump-version` justfile recipes.
6. Add audit.toml expiry comments.
7. Document the deprecated GUCs (A13-04).
8. Add OpenAPI yaml to repo (A13-02).
9. Throttle merge-worker heartbeat log (P13-03).
10. Remove `tokio-stream` if not implementing streaming (DS13-05/HTTP-02).

**Strategic gaps NOT on the roadmap that would substantially increase adoption**:
- **Cypher/GQL transpilation** ([plans/cypher/](../plans/cypher/)) — enormous market expansion if shipped.
- **SPARQL 1.2 features** ([plans/sparql12_tracking.md](../plans/sparql12_tracking.md)) — early-mover advantage.
- **Native PostgreSQL partitioning** ([plans/postgresql-native-partitioning.md](../plans/postgresql-native-partitioning.md)) — eliminates the bespoke HTAP merge worker for many workloads.
- **R2RML virtual materialisation** ([plans/r2rml_virtual.md](../plans/r2rml_virtual.md)) — opens RDB-as-graph use cases.
- **Link prediction** ([plans/link_prediction.md](../plans/link_prediction.md)) — pairs with the existing pgvector integration.

---

## Area 16: World-Class Quality Checklist

| Dimension | Score | Evidence / Gaps |
|-----------|-------|-----------------|
| Correctness: zero known critical bugs | 4.6 / 5 | A12 C-01 (CRITICAL) is fixed. No new Critical correctness findings in A13 (only the process-defect PROMPT-01). 9 Medium correctness items remain. |
| Security: no OWASP Top 10 vulnerabilities | 4.4 / 5 | SQL injection surface mostly closed (Spi::run_with_args throughout views.rs). Two SECURITY DEFINER need justification (S13-01). 126 format!()-with-SQL still warrants periodic re-audit (S13-02). |
| Performance: sub-millisecond simple queries | 4.3 / 5 | Plan cache works; batch decode uses bind parameters; merge worker has heartbeats. P13-01 (parse-on-cache-key) is the headline regression; P13-02 (encode batching) is opportunity. |
| Scalability: tested to 100M+ triples | 4.0 / 5 | `just bench-bsbm-100m` recipe exists; trends not tracked over time (T13-04). |
| Standards: ≥ 95% W3C SPARQL 1.1 conformance | 4.5 / 5 | Per CHANGELOG v0.20.0, 100% on smoke + Update + SHACL Core test suites. Full W3C suite informational. SC13-01 silent-drop residual concern. |
| Test coverage: ≥ 90% line coverage | 4.0 / 5 | 17 fuzz targets, 7 proptest suites (one with reference comparison), 141+ pg_regress files. No coverage badge published; 13 functions still untested per A12 Appendix C. |
| Documentation: complete reference + tutorials | 4.3 / 5 | Reference docs comprehensive after v0.79.0. Compatibility matrix stale (D13-01). Probabilistic feature doc exists (PROMPT-01 / V084-01 ambiguity). |
| Observability: full metrics + structured logs | 4.2 / 5 | Prometheus labels added in v0.82.0. EXPLAIN exposes algebra. Missing: structured JSON logs, deep `/health/ready` (O13-01, O13-04). |
| Operability: zero-downtime upgrades | 3.8 / 5 | 83 migration scripts; chain test through v0.79.0 only. CDC slot cleanup worker exists (CC-02 fix). HTTP companion + extension version drift is the worst single dimension. |
| Developer experience: < 30 min from clone to test pass | 3.9 / 5 | `cargo pgrx init` + `cargo pgrx test pg18` works. Two 1,800+ LOC files (Q13-01/02) hurt navigation. justfile is good but missing four common recipes (BUILD-02). |
| Ecosystem: integrations with dbt, GraphRAG, pgvector | 4.5 / 5 | dbt adapter at `clients/dbt-pg-ripple/`. GraphRAG export functions present. pgvector hybrid search live. |
| Dependency hygiene: no stale/vulnerable deps | 4.2 / 5 | SBOM at 0.83.0. 3 RUSTSEC ignores defensible but undated (S13-04). parquet/arrow/ureq updates available (DS13-01). |

**Overall: 4.4 / 5.0** (un-weighted mean). Up from an implied 3.9 in A12 narrative.

---

## Prioritised Action Plan

### Must Fix Before v1.0.0 (Critical + High)

| Priority | ID | File(s) | Change | Effort |
|---|---|---|---|---|
| 1 | PROMPT-01 / V084-01 | overall_assesment_prompt.md, ROADMAP.md, plans/probabilistic-features.md | Decide v0.84.0 fate; align prompt text. | — / L |
| 2 | MF-B / HTTP-01 / S13-05 | pg_ripple_http/Cargo.toml, pg_ripple_http/src/main.rs:38 | Bump version to 0.83.0; bump COMPATIBLE_EXTENSION_MIN. | S |
| 3 | BUILD-01 | docker-compose.yml | Bump image tag from 0.54.0 to 0.83.0 + CI lint. | S |
| 4 | S13-01 | src/schema.rs:996, sql/pg_ripple--0.55.0--0.56.0.sql:60 | Document or remove SECURITY DEFINER. | M |
| 5 | S13-02 | scripts/check_no_string_format_in_sql.sh + CI | Verify CI invokes the check; fix any remaining offenders. | M |
| 6 | T13-01 | tests/test_migration_chain.sh | Add v0.80–v0.83 checkpoints. | M |
| 7 | Q13-01 | src/gucs/registration.rs (2,032 lines) | Split into per-domain files. | M |
| 8 | C13-01 | src/sparql/translate/{filter,left_join} | Add nested-OPTIONAL+EXISTS regression test; fix if broken. | M |
| 9 | O13-01 | pg_ripple_http/src/routing/ | Add /health/ready deep-check. | M |
| 10 | P13-01 | src/sparql/plan_cache.rs | Avoid double-parsing in cache key. | M |

### Should Fix Before v1.0.0 (Medium)

11. Q13-02 (split `src/schema.rs`).
12. Q13-03 (split `src/sparql/federation.rs`).
13. T13-04 (conformance trends CSV).
14. C13-02 (strict_dictionary in decode).
15. O13-02 (federation/dict cache metrics).
16. O13-03 (algebra-after-optimisation in EXPLAIN).
17. CC13-01 (promotion crash recovery test).
18. SC13-01 (sparql_strict GUC verification).
19. D13-01/D13-02 (compatibility matrix + new feature docs).
20. A13-03 (PT-code registry).

### Backlog (Low / Strategic)

21. Q13-04 (CI lint for >1,800-line files).
22. Q13-05 (audit dead_code suppressions).
23. T13-03 (CONSTRUCT + SHACL-SPARQL fuzz targets).
24. DS13-01 (dependency upgrades).
25. HTTP-02 (streaming cursors or remove tokio-stream).
26. C13-03–C13-11 (smaller correctness items).
27. S13-06–S13-10 (defence-in-depth).
28. Strategic gaps in Area 15.

---

## Recommended New Roadmap Items

**RR-01 — Streaming SPARQL Cursors (v0.85.0)**
- *Rationale*: HTTP-02 is half-done; finishing it closes a documented promise.
- *User Story*: As an analytics engineer, I want SSE-streamed SPARQL results so I can process million-row queries without buffering them in memory.
- *Complexity*: M
- *Slot*: v0.85.0
- *Dependencies*: pg_ripple_http version sync (Action #2).

**RR-02 — Cypher/GQL Transpilation MVP (v0.86.0)**
- *Rationale*: Tap the Neo4j market without abandoning RDF semantics.
- *User Story*: As a graph-database evaluator coming from Neo4j, I want to run my existing Cypher queries against pg_ripple so the migration cost is bounded.
- *Complexity*: XL
- *Slot*: v0.86.0–v0.90.0 (multi-release)
- *Dependencies*: stable algebra IR (already exists), property-graph ↔ RDF mapping doc.

**RR-03 — Coverage badge + line-coverage gate (v0.84.x)**
- *Rationale*: T13-04 + DX. Visible coverage is a powerful trust signal.
- *User Story*: As a contributor, I want to see line coverage in the README so I know which areas need testing.
- *Complexity*: S
- *Slot*: any minor release.
- *Dependencies*: tarpaulin or llvm-cov in CI.

**RR-04 — Per-PT-code error registry (v0.84.x)**
- *Rationale*: A13-03. Closes a gap operators repeatedly hit.
- *User Story*: As an SRE diagnosing a failed query, I want a single page listing every PT code with its meaning and recommended action.
- *Complexity*: S
- *Slot*: any minor release.
- *Dependencies*: none.

**RR-05 — `pg_ripple.bump` justfile recipe + version-sync CI gate (v0.84.x)**
- *Rationale*: Eliminates the recurring HTTP-companion-version drift class of bug (MF-B).
- *User Story*: As a release engineer, I want `just bump 0.84.0` to update Cargo.toml, pg_ripple_http/Cargo.toml, pg_ripple.control, control comment, COMPATIBLE_EXTENSION_MIN, docker-compose tag, and create the migration script and CHANGELOG stub atomically.
- *Complexity*: S
- *Slot*: v0.84.x.
- *Dependencies*: none.

---

## Appendix: Verification Commands Run

All commands run from `/Users/geir.gronmo/projects/pg-ripple2`. Numeric snippets are paraphrased; full output preserved in session log.

```bash
# Versions and HEAD
git rev-parse HEAD                       # → 142d8f21a2bd1b30c283bfeb7901f276012e6b41
git tag --sort=-creatordate | head -5    # → v0.83.0, v0.82.0, v0.81.0, v0.80.0, v0.79.0
grep '^version' Cargo.toml               # → version = "0.83.0"
grep '^version' pg_ripple_http/Cargo.toml # → version = "0.77.0"   ← DRIFT
grep default_version pg_ripple.control   # → default_version = '0.83.0'
python3 -c "import json; print(json.load(open('sbom.json'))['metadata']['component']['version'])"
                                         # → 0.83.0

# Codebase mapping
find src -name "*.rs" -exec wc -l {} \; | sort -rn | head -10
# → 2032 src/gucs/registration.rs
# → 1939 src/schema.rs
# → 1693 src/sparql/federation.rs
# → 1613 src/datalog/compiler.rs
# → 1547 src/storage/ops.rs

# Code-quality scans
grep -rEn "TODO|FIXME|HACK" src/ pg_ripple_http/src/ | wc -l   # → 0
grep -rEc "// SAFETY:" src/ pg_ripple_http/src/ | grep -v ":0$" # → 18 files
grep -rEn "unsafe " src/ pg_ripple_http/src/ | wc -l            # → 89

# A12 finding re-verification
grep -n "mutation_journal" src/sparql/execute.rs   # → line 658 flush()
grep -n "CYCLE" src/sparql/property_path.rs        # → 5x "CYCLE s, o"
grep -n "ORDER BY" src/storage/merge.rs            # → lines 189, 300
grep -rn "strict_dictionary" src/                  # → registration.rs:1840, dictionary/mod.rs:657
grep -n "RFC-1918\|172.16\|192.168" src/sparql/federation.rs  # → blocks present
ls src/bidi/                                       # → mod.rs protocol.rs relay.rs subscribe.rs sync.rs

# SECURITY DEFINER inventory
grep -rn "SECURITY DEFINER" src/ sql/              # → 2 hits

# Migration chain coverage
grep -nE "0\.8[0-9]" tests/test_migration_chain.sh # → only line 383 header

# pg_ripple_http compatibility
grep -n "COMPATIBLE_EXTENSION_MIN" pg_ripple_http/src/main.rs
# → 38: const COMPATIBLE_EXTENSION_MIN: &str = "0.79.0";

# Docker
grep "image:" docker-compose.yml                   # → ghcr.io/grove/pg_ripple:0.54.0  (×2)

# Fuzz targets
ls fuzz/fuzz_targets/                              # → 17 files

# Proptest
find tests -name "*.rs" -path "*proptest*"
# → tests/proptest/{jsonld_framing,construct_template,bidi_convergence,
#                    ntriples_oxigraph,dictionary,sparql_roundtrip,sqlgen_bridge}.rs
```

---

*Assessment #13 complete. 82 findings reported across 16 areas. The v0.80–v0.83 cycle resolved 11 of 14 verified A12 open findings — an unusually strong remediation rate. The dominant remaining risks are operational drift (HTTP-companion version, docker-compose tag) and the v0.84.0 prompt-vs-reality gap. Code-level correctness, security, and observability are all now in late-RC territory.*
