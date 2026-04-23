-- Migration 0.47.0 → 0.48.0
--
-- Theme: SHACL Core Completeness, OWL 2 RL Closure & SPARQL Completeness
--
-- New capabilities (pure Rust engine changes; no SQL schema changes required):
--
-- SHACL Core:
--   • sh:minLength / sh:maxLength — string-length bounds on literal values
--   • sh:xone — exactly-one-of (XOR) logic over sub-shapes
--   • sh:minExclusive / sh:maxExclusive / sh:minInclusive / sh:maxInclusive —
--     XSD-typed numeric range constraints
--   • Complex sh:path expressions: sh:inversePath, sh:alternativePath, sequence
--     paths, sh:zeroOrMorePath, sh:oneOrMorePath, sh:zeroOrOnePath (compiled to
--     WITH RECURSIVE … CYCLE CTEs)
--   • Violation struct extended with sh_value and sh_source_constraint_component
--     fields for W3C-conformant violation reports
--
-- OWL 2 RL:
--   • cax-sco: full rdfs:subClassOf transitive closure
--   • prp-spo1: rdfs:subPropertyOf full chain
--   • prp-ifp: inverse-functional-property derived owl:sameAs propagation
--   • cls-avf: chained owl:allValuesFrom + subclass hierarchy
--   • owl:minCardinality / owl:maxCardinality / owl:cardinality entailment rules
--
-- SPARQL Update:
--   • ADD <source> TO <target>
--   • COPY <source> TO <target>
--   • MOVE <source> TO <target>
--
-- SPARQL-star:
--   • Variable-inside-quoted-triple patterns (e.g. << ?s ?p ?o >> :assertedBy ?who)
--     now emit proper dictionary joins instead of silent FALSE → rows returned
--
-- Operational hardening:
--   • pg_ripple.federation_max_response_bytes GUC (default 100 MiB)
--   • pg_ripple.insert_triples(TEXT[][]) SRF for batch single-triple inserts

-- No DDL changes required for this migration.

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.48.0', '0.47.0')
ON CONFLICT DO NOTHING;
