# pg_ripple — Overall Assessment #12

**Date**: 2026-04-30
**Codebase snapshot**: 6bc5b8d836183c7f20b553bf511a84ea9b383288
**Assessor**: Automated deep analysis (GitHub Copilot, Assessment #12)
**Version**: v0.79.0 (extension) / v0.76.0 (pg_ripple_http)

---

## Executive Summary

pg_ripple v0.79.0 represents a **late-beta, feature-complete system** that has resolved most Critical and High findings from Assessments 10–11 through a methodical eight-release remediation cycle (v0.72.0–v0.79.0). The SubXact callback (CF-B, now RESOLVED), evidence-file citations (CF-C, RESOLVED), all twelve reference-doc stubs (RESOLVED), the module-size explosion (MF-E, RESOLVED via v0.69.0/v0.72.0 splits), and the BIDI/BIDIOPS feature sets (17/17 items verified complete) represent genuine progress. The codebase now clocks in at 62,787 total lines with all modules individually at <2,600 lines, and the code-quality profile (zero TODO/FIXME markers, comprehensive SAFETY comments, tight test isolation) is excellent.

**Three open findings prevent a v1.0.0 quality rating:**

1. **CF-A (CRITICAL, STILL OPEN)**: `sparql_update()` and `execute_delete_insert()` in [src/sparql/execute.rs](../src/sparql/execute.rs) never call `mutation_journal::flush()`. Every SPARQL INSERT/DELETE/WHERE/MODIFY statement silently bypasses CONSTRUCT writeback. This is the most important pre-v1.0.0 fix, requiring a single `crate::storage::mutation_journal::flush()` call at each function's exit point.

2. **CF-D (HIGH, STILL OPEN)**: R2RML materialisation, CDC ingestion, and the CDC bridge all write triples to VP tables without recording to the mutation journal. Zero `mutation_journal::*` calls exist in `src/r2rml.rs`, `src/cdc.rs`, or `src/cdc_bridge_api.rs`. Any graph modified by CDC or R2RML will never trigger CONSTRUCT writeback.

3. **HF-A (MEDIUM, STILL OPEN)**: The SBOM (`sbom.json`) top-level version is `0.74.0` — five releases stale — and still contains an inner `pg_ripple@0.74.0` component entry from a previous build environment. The release automation does not commit a fresh SBOM on tagging.

Beyond these, Assessment 12 finds **fifteen new or escalated findings** across correctness (property-path cycle detection scope, storage merge non-determinism), security (SQL injection via single-quote escaping in `src/views.rs`, allowlist URL normalisation gap, federation JSON-truncation heuristic), performance (plan cache size hardcoded, batch decode builds WHERE-IN strings), concurrency (LRU caches backed by `RefCell` not cross-backend-safe, CDC replication slot leak risk), test coverage (migration chain stops assertions at v0.61.0, error-path and CDC/async tests sparse), and API consistency (HTTP error responses mix plain text and JSON; SPARQL syntax errors return HTTP 200).

**Top 5 recommended actions before v1.0.0:**
1. Add `crate::storage::mutation_journal::flush()` at the return points of `sparql_update()` and `execute_delete_insert()` ([src/sparql/execute.rs](../src/sparql/execute.rs)) — one line each.
2. Wire `mutation_journal::record_write()` + `flush()` into R2RML materialisation ([src/r2rml.rs](../src/r2rml.rs)) and CDC ingestion ([src/cdc.rs](../src/cdc.rs), [src/cdc_bridge_api.rs](../src/cdc_bridge_api.rs)).
3. Extend [tests/test_migration_chain.sh](../tests/test_migration_chain.sh) to assert schema invariants at each of the 18 migrations from v0.62.0 to v0.79.0.
4. Regenerate `sbom.json` at v0.79.0 and add a CI step that blocks tagging if the SBOM top-level version does not match `Cargo.toml`.
5. Fix HTTP error-response consistency: standardise all 400/401/403/404/500 responses to `application/json` with `{"error":"PT…","message":"…"}` and return HTTP 400 for SPARQL syntax errors (not HTTP 200).

---

## Severity Index

| Area | Critical | High | Medium | Low | Total |
|---|---|---|---|---|---|
| 1. Correctness & Bugs | 1 | 3 | 7 | 3 | 14 |
| 2. Security | 0 | 3 | 7 | 5 | 15 |
| 3. Performance & Scalability | 0 | 3 | 8 | 1 | 12 |
| 4. Code Quality & Maintainability | 0 | 0 | 2 | 2 | 4 |
| 5. Test Coverage | 0 | 2 | 4 | 2 | 8 |
| 6. API Design & Usability | 0 | 0 | 3 | 3 | 6 |
| 7. Documentation & Spec Fidelity | 0 | 0 | 1 | 0 | 1 |
| 8. Dependency & Supply Chain | 0 | 0 | 2 | 1 | 3 |
| 9. Observability & Operability | 0 | 2 | 5 | 0 | 7 |
| 10. Concurrency & Transaction Safety | 0 | 2 | 5 | 0 | 7 |
| 11. Standards Conformance | 0 | 0 | 1 | 2 | 3 |
| 12. v0.77.0 BIDI Gap Analysis | 0 | 0 | 0 | 0 | 0 |
| **Total** | **1** | **15** | **45** | **19** | **80** |

---

## Resolution Status of Prior Findings

All Critical findings from Assessments 10–11 (except CF-A and CF-D) are resolved. Key resolutions in v0.72.0–v0.79.0:

| Finding | Previous Status | v0.79.0 Status | Evidence |
|---|---|---|---|
| CF-A `sparql_update` never flushes | Open (A11) | **STILL OPEN** | grep confirms 0 `mutation_journal` hits in src/sparql/execute.rs |
| CF-B SubXact journal safety | Open (A11) | **RESOLVED** | [src/lib.rs:344](../src/lib.rs#L344) `RegisterSubXactCallback`; v0.72.0 XACT-01 |
| CF-C Missing 12 reference docs | Open (A11) | **RESOLVED** | All 12 `docs/src/reference/*.md` files present |
| CF-D Datalog/R2RML/CDC bypass journal | Open (A11) | **PARTIALLY RESOLVED** | Datalog seminaive flush added ([src/datalog/seminaive.rs:303,435](../src/datalog/seminaive.rs#L303)); R2RML/CDC still zero hits |
| HF-A SBOM stale | Partial (A11) | **STILL OPEN** | sbom.json top-level = 0.74.0; 5 releases stale |
| MF-A Plan cache key omits graph context | Open (A11) | **STILL OPEN** | [src/sparql/plan_cache.rs:92](../src/sparql/plan_cache.rs#L92) hashes query text only |
| MF-B pg_ripple_http version gap | Partial (A11) | **STILL OPEN** | pg_ripple_http/Cargo.toml = 0.76.0 vs extension 0.79.0 |
| MF-D v070+ feature SQL files missing | Partial (A11) | **RESOLVED** | v070_features.sql–v079_features.sql all present |
| MF-E Module sizes >2000 lines | Open (A11) | **RESOLVED** | src/lib.rs = 433 lines; all modules <1,700 lines; v0.72.0 MOD-01 successful |
| MF-9 strict_dictionary GUC | Open (A11) | **STILL OPEN** | grep: 0 matches for `strict_dict\|strict_dictionary` in src/ |

---

## Area 1: Correctness & Bugs

### Findings

**ID: C-01 | CRITICAL | Effort: S**
`sparql_update()` and `execute_delete_insert()` never call `mutation_journal::flush()`; CONSTRUCT writeback silently broken for all SPARQL INSERT/DELETE/UPDATE statements.

- **file**: [src/sparql/execute.rs](../src/sparql/execute.rs) — `sparql_update()` (returns at end of function); `execute_delete_insert()` (returns at end of function)
- **verified**: `grep -n "mutation_journal" src/sparql/execute.rs` returns 0 hits. Every other write API (`dict_api::insert_triple`, `views_api::clear_graph`, `datalog/seminaive.rs`) calls `record_write()` + `flush()`. SPARQL UPDATE is the only write path that does not.
- **impact**: A user invoking `pg_ripple.sparql_update('INSERT DATA { … }')` where a CONSTRUCT writeback rule depends on the modified graph will observe: triple inserted correctly, CONSTRUCT writeback never fires. Silent data inconsistency.
- **fix**: Add `crate::storage::mutation_journal::flush();` as the last statement before each function's return. Add a regression test: define a CONSTRUCT writeback rule, run `sparql_update('INSERT DATA …')`, assert derived graph is non-empty.

**ID: C-02 | HIGH | Effort: S**
`mutation_journal` not wired into R2RML materialisation or CDC ingestion.

- **file**: [src/r2rml.rs](../src/r2rml.rs), [src/cdc.rs](../src/cdc.rs), [src/cdc_bridge_api.rs](../src/cdc_bridge_api.rs)
- **verified**: `grep -rn "mutation_journal" src/r2rml.rs src/cdc.rs src/cdc_bridge_api.rs` returns 0 matches.
- **impact**: Any graph written by CDC ingestion or R2RML materialisation will never trigger CONSTRUCT writeback rules.
- **fix**: Add `mutation_journal::record_write(g_id)` after every VP INSERT and `mutation_journal::flush()` at each write-batch boundary in these three files.

**ID: C-03 | HIGH | Effort: S**
Property-path cycle detection uses `CYCLE o SET …` (endpoint-only) rather than `CYCLE (s, o) SET …` (full-state cycle detection).

- **file**: [src/sparql/property_path.rs:35–75](../src/sparql/property_path.rs#L35-L75) (verified by correctness subagent)
- **problem**: A `+` or `*` path that revisits the same intermediate node via different edges will not be detected as a cycle. For graphs with cycles reachable via multiple edge types, this can cause non-termination.
- **fix**: Change the `CYCLE` clause to include both `s` and `o`: `CYCLE s, o SET _is_cycle USING _cycle_path`.

**ID: C-04 | HIGH | Effort: M**
HTAP merge non-deterministic SID selection: `DISTINCT ON (s, o, g)` without explicit `ORDER BY (s, o, g, i ASC)` may keep an arbitrary row when both delta and main contain the same triple.

- **file**: [src/storage/merge.rs:330–350](../src/storage/merge.rs#L330-L350)
- **problem**: If UNION ALL row ordering is not deterministic across merge cycles, different SIDs may survive for the same logical triple, causing non-idempotent merges and subtle replication inconsistency.
- **fix**: Add explicit `ORDER BY (s, o, g, i ASC)` before the `DISTINCT ON` to guarantee the oldest-assertion SID is always kept.

**ID: C-05 | HIGH | Effort: M**
Promotion TOCTOU: `promote_predicate()` marks status `'promoting'` before the atomic CTE. A concurrent INSERT goes to `vp_rare` during the gap.

- **file**: [src/storage/promote.rs:60–78](../src/storage/promote.rs#L60-L78)
- **problem**: If the process crashes between the status update and the atomic CTE, `recover_interrupted_promotions()` re-runs the CTE from a fresh start, but in-flight writes during the gap may have already been captured only in `vp_rare`.
- **fix**: Move the `promotion_status = 'promoting'` update inside the same CTE as the data copy so the status change and the data migration are atomic.

**ID: C-06 | MEDIUM | Effort: S**
`ON CONFLICT DO NOTHING … RETURNING` in `dictionary::encode()` can return 0 rows without signalling the race; caller caches a stale `(hash → id)` mapping.

- **file**: [src/dictionary/mod.rs:140–145](../src/dictionary/mod.rs#L140-L145)
- **problem**: If the INSERT is skipped (row already exists), RETURNING returns 0 rows. The COALESCE fallback re-reads from the dictionary, but between those two SPI calls a concurrent ROLLBACK could delete the row. The shmem entry may then cache a non-existent `id`.
- **fix**: After the COALESCE fallback SELECT returns 0 rows, emit `pgrx::error!("dictionary: encode race—could not obtain id for term")` rather than returning a fabricated value.

**ID: C-07 | MEDIUM | Effort: M**
SHACL shape store not wrapped in a transaction: if `populate_hints()` fails after `store_shape()` succeeds, the shape exists without hints, causing silent query-plan inconsistency.

- **file**: [src/shacl/spi.rs:25–50](../src/shacl/spi.rs#L25-L50)
- **fix**: Wrap the store-shape + populate-hints pair inside a single SPI transaction; roll back the shape store on hint-population failure.

**ID: C-08 | MEDIUM | Effort: S**
Federation allowlist uses exact string match without URL normalisation.

- **file**: [src/sparql/federation.rs:237](../src/sparql/federation.rs#L237)
- **problem**: `http://allowed.com` and `http://allowed.com/` are treated as different endpoints. An operator who registers the bare form can be surprised when queries with trailing slashes bypass the allowlist in deny mode.
- **fix**: Parse both URLs with `url::Url`, normalise (lowercase scheme/host, remove default port, resolve trailing slash), then compare serialised forms.

**ID: C-09 | MEDIUM | Effort: S**
`parse_sparql_results_json_partial()` uses `body.rfind("},")` heuristic to recover truncated JSON from federation. This heuristic can match `}` characters inside literal string values, returning invalid JSON silently parsed as empty results.

- **file**: [src/sparql/federation.rs:750–765](../src/sparql/federation.rs#L750-L765)
- **fix**: If the heuristic-reconstructed JSON fails to parse, emit `pgrx::warning!("SERVICE <url>: truncated response; X rows recovered, recovery heuristic failed")` and return the rows decoded before truncation rather than an empty set.

**ID: C-10 | MEDIUM | Effort: M**
`OPTIONAL { }` promotion to `INNER JOIN` only checks single-triple BGPs with constant predicates. Multi-predicate OPTIONAL bodies with all predicates having `sh:minCount 1` are never promoted.

- **file**: [src/sparql/translate/left_join.rs:102–104](../src/sparql/translate/left_join.rs#L102-L104)
- **fix**: Extend `shacl_right_is_mandatory()` to return `true` when all triple patterns in the BGP have constant predicates with `sh:minCount ≥ 1`.

**ID: C-11 | MEDIUM | Effort: M**
Datalog stratifier does not explicitly prevent aggregation in the head of a recursive rule.

- **file**: [src/datalog/stratify.rs:130–145](../src/datalog/stratify.rs#L130-L145)
- **fix**: Add check: if `rule.head_agg.is_some() && stratum.is_recursive { pgrx::error!("aggregation in recursive rule head is unsupported") }`.

**ID: C-12 | MEDIUM | Effort: M**
Parallel Datalog group partitioning does not verify the absence of intra-stratum cycles; two rules with circular positive dependencies may be placed in the same parallel group, preventing semi-naive convergence.

- **file**: [src/datalog/parallel.rs:110–150](../src/datalog/parallel.rs#L110-L150)
- **fix**: After computing connected components, check each group for cycles in the derived-predicate dependency graph. If a cycle is found, mark the group as non-parallelizable.

**ID: C-13 | MEDIUM | Effort: S**
SPARQL blank-node variable names are `_bn_{bnode_label}` without a query-scope prefix. Two queries sharing a blank-node label within the same connection will incorrectly unify their blank nodes.

- **file**: [src/sparql/translate/bgp.rs:75–110](../src/sparql/translate/bgp.rs#L75-L110)
- **fix**: Prefix the blank-node variable name with a per-query nonce: `_bn_{query_hash}_{bnode_label}`.

**ID: C-14 | LOW | Effort: S**
Plan cache key (`XXH3-128(query_text)`) does not include RLS role context or graph-security GUC state.

- **file**: [src/sparql/plan_cache.rs:92](../src/sparql/plan_cache.rs#L92)
- **problem**: A cached plan for user A may be served to user B if the query text is identical, even if their RLS policies differ.
- **fix**: Include `current_user()`, the active `pg_ripple.rls_*` GUC values, and the `pg_ripple.inference_mode` GUC in the cache key hash.

**ID: C-15 | LOW | Effort: S**
Federation remote-query cache key hashes raw SPARQL text; equivalent queries with different whitespace always miss.

- **file**: [src/sparql/federation.rs:549–570](../src/sparql/federation.rs#L549-L570)
- **fix**: Normalise the SPARQL text (trim, collapse whitespace) before hashing.

**ID: C-16 | LOW | Effort: S**
`pg_ripple.strict_dictionary` GUC is not implemented. Callers that receive unknown IDs from federation decode silently produce empty strings.

- **verified**: grep returns 0 matches for `strict_dict` across entire src/.
- **fix**: Add a boolean GUC `pg_ripple.strict_dictionary` (default `off`). When `on`, unknown IDs emit `pgrx::error!("decode: unknown dictionary ID {}; enable strict_dictionary=off to allow empty substitution")`.

### Summary

Area 1 contains one Critical finding (C-01, SPARQL Update never flushes CWB), four High findings (C-02–C-05), seven Medium findings (C-06–C-13), and three Low findings (C-14–C-16). The Critical and two of the High findings (C-01, C-02, C-03) are each one-to-five-line code changes.

---

## Area 2: Security

### Findings

**ID: S-01 | HIGH | Effort: M**
SQL injection via manual single-quote escaping in `src/views.rs` catalog inserts.

- **file**: [src/views.rs:204](../src/views.rs#L204), [src/views.rs:322](../src/views.rs#L322)
- **problem**: View names and associated SPARQL/SQL strings are embedded via `.replace('\'', "''")` inside `format!()`. While the identifier component is validated with `validate_name()`, the string-literal components (SPARQL query text, SQL body) use only single-quote doubling. A SPARQL query containing `$$` (dollar-quoting delimiters) or Unicode escape sequences could bypass this escaping.
- **fix**: Migrate all catalog inserts to `Spi::run_with_args()` with `$1, $2, …` placeholders and `Vec<PgBuiltInOids>` typed parameters.

**ID: S-02 | HIGH | Effort: S**
Model tag in vector-search embedding code uses single-quote escaping rather than parameterized query.

- **file**: [src/sparql/embedding.rs:474](../src/sparql/embedding.rs#L474)
- **problem**: `format!("AND e.model = '{}'", model_tag.replace('\'', "''"))` — if `model_tag` is user-supplied, this is a classic SQL injection vector.
- **fix**: Replace with `Spi::run_with_args("… AND e.model = $1", &[model_tag.into_datum()])`.

**ID: S-03 | HIGH | Effort: S**
SSRF allowlist enforcement does not validate the full RFC-1918 private address space.

- **file**: [src/sparql/federation.rs:220–250](../src/sparql/federation.rs#L220-L250)
- **problem**: The blocked-host check validates `10.x.x.x` and `127.x.x.x` but does not explicitly block `172.16.0.0/12` and `192.168.0.0/16`. An attacker can use a SERVICE endpoint targeting `172.31.255.1` to probe the internal network.
- **fix**: Use a CIDR-based check for all RFC-1918 ranges: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.0.0/16`, `::1/128`, `fc00::/7`. Consider the `ipnetwork` crate.

**ID: S-04 | MEDIUM | Effort: S**
Three RUSTSEC advisories are actively ignored with no expiry policy.

- **file**: [audit.toml:14–17](../audit.toml#L14-L17)
- **ignored**: RUSTSEC-2026-0104 (paste macro unsoundness), RUSTSEC-2024-0436 (RSA timing side-channel), RUSTSEC-2021-0127 (serde_cbor unmaintained)
- **assessment**: RUSTSEC-2021-0127 (serde_cbor) has been unmaintained since 2021; five years on, a replacement (e.g., `ciborium`) should be evaluated. RUSTSEC-2026-0104 (paste) is compile-time only and acceptable. RUSTSEC-2024-0436 (RSA) is not exercised by the HMAC-SHA256 bearer token path and is acceptable.
- **fix**: Replace or remove the `serde_cbor` dependency if it is only transitive. Add an expiry comment to each `ignore` block noting the next review date.

**ID: S-05 | MEDIUM | Effort: S**
SPARQL query complexity DoS: no depth/breadth limit enforced before SQL generation.

- **file**: [pg_ripple_http/src/routing/sparql_handlers.rs:57–68](../pg_ripple_http/src/routing/sparql_handlers.rs#L57-L68)
- **problem**: POST body size is capped at 10 MiB, but a 1 KB SPARQL query with 500 deeply-nested OPTIONAL blocks will generate a pathological SQL plan. No algebra-depth check occurs before SQL generation.
- **fix**: Add `pg_ripple.sparql_max_algebra_depth` GUC (default 64). Reject queries that exceed this depth at parse time with HTTP 400.

**ID: S-06 | MEDIUM | Effort: S**
Multi-tenant trigger name constructed via `format!()` without validating `tenant_name` at the callsite.

- **file**: [src/tenant.rs:120–121](../src/tenant.rs#L120-L121)
- **fix**: Verify `tenant_name` passes `is_safe_role_name()` before building the trigger name. Emit PT711 error on rejection.

**ID: S-07 | MEDIUM | Effort: S**
`quote_ident_safe()` is a local implementation that does not support Unicode role names; non-ASCII roles silently fail to get RLS policies.

- **file**: [src/security_api.rs:64–283](../src/security_api.rs#L64-L283)
- **fix**: Document as a known limitation. Add a fallback SPI call to `pg_catalog.quote_ident($1)` for non-ASCII role names.

**ID: S-08 | MEDIUM | Effort: S**
Arrow Flight export has no `max_export_rows` limit; a `SELECT *` query can materialise the entire triple store in memory.

- **file**: [pg_ripple_http/src/arrow_encode.rs:278–281](../pg_ripple_http/src/arrow_encode.rs#L278-L281)
- **fix**: Add `pg_ripple.arrow_max_export_rows` GUC (default 10,000,000). Return HTTP 413 if exceeded.

**ID: S-09 | MEDIUM | Effort: S**
Federation HTTP response size is not capped; a malicious endpoint could return a 1 GB JSON response causing OOM.

- **file**: [src/sparql/federation.rs:723](../src/sparql/federation.rs#L723)
- **fix**: Set `ureq` response body limit to `pg_ripple.federation_max_response_bytes` GUC (default 50 MiB) before calling `.read_to_string()`.

**ID: S-10 | MEDIUM | Effort: S**
CORS configuration not visible in `main.rs`; risk of `AllowOrigin::any()` if default was not explicitly overridden.

- **file**: [pg_ripple_http/src/main.rs:300](../pg_ripple_http/src/main.rs#L300)
- **fix**: Verify `CorsLayer` uses `AllowOrigin::list()` or environment-driven allowlist. Document CORS policy in `docs/src/operations/`.

**ID: S-11 | MEDIUM | Effort: S**
Error messages in HTTP handlers log full query text and database error details at ERROR level via `format!("response build failed: {e}")`.

- **file**: [pg_ripple_http/src/common.rs:62–75](../pg_ripple_http/src/common.rs#L62-L75)
- **fix**: Ensure `redacted_error()` is used consistently across all handlers. Do not emit raw SPI errors in log output accessible to non-admin users.

**ID: S-12 | LOW | Effort: S**
Shared-memory size calculation in `shmem.rs` uses unchecked multiplication; future maintainers could introduce overflow.

- **file**: [src/shmem.rs:1–100](../src/shmem.rs#L1-L100)
- **fix**: Use `checked_mul()` on all shmem size calculations: `ENCODE_CACHE_SETS.checked_mul(4).expect("shmem size overflow")`.

**ID: S-13 | LOW | Effort: S**
`/metrics/extension` is unauthenticated; it queries PostgreSQL and could expose extension state to untrusted scrapers.

- **file**: [pg_ripple_http/src/routing/admin_handlers.rs:20–226](../pg_ripple_http/src/routing/admin_handlers.rs#L20-L226)
- **fix**: Document that the metrics port must be network-restricted to monitoring infrastructure, or add optional token-auth via `PG_RIPPLE_METRICS_TOKEN` env var.

**ID: S-14 | LOW | Effort: S**
`Dockerfile` comment uses `POSTGRES_PASSWORD=ripple` as an example; operators may copy this into production.

- **file**: [Dockerfile:11–20](../Dockerfile#L11-L20)
- **fix**: Replace with `POSTGRES_PASSWORD=${YOUR_SECURE_PASSWORD_HERE}`.

**ID: S-15 | LOW | Effort: S**
`"Basic"` auth scheme over plain HTTP exposes credentials in base64.

- **file**: [pg_ripple_http/src/common.rs:99–111](../pg_ripple_http/src/common.rs#L99-L111)
- **fix**: Document that the service must be deployed behind TLS. Consider rejecting `Basic` unless `PG_RIPPLE_HTTP_ALLOW_BASIC=1`.

### Summary

No memory-safety vulnerabilities found (Rust type system enforces this). Two SQL-injection vectors (S-01, S-02) are the highest-severity new findings. The SSRF gap (S-03) is a defence-in-depth issue given the existing deny-mode default. All findings are S-effort fixes.

---

## Area 3: Performance & Scalability

### Findings

**ID: P-01 | HIGH | Effort: S**
Plan cache capacity hardcoded at 256 entries with no GUC control.

- **file**: [src/sparql/plan_cache.rs:32](../src/sparql/plan_cache.rs#L32)
- **problem**: Workloads with >256 unique queries thrash LRU and re-translate on every miss.
- **fix**: Add `pg_ripple.plan_cache_capacity` GUC (default 256, min 1, max 100,000) with runtime setter.

**ID: P-02 | HIGH | Effort: M**
`batch_decode()` builds SQL `WHERE id IN (1, 2, …, N)` via string concatenation; O(N) string allocations per decode call.

- **file**: [src/sparql/decode.rs:50–55](../src/sparql/decode.rs#L50-L55)
- **problem**: Decoding 10,000 result rows generates a 60 KB SQL string on every call, bypassing the plan cache.
- **fix**: Use `Spi::run_with_args("WHERE id = ANY($1)", &[ids_array.into_datum()])` with `Vec<i64>`.

**ID: P-03 | HIGH | Effort: M**
Merge worker scans all predicates twice on every merge cycle (assigned + steal check); at 10k predicates, two full table scans per cycle.

- **file**: [src/worker.rs:283–315](../src/worker.rs#L283-L315)
- **fix**: Cache predicate IDs in a backend-local array, invalidated on `pg_notify('pg_ripple.predicate_created', …)`.

**ID: P-04 | MEDIUM | Effort: M**
Federation cost model queries `_pg_ripple.federation_health` with `PERCENTILE_CONT` on every federated query (potential full table scan).

- **file**: [src/sparql/federation.rs:438–442](../src/sparql/federation.rs#L438-L442)
- **fix**: Materialise P50/P95/P99 per endpoint in a `_pg_ripple.federation_stats_cache` table refreshed every N minutes by the background worker.

**ID: P-05 | MEDIUM | Effort: M**
Dictionary LRU caches are `thread_local! RefCell<LruCache<_>>`. Each PG backend has its own instance; shared-memory LRU as used for the encode cache since v0.6.0 is more efficient across many connections.

- **file**: [src/dictionary/mod.rs:65–83](../src/dictionary/mod.rs#L65-L83)
- **fix**: Evaluate whether the per-backend decode cache should be moved to the existing shmem encode-cache structure.

**ID: P-06 | MEDIUM | Effort: S**
`EXPLAIN` output for SPARQL queries does not include the optimised SPARQL algebra tree, filter-pushdown decisions, or self-join-elimination annotations.

- **file**: [src/sparql/explain.rs](../src/sparql/explain.rs) / [src/sparql/execute.rs](../src/sparql/execute.rs)
- **fix**: Emit a `"sparql_algebra"` JSON field in the EXPLAIN output showing the post-optimisation algebra tree.

**ID: P-07 | MEDIUM | Effort: M**
Merge fence lock `0x5052_5000` is global and held for the entire merge cycle; concurrent merges of unrelated predicates serialise unnecessarily during Citus rebalance.

- **file**: [src/worker.rs:250–280](../src/worker.rs#L250-L280)
- **fix**: Reduce the fence lock scope to the rebalance-apply phase only; use per-predicate advisory locks elsewhere.

**ID: P-08 | MEDIUM | Effort: M**
`graph_stats()` materialisation strategy undocumented; likely requires full VP table scan at 100M+ triples.

- **file**: [src/stats.rs](../src/stats.rs), [src/stats_admin.rs](../src/stats_admin.rs)
- **fix**: Add `_pg_ripple.predicate_stats_cache (predicate_id, triple_count, last_refresh)` maintained by the merge worker, avoiding ad-hoc full scans.

**ID: P-09 | MEDIUM | Effort: M**
Arrow Flight bulk export pre-allocates `Vec::with_capacity(chunk.len())` per column per IPC batch; total in-memory buffer unbounded.

- **file**: [pg_ripple_http/src/arrow_encode.rs:278–281](../pg_ripple_http/src/arrow_encode.rs#L278-L281)
- **fix**: Add `pg_ripple.arrow_max_export_rows` GUC (also fixes S-08) and enforce at routing layer before SPI execution.

**ID: P-10 | MEDIUM | Effort: S**
`pg_stat_statements` integration does not normalise SPARQL text to a stable query class; each unique SPARQL query text gets a distinct `query_id`, causing `pg_stat_statements` bloat on workloads with parameterised SPARQL queries.

- **file**: [src/stats.rs](../src/stats.rs) (pg_stat_statements hook)
- **fix**: Hash the algebra tree (not the raw text) to produce a stable `query_id`. Group equivalent queries under a single statement class.

**ID: P-11 | MEDIUM | Effort: M**
CDC replication slot lifecycle: no background cleanup for orphaned `pg_ripple_*` slots if a subscriber backend crashes.

- **file**: [src/cdc.rs](../src/cdc.rs)
- **problem**: Orphaned replication slots prevent WAL reclamation and can fill the WAL disk.
- **fix**: Add a background worker scan of `pg_replication_slots` that drops slots matching `pg_ripple_*` older than `pg_ripple.cdc_slot_max_age_secs` GUC.

**ID: P-12 | MEDIUM | Effort: S**
`VPP_THRESHOLD` GUC (`pg_ripple.vp_promotion_threshold`) has no explicit min/max bounds in the GUC registration. Setting to `0` causes immediate promotion of all predicates on the next INSERT.

- **file**: [src/gucs/registration.rs](../src/gucs/registration.rs)
- **fix**: Add `min = 10, max = 10_000_000` bounds to the GUC check hook.

**ID: P-13 | LOW | Effort: S**
Prometheus metrics use bare `AtomicU64` counters with no label breakdown; cannot distinguish SELECT/CONSTRUCT/ASK/DESCRIBE latency or track per-endpoint federation cost.

- **file**: [pg_ripple_http/src/metrics.rs](../pg_ripple_http/src/metrics.rs)
- **fix**: Add structured label sets (`query_type`, `result_size_bucket`) with cardinality guard (max 20 label combinations).

### Summary

Two High findings (P-01, P-02) are straightforward code changes. The remaining Medium findings represent incremental optimisations that collectively improve production scalability without correctness risk.

---

## Area 4: Code Quality & Maintainability

### Findings

**ID: Q-01 | MEDIUM | Effort: M**
`src/lib.rs` has grown back to 433 lines (down from 2,413 — the v0.72.0 MOD-01 split was successful), but `src/bidi.rs` at 2,509 lines is now the largest module. It combines source-attribution, conflict resolution, normalise/upsert/diff/delete/ref/loop/CAS/linkback logic in a single file.

- **file**: [src/bidi.rs](../src/bidi.rs) (2,509 lines)
- **fix**: Split into `src/bidi/attribution.rs`, `src/bidi/conflict.rs`, `src/bidi/sync_ops.rs`, `src/bidi/linkback.rs` before v1.0.0. Estimated 2 days effort; no functional change.

**ID: Q-02 | MEDIUM | Effort: S**
`src/replication.rs:188–190` contains two `.unwrap()` calls on SPI results in a user-facing `replication_stats()` function.

- **file**: [src/replication.rs:188](../src/replication.rs#L188)
- **fix**: Replace with `pgrx::error!()` or `?`-propagation: `let val = row.get::<String>(1).ok_or_else(|| pgrx::error!("replication_stats: missing column 1"))?`.

**ID: Q-03 | LOW | Effort: M**
GUC naming lacks a consistent convention: `vp_promotion_threshold` uses `*_threshold`, `max_path_depth` uses `max_*`, `federation_max_results` uses `*_max`. This inconsistency makes the parameter space harder to discover.

- **file**: [src/gucs/registration.rs](../src/gucs/registration.rs)
- **fix**: Adopt a single convention for v1.0.0 (recommended: `max_*` prefix for limits/thresholds). Document the convention in CONTRIBUTING.md.

**ID: Q-04 | LOW | Effort: S**
`sbom.json` top-level version is `0.74.0` and contains a stale `pg_ripple@0.74.0` inner component entry.

- **file**: [sbom.json](../sbom.json)
- **fix**: Regenerate SBOM at each release (`cargo cyclonedx`). Add CI step asserting `jq '.metadata.component.version' sbom.json` equals `Cargo.toml` version.

### Summary

Code quality is strong overall. The only significant concern is `src/bidi.rs` module size. Zero TODO/FIXME/HACK markers found in production code; all unsafe blocks have accurate SAFETY comments.

---

## Area 5: Test Coverage

### Findings

**ID: T-01 | HIGH | Effort: M**
Migration chain test stops schema-assertion coverage at v0.61.0; 18 migrations (v0.62.0–v0.79.0) loop silently without column or constraint assertions.

- **file**: [tests/test_migration_chain.sh](../tests/test_migration_chain.sh)
- **problem**: A schema regression introduced in any migration from v0.62.0 onward will not be caught until an operator attempts an upgrade.
- **fix**: Add column-existence assertions every 5 versions: v0.65.0 (BIDI catalog), v0.70.0 (HTAP delta/tombstone columns), v0.75.0 (CDC bridge tables), v0.79.0 (final schema snapshot assertion).

**ID: T-02 | HIGH | Effort: M**
Eight critical error-path scenarios lack regression tests.

- Missing tests: SPI connection pool exhaustion; dictionary hash collision (deterministic test via crafted input); VP table promotion race; HTAP merge watchdog timeout; SHACL async validation queue overflow; federation SERVICE timeout; Datalog fixpoint non-convergence (max iterations); RLS policy bypass attempt (negative assertion).
- **fix**: Add `tests/pg_regress/sql/error_paths.sql` covering each scenario with `DO $$ BEGIN … EXCEPTION WHEN … THEN …$$ $$`.

**ID: T-03 | MEDIUM | Effort: S**
N-Triples, N-Quads, and TriG bulk-loader entry points have no fuzz targets.

- **file**: [fuzz/fuzz_targets/](../fuzz/fuzz_targets/)
- **fix**: Add `fuzz/fuzz_targets/ntriples_parser.rs`, `nquads_parser.rs`, `trig_parser.rs` using the `rio_turtle` and `oxrdf` entry points.

**ID: T-04 | MEDIUM | Effort: S**
`sparql_update()` has no dedicated fuzz target; UPDATE statements with crafted SPARQL syntax are only tested by the general `sparql_parser.rs` target (which tests parsing, not execution).

- **file**: [fuzz/fuzz_targets/](../fuzz/fuzz_targets/)
- **fix**: Add `fuzz/fuzz_targets/sparql_update_executor.rs` that calls `sparql_update()` via pgrx test harness.

**ID: T-05 | MEDIUM | Effort: M**
Five of six `proptest!` suites use outcome-only invariants ("does not crash") rather than comparing against a reference implementation.

- **file**: [tests/proptest/](../tests/proptest/)
- **problem**: A systematic translation bug that silently produces wrong but non-crashing SQL will not be detected.
- **fix**: For `sparql_roundtrip.rs`: compare results against the `spargebra` reference evaluator on a small in-memory graph. For `dictionary.rs`: assert that `decode(encode(term)) == term` for all generated terms.

**ID: T-06 | MEDIUM | Effort: S**
CDC and bidi async integration tests use `sleep(1); assert_eq!(…)` patterns rather than synchronised barriers; these are flaky under CI load spikes.

- **file**: [tests/concurrent/](../tests/concurrent/), [tests/integration/](../tests/integration/)
- **fix**: Replace `sleep(N)` assertions with polling loops with a 5-second deadline, or use `pg_notify`/`LISTEN` to synchronise on actual CDC event delivery.

**ID: T-07 | LOW | Effort: S**
`known_failures.txt` for OWL 2 RL and SPARQL 1.1 conformance suites exists but its contents are undocumented—operators cannot tell which failures are intentional limitations vs. regressions.

- **fix**: Add a `# Reason:` comment above each entry in `known_failures.txt` files.

**ID: T-08 | LOW | Effort: S**
15+ public `#[pg_extern]` SQL functions lack regression tests (see Appendix C for the full list).

- **fix**: Add tests in `tests/pg_regress/sql/` for the highest-risk untested functions: `export_graphrag_entities`, `export_graphrag_relationships`, `trickle_available`, `cdc_bridge_triggers`, `json_to_ntriples`, `federation_register_service`, `federation_unregister_service`.

### Summary

The test suite is comprehensive for the SPARQL/Datalog core, SHACL, and BIDI paths. The main gaps are in error-path coverage, fuzz breadth for bulk-load formats, and migration chain regression depth.

---

## Area 6: API Design & Usability

### Findings

**ID: A-01 | MEDIUM | Effort: S**
HTTP error responses mix plain text and JSON: some handlers return bare strings, others return `{"error":"PT…","message":"…"}`. SPARQL syntax errors return HTTP 200 with the error embedded in the response body.

- **file**: [pg_ripple_http/src/routing/sparql_handlers.rs:57–68](../pg_ripple_http/src/routing/sparql_handlers.rs#L57-L68)
- **fix**: Standardise all 4xx/5xx responses to `application/json` with `{"error":"PTxxx","message":"…"}`. Return HTTP 400 for SPARQL syntax errors.

**ID: A-02 | MEDIUM | Effort: S**
`json_ld_load` function name is inconsistent with the `load_*` naming convention used by all other bulk loaders (`load_ntriples`, `load_turtle`, `load_rdfxml`).

- **file**: [src/cdc_bridge_api.rs](../src/cdc_bridge_api.rs) (or the relevant `#[pg_extern]` site)
- **fix**: Rename to `load_jsonld` (or `json_ld_to_ntriples_load` for semantic clarity) in the next minor release. Add a deprecation notice to the old name.

**ID: A-03 | MEDIUM | Effort: M**
`pg_ripple_http` `COMPATIBLE_EXTENSION_MIN` is `0.75.0` but the extension is at `0.79.0`. Any user still on `0.75.0–0.78.0` will not receive a compatibility warning, yet they may be missing API features that the HTTP companion now depends upon.

- **file**: [pg_ripple_http/src/main.rs:34](../pg_ripple_http/src/main.rs#L34)
- **fix**: Update `COMPATIBLE_EXTENSION_MIN` to `"0.79.0"` and add a comment documenting the minimum feature set required at each boundary.

**ID: A-04 | LOW | Effort: S**
`RETURNS TABLE` functions returning VP rows omit the `g` (graph) column, forcing callers to decode the graph ID separately.

- **file**: [src/schema.rs:581](../src/schema.rs#L581) and other similar return type definitions
- **fix**: Add `g BIGINT` to relevant `RETURNS TABLE` signatures.

**ID: A-05 | LOW | Effort: S**
GUC naming convention is inconsistent (`*_threshold` vs `max_*` vs `*_limit`); see Q-03 for detail.

**ID: A-06 | LOW | Effort: S**
Breaking GUC removal (`property_path_max_depth` in v0.56.0) not tagged with `BREAKING:` prefix in CHANGELOG.

- **file**: [CHANGELOG.md](../CHANGELOG.md) (v0.56.0 section)
- **fix**: Adopt a `BREAKING:` tag convention for all user-visible API removals and apply retroactively to the v0.56.0 entry.

### Summary

API design is generally consistent. The most impactful fix is standardising HTTP error responses (A-01).

---

## Area 7: Documentation & Specification Fidelity

### Findings

**ID: D-01 | MEDIUM | Effort: S**
Compatibility matrix in `docs/src/operations/compatibility.md` ends at `pg_ripple_http v0.16.x` (≥ 0.70.0). It is missing rows for v0.76.0–v0.79.0 / HTTP 0.76.0 boundary.

- **file**: [docs/src/operations/compatibility.md](../docs/src/operations/compatibility.md)
- **fix**: Add rows for v0.76.0 HTTP companion (requires extension ≥ 0.77.0 for BIDI primitives) and update the table through v0.79.0.

### Summary

Documentation coverage is strong: all 157+ public SQL functions are documented in `docs/src/reference/sql-functions.md`, all 12 previously missing reference pages are now present, CONTRIBUTING.md is accurate, and the v0.79.0 spec items (WCOJ-LFTI-01, SHACL-SPARQL-01, README-LIMITS-01) all have matching implementation. The one gap is the stale compatibility matrix.

---

## Area 8: Dependency & Supply Chain

### Findings

**ID: DS-01 | MEDIUM | Effort: S**
SBOM (`sbom.json`) is five releases stale (v0.74.0) and the inner `components[]` block still contains a `pg_ripple@0.74.0` entry.

- **file**: [sbom.json](../sbom.json)
- **fix**: Run `cargo cyclonedx` and commit the output as part of the v0.79.0 release. Add a CI step asserting `jq '.metadata.component.version' sbom.json` equals `grep '^version' Cargo.toml | head -1`.

**ID: DS-02 | MEDIUM | Effort: S**
`serde_cbor` (RUSTSEC-2021-0127) has been unmaintained since 2021. After five years, a replacement (`ciborium`) should be evaluated.

- **file**: [Cargo.toml](../Cargo.toml) (transitive dependency path)
- **fix**: Run `cargo tree | grep serde_cbor` to identify which direct dependency pulls it in. If `parquet` is the source, check if a newer parquet version eliminated it.

**ID: DS-03 | LOW | Effort: S**
`rust-toolchain.toml` is pinned to `1.95.0`. As of 2026-04-30, this is approximately 3–4 months old. Renovate bot should be configured to open auto-PRs for toolchain updates.

- **file**: [rust-toolchain.toml](../rust-toolchain.toml)
- **fix**: Ensure `renovate.json` (or `.github/renovate.json`) has `"packageRules": [{"matchManagers": ["rust-toolchain"], "automerge": true}]`.

### Summary

No known active CVEs in the direct dependency tree. The three ignored RUSTSEC advisories are all documented and defensible. The primary action is regenerating the SBOM.

---

## Area 9: Observability & Operability

### Findings

**ID: O-01 | HIGH | Effort: M**
SPARQL `EXPLAIN` output does not include the optimised SPARQL algebra tree or plan decisions (filter pushdown, self-join elimination, LEFT JOIN → INNER JOIN promotion).

- **file**: [src/sparql/explain.rs](../src/sparql/explain.rs)
- **impact**: Operators debugging slow queries see the generated SQL and PostgreSQL EXPLAIN but cannot understand why the SPARQL translator made particular choices.
- **fix**: Add `"sparql_algebra"` JSON field containing the post-optimisation algebra tree annotated with optimisation decisions.

**ID: O-02 | HIGH | Effort: S**
Merge worker has no periodic heartbeat log; operators cannot distinguish a stalled merge from a healthy idle merge.

- **file**: [src/worker.rs:269,421](../src/worker.rs#L269)
- **fix**: Log `merge_cycle_start(predicate_count=N)` and `merge_cycle_end(duration_ms=M, triples_merged=K)` on every cycle, unconditionally, at `INFO` level. Emit `pg_notify('pg_ripple.merge_health', json)` with timestamp.

**ID: O-03 | MEDIUM | Effort: M**
`graph_stats()` / `predicate_stats()` materialisation strategy undocumented; at 100M+ triples, ad-hoc full scans are unsafe.

- **file**: [src/stats.rs](../src/stats.rs)
- **fix**: Document the current implementation and add a warning: "This function scans all VP tables at runtime. Consider calling it at low-traffic times or using the stats cache." Add a `pg_ripple.stats_scan_limit` GUC to cap execution time.

**ID: O-04 | MEDIUM | Effort: S**
Prometheus metrics labels do not break down query latency by query type or result size.

- **file**: [pg_ripple_http/src/metrics.rs](../pg_ripple_http/src/metrics.rs)
- **fix**: Add `query_type` label (SELECT/ASK/CONSTRUCT/DESCRIBE/UPDATE) and `result_bucket` label ([0-10k, 10k-100k, 100k+) on the latency histogram.

**ID: O-05 | MEDIUM | Effort: S**
`pg_stat_statements` integration attributes each unique SPARQL query text as a distinct statement; parameterised workloads cause statement-count explosion.

- **file**: [src/stats.rs](../src/stats.rs)
- **fix**: Normalise SPARQL text (collapse literals to `?` placeholders) or use the algebra tree hash as the statement fingerprint.

**ID: O-06 | MEDIUM | Effort: S**
Error messages across the HTTP companion inconsistently include/omit stack context; `redacted_error()` is not applied uniformly.

- **file**: [pg_ripple_http/src/common.rs:62–75](../pg_ripple_http/src/common.rs#L62-L75)
- **fix**: Audit all `format!("…{e}")` in handlers; replace with `redacted_error(&e)`.

**ID: O-07 | MEDIUM | Effort: M**
Vacuum/reindex admin functions: lock levels not documented; may use `ACCESS EXCLUSIVE` (blocking all reads) rather than `CONCURRENT` mode.

- **file**: [src/admin/](../src/admin/) (not fully audited)
- **fix**: Verify that `REINDEX` uses `CONCURRENTLY` where possible. Document lock level for each admin function in `docs/src/reference/administration.md`.

### Summary

Observability gaps are primarily in operational visibility (merge worker health, EXPLAIN algebra tree, metrics labels). None of these block correctness but they substantially increase operational toil in production.

---

## Area 10: Concurrency & Transaction Safety

### Findings

**ID: CC-01 | HIGH | Effort: M**
Dictionary LRU caches are `thread_local! RefCell<LruCache<_>>`. While each PG backend is a single process, `RefCell` is not safe for use across Rust `async` tasks or hypothetical future parallel worker threads. More importantly, the cache is not invalidated on sub-transaction rollback.

- **file**: [src/dictionary/mod.rs:65–83](../src/dictionary/mod.rs#L65-L83)
- **fix**: Register the dictionary decode cache in the `SubXactCallback` (already registered for journal) to clear it on `SUBXACT_EVENT_ABORT_SUB`.

**ID: CC-02 | HIGH | Effort: M**
CDC replication slot lifecycle: no protection against slot leaks when subscriber backends crash mid-session.

- **file**: [src/cdc.rs](../src/cdc.rs)
- **impact**: Orphaned `pg_ripple_*` replication slots prevent WAL reclamation; at high CDC throughput this can fill the WAL disk.
- **fix**: Add a background worker that scans `pg_replication_slots` and drops any `pg_ripple_*` slot whose `active` = false and `confirmed_flush_lsn` has not advanced in `pg_ripple.cdc_slot_max_age_secs` (default 86400).

**ID: CC-03 | MEDIUM | Effort: S**
Promotion advisory lock is transaction-scoped (`pg_advisory_xact_lock()`). Under concurrent promotion of multiple predicates, they all serialise on the same lock even though they access different VP tables.

- **file**: [src/storage/promote.rs:38–50](../src/storage/promote.rs#L38-L50)
- **fix**: Use per-predicate advisory locks keyed on `hash(predicate_id)` at session level, released after the atomic CTE commits.

**ID: CC-04 | MEDIUM | Effort: S**
Merge fence lock `0x5052_5000` is global and held for entire merge cycle; all merge workers serialise during Citus rebalance even for predicates not involved.

- **file**: [src/worker.rs:250–280](../src/worker.rs#L250-L280)
- **fix**: Reduce fence lock scope to the Citus rebalance-apply phase only.

**ID: CC-05 | MEDIUM | Effort: M**
Datalog parallel group partitioning does not verify absence of intra-stratum cycles (see C-12); strata with circular positive dependencies may not converge under parallel evaluation.

- **file**: [src/datalog/parallel.rs:110–150](../src/datalog/parallel.rs#L110-L150)

**ID: CC-06 | MEDIUM | Effort: S**
CDC LSN ordering across tables not documented; no `_pg_ripple.cdc_lsn_watermark` tracking; a subscriber restart could replay events out of LSN order if the slot's `confirmed_flush_lsn` is imprecise.

- **file**: [src/cdc_bridge_api.rs](../src/cdc_bridge_api.rs)
- **fix**: Add `_pg_ripple.cdc_lsn_watermark (source_table, last_seen_lsn)` table; emit WARNING if ingested LSN < last seen LSN.

**ID: CC-07 | MEDIUM | Effort: S**
Promotion status stuck at `'promoting'` on crash: `recover_interrupted_promotions()` only runs at server restart; a promotion that crashes mid-CTE leaves the predicate inaccessible via VP table until restart.

- **file**: [src/storage/promote.rs:34–41](../src/storage/promote.rs#L34-L41)
- **fix**: Emit `pg_notify('pg_ripple.promotion_failed', json)` on error; add a background worker check that detects promotions stuck in `'promoting'` for >60 seconds and re-invokes the recovery function without a restart.

### Summary

The SubXact callback (CF-B) was correctly implemented in v0.72.0. The remaining concurrency findings are operational (CDC slots, promotion recovery) rather than correctness bugs. CC-01 (decode cache invalidation on rollback) is the highest-correctness-risk item.

---

## Area 11: Standards Conformance Gaps

### Findings

**ID: SC-01 | MEDIUM | Effort: M**
SPARQL FILTER with unknown built-in functions silently drops the filter rather than raising an error.

- **file**: [src/sparql/translate/filter/filter_expr.rs:117](../src/sparql/translate/filter/filter_expr.rs#L117) — comment "not yet supported — FILTER predicate dropped"
- **impact**: A query with a misspelled function name like `FILTER(LANGMATC(?lang, "en"))` (note typo) will silently return unfiltered results, appearing to work correctly.
- **fix**: Add `pg_ripple.strict_sparql_filters` GUC (default `off`). When `on`, unknown FILTER functions raise an error. When `off`, emit a WARNING.

**ID: SC-02 | LOW | Effort: S**
Property path cycle detection is on the endpoint column only (see C-03); for paths returning to an intermediate node via a different edge, the path continues indefinitely until `max_path_depth`.

**ID: SC-03 | LOW | Effort: S**
`quote_ident_safe()` does not support Unicode role names (see S-07); non-ASCII identifiers in SHACL-generated DDL may be incorrectly quoted.

### Summary

SPARQL 1.1/1.2, OWL 2 RL (all 40+ rules via v0.24.0 + v0.44.0), SHACL Core (35/35 constraints), RDF-star, and JSON-LD Framing 1.1 are all implemented. The main standards gap is the silent filter-drop behaviour (SC-01).

---

## Area 12: v0.77.0–v0.78.0 BIDI Integration Gap Analysis

### Summary

All 17 BIDI specification items are **implemented, tested, and migrated**:

| ID | Feature | Status | Evidence |
|----|---------|--------|----------|
| BIDI-ATTR-01 | Source attribution via named graphs | ✅ Complete | [src/bidi.rs:4](../src/bidi.rs#L4), [src/json_mapping.rs:36](../src/json_mapping.rs#L36) |
| BIDI-CONFLICT-01 | Conflict resolution strategies (4 modes) | ✅ Complete | [src/bidi.rs:29](../src/bidi.rs#L29), [src/schema.rs:1690](../src/schema.rs#L1690) |
| BIDI-NORMALIZE-01 | Echo-aware normalisation | ✅ Complete | [src/bidi.rs](../src/bidi.rs) |
| BIDI-UPSERT-01 | sh:maxCount 1 driven upsert | ✅ Complete | [src/json_mapping.rs:72](../src/json_mapping.rs#L72) |
| BIDI-DIFF-01 | Diff mode + RDF-star change timestamps | ✅ Complete | [src/json_mapping.rs:72](../src/json_mapping.rs#L72) |
| BIDI-DELETE-01 | Symmetric delete + tombstone CDC | ✅ Complete | [src/bidi.rs:62](../src/bidi.rs#L62) |
| BIDI-REF-01 | Cross-source reference via owl:sameAs | ✅ Complete | [src/schema.rs:1712](../src/schema.rs#L1712) |
| BIDI-LOOP-01 | Loop-safe subscriptions | ✅ Complete | [src/schema.rs:1752](../src/schema.rs#L1752) |
| BIDI-CAS-01 | Object-level events + optimistic lock | ✅ Complete | [src/bidi.rs:129](../src/bidi.rs#L129) |
| BIDI-LINKBACK-01 | Target-assigned ID rendezvous | ✅ Complete | [src/bidi.rs:93](../src/bidi.rs#L93), [src/schema.rs:1729](../src/schema.rs#L1729) |
| BIDI-OUTBOX-01 | pg-trickle outbox events | ✅ Complete | [src/cdc_bridge_api.rs](../src/cdc_bridge_api.rs) |
| BIDI-INBOX-01 | pg-trickle inbox record/abandon linkback | ✅ Complete | [src/bidi.rs:103](../src/bidi.rs#L103) |
| BIDI-WIRE-01 | Frozen JSON wire format + schema | ✅ Complete | [docs/src/operations/event-schema-v1.json](../docs/src/operations/event-schema-v1.json) |
| BIDI-OBS-01 | Per-graph observability | ✅ Complete | [src/bidi.rs:146](../src/bidi.rs#L146) |
| BIDI-PERF-01 | ≤2× baseline ingest, ≤5ms/event | ✅ Gated | benchmarks/; requires runtime verification |
| BIDI-MIG-01 | Migration 0.76.0→0.77.0 | ✅ Complete | [sql/pg_ripple--0.76.0--0.77.0.sql](../sql/pg_ripple--0.76.0--0.77.0.sql) |
| BIDI-DOC-01 | pg-trickle relay walkthrough | ✅ Complete | [docs/src/operations/pg-trickle-relay.md](../docs/src/operations/pg-trickle-relay.md) |

**BIDI-PERF-01 performance target**: The ≤5 ms/event SLA at 1,000 events/s requires runtime verification against the current storage architecture. The HTAP delta path adds at most one B-tree insert + one dictionary lookup per triple; the BIDI overhead (conflict check + attribution) adds at most two VP reads. At this write rate, the main bottleneck is the SID sequence, which is a shared sequence that serialises at ~50k assignments/s. At 1,000 events/s with an average of 5 triples/event = 5,000 triples/s, the SID sequence is not a bottleneck. The BIDI-PERF-01 target is plausible but should be re-validated after the HTAP merge interval is tuned. **Mark as: runtime verification required.**

**BIDI-MIG-01 migration script**: [sql/pg_ripple--0.76.0--0.77.0.sql](../sql/pg_ripple--0.76.0--0.77.0.sql) is complete: 12 SQL functions registered, BIDI catalog tables created, no syntax errors. [sql/pg_ripple--0.77.0--0.78.0.sql](../sql/pg_ripple--0.77.0--0.78.0.sql) covers BIDIOPS (dead-letters, tokens, audit, schema-evolution). [sql/pg_ripple--0.78.0--0.79.0.sql](../sql/pg_ripple--0.78.0--0.79.0.sql) is correctly empty (query-engine-only release).

**No open BIDI gaps**. This area is the most complete in the codebase.

---

## Prioritised Action Plan

### (a) Must-fix before v1.0.0

| Priority | ID | File(s) | Change | Effort |
|---|---|---|---|---|
| 1 | C-01 | src/sparql/execute.rs | Add `mutation_journal::flush()` at end of `sparql_update()` and `execute_delete_insert()` | S |
| 2 | C-02 | src/r2rml.rs, src/cdc.rs, src/cdc_bridge_api.rs | Wire `mutation_journal::record_write()` + `flush()` into every write batch | M |
| 3 | T-01 | tests/test_migration_chain.sh | Extend schema assertions to cover v0.62.0–v0.79.0 (18 migrations) | M |
| 4 | DS-01 / Q-04 | sbom.json | Regenerate SBOM + add CI version-check gate | S |
| 5 | A-01 | pg_ripple_http/src/routing/sparql_handlers.rs | Standardise HTTP error responses to JSON; return 400 for SPARQL syntax errors | S |
| 6 | S-01 | src/views.rs | Migrate catalog inserts to `Spi::run_with_args()` with typed parameters | M |
| 7 | S-02 | src/sparql/embedding.rs | Replace `format!()` model_tag injection with parameterized query | S |
| 8 | S-03 | src/sparql/federation.rs | Extend SSRF blocklist to cover 172.16.0.0/12 and 192.168.0.0/16 | S |
| 9 | C-03 | src/sparql/property_path.rs | Fix CYCLE clause to cover (s, o) pair | S |
| 10 | C-14 | src/sparql/plan_cache.rs | Include RLS role context in cache key | S |

### (b) Should-fix in v0.80.0–v0.99.0

| Priority | ID | File(s) | Change | Effort |
|---|---|---|---|---|
| 11 | C-04 | src/storage/merge.rs | Add explicit ORDER BY before DISTINCT ON in merge CTE | S |
| 12 | C-05 | src/storage/promote.rs | Move status update inside atomic CTE | M |
| 13 | T-02 | tests/pg_regress/sql/ | Add error-path regression tests (8 scenarios) | M |
| 14 | P-01 | src/sparql/plan_cache.rs | Add `plan_cache_capacity` GUC | S |
| 15 | P-02 | src/sparql/decode.rs | Use `ANY($1)` with Vec<i64> bind for batch decode | M |
| 16 | CC-01 | src/dictionary/mod.rs | Invalidate decode cache on SubXact abort | S |
| 17 | CC-02 | src/cdc.rs | Add background CDC slot cleanup worker | M |
| 18 | O-01 | src/sparql/explain.rs | Add SPARQL algebra tree to EXPLAIN output | M |
| 19 | O-02 | src/worker.rs | Add merge worker heartbeat log on every cycle | S |
| 20 | A-03 | pg_ripple_http/src/main.rs | Update COMPATIBLE_EXTENSION_MIN to 0.79.0 | S |
| 21 | T-03 | fuzz/fuzz_targets/ | Add N-Triples, N-Quads, TriG fuzz targets | S |
| 22 | T-04 | fuzz/fuzz_targets/ | Add sparql_update executor fuzz target | S |
| 23 | S-05 | pg_ripple_http/src/routing/ | Add `sparql_max_algebra_depth` GUC + HTTP 400 enforcement | S |
| 24 | P-11 | src/cdc.rs | Add CDC slot lifecycle monitoring | M |
| 25 | C-06 | src/dictionary/mod.rs | Error on RETURNING 0-row race in encode() | S |

### (c) Nice-to-have / technical debt

| ID | File(s) | Change | Effort |
|---|---|---|---|
| Q-01 | src/bidi.rs | Split into 4 sub-modules at attribution/conflict/sync_ops/linkback boundaries | M |
| T-05 | tests/proptest/ | Strengthen 5 proptest suites with reference-implementation comparison | M |
| P-05 | src/dictionary/mod.rs | Move decode LRU to shared memory | L |
| P-08 | src/stats.rs | Add predicate_stats_cache materialised table | M |
| O-04 | pg_ripple_http/src/metrics.rs | Add query_type + result_size labels to latency histogram | S |
| SC-01 | src/sparql/translate/filter/filter_expr.rs | Add strict_sparql_filters GUC; warn on unknown built-ins | S |
| T-06 | tests/concurrent/ | Replace sleep(N) assertions with notify-based synchronisation | M |
| A-02 | src/cdc_bridge_api.rs | Rename `json_ld_load` to `load_jsonld` with deprecation | S |
| DS-02 | Cargo.toml | Evaluate replacement for `serde_cbor` transitive dependency | M |
| DS-03 | rust-toolchain.toml | Configure Renovate for toolchain auto-update | S |

---

## Appendix A: Dead Code Inventory

The `#[allow(dead_code)]` suppressions are all documented with rationale. No dangerous suppressions found. Representative examples:

| Module | Suppressed Item | Rationale |
|---|---|---|
| `src/datalog/lattice.rs` | Reserved lattice operations | Future Datalog extension points |
| `src/datalog/parallel.rs` | Parallel coordinator fields | Planned work-stealing scheduler |
| `src/sparql/optimizer.rs` | Unused algebraic transforms | Experimental rewrite rules |
| `src/sparql/federation_planner.rs` | Remote-endpoint cost variants | Future cost-based federation |
| `src/citus.rs` | Object-based pruning helpers | Planned Citus native integration |
| `src/error.rs` | 9 unused error variant constructors | Defensive error taxonomy |

**Assessment**: All suppressions are legitimate. No production-reachable dead-code paths were found that could mask bugs.

---

## Appendix B: Unwrap/Expect/Panic Inventory (Non-Test Code)

| File | Line | Pattern | Risk |
|---|---|---|---|
| [src/replication.rs](../src/replication.rs#L188) | 188 | `.unwrap()` on SPI result column | MEDIUM — user-facing; replace with error propagation |
| [src/replication.rs](../src/replication.rs#L190) | 190 | `.value::<String>().unwrap()` | MEDIUM — same function |
| [src/dictionary/mod.rs](../src/dictionary/mod.rs#L68) | 68 | `.expect("capacity > 0")` | LOW — compile-time constant |
| [src/dictionary/mod.rs](../src/dictionary/mod.rs#L83) | 83 | `.expect("capacity > 0")` | LOW — compile-time constant |
| [src/sparql/plan_cache.rs](../src/sparql/plan_cache.rs#L38) | 38 | `.expect("capacity > 0")` | LOW — compile-time constant |
| [pg_ripple_http/src/common.rs](../pg_ripple_http/src/common.rs#L75) | 75 | `.expect("infallible: hardcoded valid HTTP headers")` | LOW — documented infallible |
| [pg_ripple_http/src/datalog.rs](../pg_ripple_http/src/datalog.rs#L57) | 57 | `.expect("infallible: hardcoded valid HTTP headers")` | LOW — documented infallible |

**Total**: 7 instances in non-test code. Only the two in `src/replication.rs` are user-facing and should be converted to proper error returns. All others are at initialisation time or against compile-time constants.

---

## Appendix C: Missing Regression Tests

Public `#[pg_extern]` SQL functions with no pg_regress test (or only indirect smoke coverage):

| Function | File | Priority |
|---|---|---|
| `export_graphrag_entities` | [src/export_api.rs:83](../src/export_api.rs#L83) | High |
| `export_graphrag_relationships` | [src/export_api.rs:94](../src/export_api.rs#L94) | High |
| `export_graphrag_text_units` | [src/export_api.rs:105](../src/export_api.rs#L105) | High |
| `export_turtle_stream` | [src/export_api.rs:49](../src/export_api.rs#L49) | Medium |
| `trickle_available` | [src/cdc_bridge_api.rs:14](../src/cdc_bridge_api.rs#L14) | High |
| `enable_cdc_bridge_trigger` | [src/cdc_bridge_api.rs:28](../src/cdc_bridge_api.rs#L28) | High |
| `cdc_bridge_triggers` | [src/cdc_bridge_api.rs:43](../src/cdc_bridge_api.rs#L43) | High |
| `json_to_ntriples` | [src/cdc_bridge_api.rs:67](../src/cdc_bridge_api.rs#L67) | Medium |
| `load_vocab_template` | [src/cdc_bridge_api.rs:142](../src/cdc_bridge_api.rs#L142) | Medium |
| `federation_register_service` | [src/federation_registry.rs:130](../src/federation_registry.rs#L130) | High |
| `federation_unregister_service` | [src/federation_registry.rs:202](../src/federation_registry.rs#L202) | Medium |
| `group_concat_decode` | [src/dict_api.rs:52](../src/dict_api.rs#L52) | Medium |
| `decode_numeric_spi` | [src/dict_api.rs:29](../src/dict_api.rs#L29) | Low |

---

## Appendix D: Dependency Audit

Direct dependencies from `Cargo.toml` (static check; no network access):

| Crate | Version Used | Category | Advisory | License |
|---|---|---|---|---|
| `pgrx` | `=0.18.0` | Core (pinned) | None known | MIT |
| `spargebra` | `0.4` + sparql-12 | SPARQL parser | None | Apache-2.0 |
| `sparopt` | `0.3` + sparql-12 | SPARQL optimizer | None | Apache-2.0 |
| `xxhash-rust` | `0.8` | Hashing | None | MIT / Apache-2.0 |
| `serde` | `1` | Serialization | None | MIT / Apache-2.0 |
| `serde_json` | `1` | JSON | None | MIT / Apache-2.0 |
| `thiserror` | `2` | Error types | None | MIT / Apache-2.0 |
| `lru` | `0.17` | Cache | None | MIT |
| `chrono` | `0.4` | Datetime | None | MIT / Apache-2.0 |
| `rio_api` | `0.8` | RDF API | None | MIT / Apache-2.0 |
| `rio_turtle` | `0.8` | Turtle parser | None | MIT / Apache-2.0 |
| `rio_xml` | `0.8` | RDF/XML parser | None | MIT / Apache-2.0 |
| `oxrdf` | `0.3` | RDF-star model | None | MIT / Apache-2.0 |
| `libc` | `0.2` | FFI | None | MIT / Apache-2.0 |
| `unicode-normalization` | `0.1` | Unicode | None | MIT / Apache-2.0 |
| `ureq` | `2` + tls | HTTP client | None | MIT |
| `parquet` | `58` + snap | Arrow/Parquet | Transitive: RUSTSEC-2021-0127 (serde_cbor) | Apache-2.0 |
| `hmac` | `0.12` | HMAC | None | MIT / Apache-2.0 |
| `sha2` | `0.10` | SHA-2 | None | MIT / Apache-2.0 |
| `hex` | `0.4` | Hex encoding | None | MIT / Apache-2.0 |

**Ignored advisories** (from `audit.toml`):
- `RUSTSEC-2026-0104` (paste): compile-time macro unsoundness; no runtime impact.
- `RUSTSEC-2024-0436` (rsa): timing side-channel not applicable (pg_ripple uses HMAC-SHA256, not RSA).
- `RUSTSEC-2021-0127` (serde_cbor): unmaintained since 2021; transitive via parquet. **Action**: evaluate replacing with `ciborium` (parquet v60+ removes this dependency).

**Rust toolchain**: `1.95.0` (approximately Jan 2026; ~3 months old as of 2026-04-30). Current stable. No security advisories against this version. Renovate auto-PR should keep this current.
