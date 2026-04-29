# CONSTRUCT Writeback Rules Reference

This page is the reference for pg_ripple's CONSTRUCT writeback rules (CWB).

## Overview

CONSTRUCT writeback rules allow SPARQL CONSTRUCT queries to act as
transformation pipelines: whenever triples are written to a source graph, the
rule re-runs the CONSTRUCT query and writes the derived triples to a target
graph. This enables live, incrementally-updated canonical views, raw-to-clean
ETL pipelines, and schema-mapping layers entirely within PostgreSQL.

## Status

```sql
SELECT feature_name, status FROM pg_ripple.feature_status()
WHERE feature_name LIKE 'construct%';
```

## SQL Functions

| Function | Description |
|---|---|
| `pg_ripple.register_construct_rule(name TEXT, construct_query TEXT, source_graphs TEXT[], target_graph TEXT) → void` | Register a CONSTRUCT writeback rule |
| `pg_ripple.drop_construct_rule(name TEXT) → void` | Remove a rule and its derived triples |
| `pg_ripple.list_construct_rules() → SETOF record` | List all registered rules |
| `pg_ripple.recompute_construct_rule(name TEXT) → BIGINT` | Trigger full recompute for a rule |

## How It Works

1. **Registration**: `register_construct_rule()` stores the rule in
   `_pg_ripple.construct_rules`. The CONSTRUCT query is parsed and validated.
   Source and target graphs are encoded as dictionary IDs.

2. **Trigger**: After any write to a source graph (via `mutation_journal::flush()`),
   `construct_rules::on_graph_write()` is called for each affected graph.

3. **Delta maintenance**: The engine re-executes the CONSTRUCT query on the
   affected source graph and computes the diff (new triples minus existing,
   deleted triples to retract) using Delete-Rederive (DRed).

4. **Writeback**: New triples are batch-inserted into the target graph.
   Retracted triples are deleted.

## Provenance

Each derived triple is tracked in `_pg_ripple.construct_rule_triples` with
a reference to the rule that generated it. This enables precise retraction
when a rule is updated or dropped.

## Pipeline Stratification

Rules are topologically sorted by their source/target graph dependencies.
Cycles are detected and rejected at registration time. Rules fire in
dependency order after each write.

## Performance Notes

- Per-statement deferral: the mutation journal accumulates writes during a
  statement and flushes once at statement end, so N inserts in a single
  statement fire CWB rules exactly once, not N times.
- CONSTRUCT queries are compiled to SQL and cached in the plan cache.

## Related Pages

- [SPARQL CONSTRUCT writeback guide](../../user-guide/construct-views.md)
- [Architecture Internals](architecture.md)
- [Feature Status Taxonomy](feature-status-taxonomy.md)
