-- pg_regress test: Datalog rules with GRAPH ?g variable in head and body.
--
-- Regression for: non-recursive compiler emitted literal g_var_g in INSERT SELECT
-- instead of binding ?g from body atoms (ERROR: column "g_var_g" does not exist).

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

SELECT pg_ripple.drop_graph('https://gvar.test/tenant/a');
SELECT pg_ripple.drop_rules('gvar_test');

SELECT pg_ripple.load_nquads(
    '<https://gvar.test/svc> <https://gvar.test/runs_on> <https://gvar.test/host> <https://gvar.test/tenant/a> .' || E'\n' ||
    '<https://gvar.test/host> <https://gvar.test/depends_on> <https://gvar.test/db> <https://gvar.test/tenant/a> .' || E'\n'
) = 2 AS base_triples_loaded;

SELECT pg_ripple.load_rules(
    'GRAPH ?g { ?s <https://gvar.test/indirectly_runs_on> ?i } :-
       GRAPH ?g { ?s <https://gvar.test/runs_on> ?h },
       GRAPH ?g { ?h <https://gvar.test/depends_on> ?i } .',
    'gvar_test'
) = 1 AS graph_var_rule_loaded;

SET pg_ripple.rule_graph_scope = 'all';
SELECT (pg_ripple.infer_with_stats('gvar_test')->>'derived')::int >= 1 AS inference_derived;

SELECT COUNT(*) = 1 AS derived_in_named_graph
FROM pg_ripple.sparql($$
    SELECT ?s ?i WHERE {
        GRAPH <https://gvar.test/tenant/a> {
            ?s <https://gvar.test/indirectly_runs_on> ?i .
            FILTER(?s = <https://gvar.test/svc> && ?i = <https://gvar.test/db>)
        }
    }
$$);

RESET pg_ripple.rule_graph_scope;
SELECT pg_ripple.drop_rules('gvar_test') >= 0 AS rules_cleaned;
SELECT pg_ripple.drop_graph('https://gvar.test/tenant/a') >= 0 AS graph_cleaned;