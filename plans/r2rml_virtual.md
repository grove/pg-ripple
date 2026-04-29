# R2RML Virtual Graph Layer — Tracking Item

**ID**: R2RML-VIRTUAL-01  
**Status**: Planned (post-v1.0.0)  
**Depends on**: v0.73.0 R2RML-DOC-01 (materialization scope clarified)  

---

## Problem statement

`pg_ripple.load_r2rml(mapping)` *materializes* triples: it executes the R2RML
mapping, generates N-Triples, and writes them to VP storage. The mapping is
evaluated once at call time; subsequent changes to the source relational tables
are not reflected until `load_r2rml()` is called again.

A **virtual R2RML graph layer** would evaluate the R2RML mapping at *query
time*, projecting a live view of the relational data as an RDF graph without
materializing any triples. This is similar to how R2RML processors like
Ontop or morph-KGC support virtual SPARQL endpoints over relational databases.

---

## Proposed design

1. **`pg_ripple.register_r2rml_virtual(name TEXT, mapping TEXT) RETURNS VOID`**:
   Stores the mapping text in a new `_pg_ripple.r2rml_virtual_mappings` catalog
   table. No triples are written.

2. **Query-time translation**: when a SPARQL query includes a named graph IRI
   that matches a registered virtual mapping (e.g.
   `GRAPH <urn:r2rml:my_mapping> { ... }`), the SPARQL → SQL translator
   generates a SQL query that evaluates the R2RML mapping inline as a CTE
   rather than scanning a VP table.

3. **Limitation**: Only simple R2RML patterns (single source table, direct
   object maps, IRI templates) are supported in the first iteration. Complex
   patterns (joins, referencing object maps) remain materialization-only.

---

## Deferred because

- The query-time translation layer changes are non-trivial (requires a new
  planner path in `src/sparql/plan.rs`).
- The set of R2RML patterns that can be safely translated to CTE-based
  virtual queries is a subset of full R2RML.
- For most production use cases, materialization provides better query
  performance; virtual graphs are mainly valuable for low-latency relational
  integration scenarios.

---

## Notes

`register_json_mapping` (v0.73.0 JSON-MAPPING-01) covers the simpler
JSON-to-RDF case where a CDC relay pattern is in use and a registered
bidirectional mapping is sufficient.  The virtual R2RML layer addresses the
general case of live relational-to-RDF projection for existing SQL tables.
