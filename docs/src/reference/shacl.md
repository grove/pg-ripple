# SHACL Reference

This page is the reference for pg_ripple's SHACL (Shapes Constraint Language) engine.

## Overview

pg_ripple implements the SHACL Core and SHACL-SPARQL constraint sets. Shape
definitions are loaded from the triple store using `pg_ripple.load_shacl()`.
Constraints are compiled to DDL CHECK constraints (for synchronous validation)
and to an asynchronous validation pipeline backed by a background worker for
complex shape hierarchies.

## Status

```sql
SELECT feature_name, status FROM pg_ripple.feature_status()
WHERE feature_name LIKE 'shacl%';
```

## SQL Functions

| Function | Description |
|---|---|
| `pg_ripple.load_shacl(graph_iri TEXT) → void` | Load SHACL shapes from a named graph |
| `pg_ripple.validate_shacl(graph_iri TEXT) → SETOF record` | Validate a named graph against loaded shapes |
| `pg_ripple.list_shapes() → SETOF record` | List all loaded SHACL shapes |
| `pg_ripple.drop_shape(shape_iri TEXT) → void` | Remove a shape and its constraints |

## SHACL Core Constraint Coverage

All 35 SHACL Core constraint components are implemented:

- **Property constraints**: `sh:minCount`, `sh:maxCount`, `sh:minLength`, `sh:maxLength`, `sh:pattern`, `sh:languageIn`, `sh:uniqueLang`, `sh:equals`, `sh:disjoint`, `sh:lessThan`, `sh:lessThanOrEquals`
- **Value constraints**: `sh:in`, `sh:hasValue`, `sh:class`, `sh:datatype`, `sh:nodeKind`
- **Shape constraints**: `sh:node`, `sh:property`, `sh:qualifiedValueShape`, `sh:qualifiedMinCount`, `sh:qualifiedMaxCount`
- **Logical constraints**: `sh:and`, `sh:or`, `sh:not`, `sh:xone`
- **Closed shapes**: `sh:closed`, `sh:ignoredProperties`

## SHACL-SPARQL Constraints

Custom constraint components using SPARQL SELECT and ASK are supported via
`sh:sparql` on shape nodes. The constraint query is compiled to a pg_trickle
stream table for continuous validation.

## SHACL-AF Rule Execution

`sh:rule` triple rules (SHACL Advanced Features) are supported and compiled
to CONSTRUCT writeback rules, allowing shapes to derive new triples from
existing data.

## Performance Notes

- `sh:maxCount 1` hints are propagated to the SPARQL planner: DISTINCT is omitted.
- `sh:minCount 1` hints allow LEFT JOINs to be rewritten as INNER JOINs.
- Shape hints are cached in `_pg_ripple.shape_hints` and consulted at plan time.

## Related Pages

- [SHACL SQL Reference](../user-guide/sql-reference/shacl.md)
- [SHACL Data Quality Guide](../../user-guide/shacl.md)
- [Feature Status Taxonomy](feature-status-taxonomy.md)
