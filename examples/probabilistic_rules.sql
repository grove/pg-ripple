-- Example: Probabilistic Datalog Rules (v0.57.0)
-- Demonstrates the @weight annotation for soft rules (Markov-Logic style).
--
-- NOTE: This feature is a preview in v0.57.0. The GUC
--   pg_ripple.probabilistic_datalog = on
-- must be set to enable weight-aware evaluation. When off, rules are treated
-- as standard Datalog rules (weight ignored).

SET search_path TO pg_ripple, public;
SET pg_ripple.probabilistic_datalog = on;

-- Example: soft subclass relationships with confidence weights.
-- A rule: if ?x rdf:type :Employee then ?x rdf:type :Person with weight 0.95.
SELECT pg_ripple.add_rule(
    $rule$
    -- @weight(0.95)
    ?x rdf:type :Person :-
        ?x rdf:type :Employee .
    $rule$
);

-- Another soft rule: co-worker inference with lower confidence.
SELECT pg_ripple.add_rule(
    $rule$
    -- @weight(0.7)
    ?x :knows ?y :-
        ?x :worksFor ?org ,
        ?y :worksFor ?org ,
        FILTER(?x != ?y) .
    $rule$
);

-- Run inference (confidence values propagated through derivation chains).
SELECT pg_ripple.infer();

-- Query inferred facts (confidence column available when probabilistic mode is on).
SELECT subject_iri, predicate_iri, object_iri
FROM pg_ripple.triples()
WHERE predicate_iri = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type'
  AND object_iri = 'http://example.org/Person'
LIMIT 10;

RESET pg_ripple.probabilistic_datalog;
