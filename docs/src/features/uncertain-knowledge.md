# Uncertain Knowledge: Probabilistic Datalog, Fuzzy SPARQL, and Soft SHACL

pg_ripple v0.87.0 introduces the **Uncertain Knowledge Engine** — a suite of features for reasoning with imprecise, probabilistic, or fuzzy data.

---

## Overview

Real-world knowledge graphs contain data of varying reliability. A fact extracted from a scientific paper has a different confidence level than one scraped from social media. pg_ripple now lets you:

1. **Annotate Datalog rules with probability weights** (`@weight`) so that inferred facts carry a confidence score.
2. **Query confidence with SPARQL** using the `pg:confidence()` extension function.
3. **Traverse graphs with a fuzzy similarity filter** using `pg:confPath()`.
4. **Match strings fuzzily** with `pg:fuzzy_match()` and `pg:token_set_ratio()`.
5. **Score SHACL validation** results with per-shape severity weights.
6. **Export Turtle with RDF\* confidence annotations**.

---

## Probabilistic Datalog (`@weight`)

Add a `@weight(0.8)` annotation to any Datalog rule to declare that derived facts have at most 0.8 confidence. The engine propagates confidence using **noisy-OR combination**:

```
parent(X, Y) :- father(X, Y).           @weight(1.0)
parent(X, Y) :- mother(X, Y).           @weight(1.0)
ancestor(X, Z) :- parent(X, Z).         @weight(0.9)
ancestor(X, Z) :- parent(X, Y), ancestor(Y, Z). @weight(0.85)
```

Enable with:

```sql
SET pg_ripple.probabilistic_datalog = on;
```

Confidence scores are stored in `_pg_ripple.confidence`:

```sql
SELECT statement_id, confidence, model
FROM   _pg_ripple.confidence
LIMIT  10;
```

### GUCs for probabilistic evaluation

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.probabilistic_datalog` | `off` | Enable `@weight` rule processing |
| `pg_ripple.prob_datalog_cyclic` | `off` | Allow approximate evaluation on cyclic rule sets |
| `pg_ripple.prob_datalog_max_iterations` | `100` | Maximum iterations for cyclic evaluation |
| `pg_ripple.prob_datalog_convergence_delta` | `0.001` | Early-exit threshold |
| `pg_ripple.prob_datalog_cyclic_strict` | `off` | Promote non-convergence from WARNING to ERROR |

---

## `pg:confidence()` — SPARQL confidence lookup

```sparql
PREFIX pg: <http://pg-ripple.org/functions/>

SELECT ?s ?p ?o ?conf WHERE {
  ?s ?p ?o .
  BIND(pg:confidence(?s, ?p, ?o) AS ?conf)
  FILTER(?conf > 0.7)
}
```

Returns the maximum confidence across all models for the triple `(?s, ?p, ?o)`. Returns `1.0` when no confidence row exists.

---

## Fuzzy SPARQL functions

These require `CREATE EXTENSION IF NOT EXISTS pg_trgm;`.

### `pg:fuzzy_match(a, b)`

Returns the trigram similarity (`similarity()`) between two string literals.

```sparql
PREFIX pg: <http://pg-ripple.org/functions/>

SELECT ?label WHERE {
  ?entity rdfs:label ?label .
  FILTER(pg:fuzzy_match(?label, "Alice Smith") > 0.6)
}
```

### `pg:token_set_ratio(a, b)`

Returns word-set similarity (`word_similarity()`) — better for substring matches.

```sparql
FILTER(pg:token_set_ratio(?label, "Smith") > 0.5)
```

### `pg:confPath(predicate, threshold)` — confidence property path

```sparql
SELECT ?x WHERE {
  <http://example.org/alice> <pg:conf_path/http://example.org/knows/0.8> ?x
}
```

Traverses the `knows` predicate with a minimum confidence threshold of 0.8.

### GUC

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.default_fuzzy_threshold` | `0.7` | Default threshold when not explicit |

---

## Soft SHACL scoring

Instead of a pass/fail validation, pg_ripple can compute a **weighted data-quality score** for a graph:

```sql
SELECT pg_ripple.shacl_score('http://example.org/data');
-- Returns a float8 in [0.0, 1.0], where 1.0 = fully compliant
```

Annotate shapes with `sh:severityWeight` to control their contribution:

```turtle
ex:MyShape a sh:NodeShape ;
    sh:targetClass ex:Person ;
    sh:property [ sh:path ex:name ; sh:minCount 1 ] ;
    sh:severityWeight "2.0"^^xsd:decimal .
```

Score history is logged to `_pg_ripple.shacl_score_log`:

```sql
SELECT pg_ripple.log_shacl_score('http://example.org/data');
SELECT * FROM _pg_ripple.shacl_score_log ORDER BY measured_at DESC;
```

---

## Loading triples with confidence

```sql
SELECT pg_ripple.load_triples_with_confidence(
    '<http://example.org/alice> <http://example.org/knows> <http://example.org/bob> .',
    confidence => 0.85,
    format => 'ntriples'
);
```

---

## Exporting with confidence annotations (RDF\*)

```sql
SET pg_ripple.export_confidence = on;
SELECT pg_ripple.export_turtle_with_confidence('http://example.org/data');
```

Returns Turtle with `<< s p o >> pg:confidence "0.85"^^xsd:float .` annotations.

---

## PROV-O confidence propagation

Set `pg_ripple.prov_confidence = on` to enable automatic confidence propagation from `pg:sourceTrust` predicates — triples derived from low-trust sources inherit lower confidence.

---

## HTTP API (pg_ripple_http)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/confidence/load` | Load triples with confidence |
| `GET` | `/confidence/shacl-score?graph=<IRI>` | Compute SHACL quality score |
| `GET` | `/confidence/shacl-report?graph=<IRI>` | Scored violation report |
| `POST` | `/confidence/vacuum` | Purge orphaned confidence rows |

---

## Garbage collection

Orphaned confidence rows (whose `statement_id` no longer exists in any VP table) are purged:

1. **Automatically** during each HTAP merge cycle.
2. **On demand**: `SELECT pg_ripple.vacuum_confidence();`
