# SPARQL 1.2 Tracking — pg_ripple Design Document

**ID**: SPARQL12-01  
**Version**: v0.73.0  
**Status**: Tracking — post-v1.0.0 unless `spargebra` ships 1.2 support before v1.0.0  

---

## Overview

SPARQL 1.2 is currently in W3C Working Group review. This document tracks the
key changes in the SPARQL 1.2 draft, their compatibility impact on pg_ripple,
and the migration path once the spec is finalised.

---

## Key changes in SPARQL 1.2 vs 1.1

### 1. RDF-star integration (RDF 1.2 alignment)

SPARQL 1.1 does not standardise RDF-star syntax.  SPARQL 1.2 formalises
triple-pattern quotation syntax (`<< s p o >>`) and introduces:

- `BIND` expressions over quoted triples.
- `GRAPH` pattern matching against quoted-triple subjects.
- `TRIPLE()` function: constructs a quoted-triple term at query time.
- `SUBJECT()`, `PREDICATE()`, `OBJECT()` functions: destructure a quoted-triple term.

**pg_ripple impact**: pg_ripple already stores RDF-star via the `qt_s/qt_p/qt_o`
dictionary columns (v0.4.0) and supports `<< s p o >>` in N-Triples-star and
Turtle-star ingest (v0.4.0+). The SPARQL translator generates CTE-based
quoted-triple joins. Full `TRIPLE()` / `SUBJECT()` / `PREDICATE()` / `OBJECT()`
built-in support is the primary gap. This is a targeted addition — no
architectural change required.

### 2. `LATERAL` joins

SPARQL 1.2 introduces `LATERAL { }` to allow inner graph patterns to reference
variables bound in the outer scope — analogous to SQL's `LATERAL` join.

**pg_ripple impact**: The SPARQL → SQL translator generates standard SQL
`LATERAL` joins for subqueries that reference outer variables. Adding `LATERAL`
syntax support requires grammar changes in `spargebra` (the upstream parser crate
used by pg_ripple). Once `spargebra` ships a SPARQL 1.2 grammar, the translation
layer needs no changes — the `LATERAL` subquery translation already exists for
subqueries.

### 3. Updated `VALUES` syntax

SPARQL 1.2 generalises `VALUES` to allow inline result sequences directly inside
`SELECT` and removes some syntactic restrictions on inline `VALUES` placement.

**pg_ripple impact**: Minor. The translator already handles `VALUES` inline tables
via the `spargebra` Values algebra node. Grammar-level changes land in the parser
crate; the translation layer is unaffected.

### 4. Additional built-in functions

SPARQL 1.2 draft adds:

- `xsd:dateTimeStamp` coercion helpers.
- `math:log`, `math:pow`, `math:sqrt` (aligned with XQuery 3.1 math functions).
- `sfn:` Spatial Functions Note functions (if included in the final spec).

**pg_ripple impact**: pg_ripple maps built-in SPARQL functions to PostgreSQL
equivalents in `src/sparql/translate/filters.rs`.  The math functions map
directly to `ln()`, `power()`, `sqrt()` in PostgreSQL. Adding them requires
minor additions to the filter translator.

---

## Compatibility assessment

| Change | Backward compatible? | pg_ripple work required |
|---|---|---|
| RDF-star `TRIPLE()` / `SUBJECT()` / `OBJECT()` / `PREDICATE()` | Yes (new functions) | Add to filter translator |
| `LATERAL {}` | Yes (new syntax) | `spargebra` grammar update; translator unchanged |
| `VALUES` placement changes | Yes (relaxed grammar) | `spargebra` grammar update |
| Math built-in functions | Yes (new functions) | Minor filter translator additions |

No breaking changes are expected. All SPARQL 1.2 features are additive.

---

## Migration path

1. **Wait for `spargebra` 1.2 support**: The primary dependency is the upstream
   `spargebra` crate. Track the crate's issue tracker and upgrade when a SPARQL 1.2
   grammar is available.

2. **Update grammar token handling** in `src/sparql/parse.rs` if new tokens
   (e.g., `LATERAL`) require additional complexity checks.

3. **Add new built-in functions** to `src/sparql/translate/filters.rs`:
   - `TRIPLE()` → `(SELECT id FROM _pg_ripple.dictionary WHERE qt_s=$s AND qt_p=$p AND qt_o=$o)`
   - `SUBJECT()` / `PREDICATE()` / `OBJECT()` → dictionary join on `qt_s/qt_p/qt_o`
   - Math functions → direct PostgreSQL equivalents

4. **Run the W3C SPARQL 1.2 test suite**: When published by W3C, integrate it
   into the existing `tests/pg_regress/sql/w3c_sparql_query_conformance.sql`
   pipeline.

---

## Tentative timeline

SPARQL 1.2 is in W3C Working Group last call as of April 2026. The spec is
expected to reach Candidate Recommendation in late 2026. pg_ripple will track it
as a **post-v1.0.0** feature unless `spargebra` ships SPARQL 1.2 support before
the v1.0.0 release.

---

## References

- W3C SPARQL 1.2 Working Group: <https://www.w3.org/groups/wg/rdf-star>
- `spargebra` crate: <https://crates.io/crates/spargebra>
- RDF 1.2 (RDF-star formalisation): <https://www.w3.org/TR/rdf12-concepts/>

---

## Gap Analysis vs. W3C SPARQL 1.2 Draft (D13-03, v0.86.0)

Last reviewed: 2026-05-02. Community draft source: <https://w3c.github.io/sparql-query/>

| SPARQL 1.2 Feature | pg_ripple Status | Notes |
|---|---|---|
| `<< s p o >>` quoted triple syntax in BGPs | ✅ Implemented (v0.4.0) | Stored via `qt_s/qt_p/qt_o` dictionary columns |
| `TRIPLE(s, p, o)` constructor | ⚠ Partial | Not yet translated to SQL; spargebra 0.4 does not expose the function call |
| `SUBJECT()` / `PREDICATE()` / `OBJECT()` | ⚠ Partial | Dictionary join required; not yet plumbed through SQL generator |
| `REIF` keyword (annotation syntax) | ❌ Not started | spargebra 0.4 does not support REIF; depends on spargebra update |
| `LATERAL` in FROM clause | ❌ Not started | spargebra grammar update required |
| New math functions (`op:numeric-integer-divide`, etc.) | ❌ Not started | Low priority; PostgreSQL CAST suffices |
| Revised aggregate semantics (distinct in aggregate body) | ❌ Not started | Impact on Datalog aggregate push-down TBD |
| `ADJUST()` / `TZ()` date functions | ❌ Not started | PostgreSQL `AT TIME ZONE` is equivalent |

**Overall**: pg_ripple's RDF-star foundation (v0.4.0) covers the most impactful SPARQL 1.2 change.
Remaining gaps depend on `spargebra` adopting SPARQL 1.2 grammar; tracked as post-v1.0.0 work.

