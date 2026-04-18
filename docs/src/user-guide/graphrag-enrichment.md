# GraphRAG Datalog Enrichment

pg_ripple's Datalog engine can derive additional relationships from GraphRAG triples.
Enrichment rules are stored in rule sets and materialized with `infer()`.

## Bundled enrichment rules

The file `sql/graphrag_enrichment_rules.pl` ships with pg_ripple and defines
inferred relationship types:

| Derived property | Meaning |
|---|---|
| `gr:coworker` | Two entities both have relationships targeting the same org |
| `gr:collaborates` | Two entities appear in the same text unit |
| `gr:indirectReport` | Transitive closure of `gr:manages` |
| `gr:relatedOrg` | Two organizations share an entity bridge |

## Loading rules

```sql
-- Load from the shipped file (adjust path as needed)
SELECT pg_ripple.load_rules(
    pg_read_file('/path/to/graphrag_enrichment_rules.pl'),
    'graphrag_enrichment'
);

-- Or load inline
SELECT pg_ripple.load_rules(
    $RULES$
?a <https://graphrag.org/ns/coworker> ?b :-
    ?rel1 <https://graphrag.org/ns/source> ?a ,
    ?rel2 <https://graphrag.org/ns/source> ?b ,
    ?rel1 <https://graphrag.org/ns/target> ?org ,
    ?rel2 <https://graphrag.org/ns/target> ?org .
$RULES$,
    'graphrag_enrichment'
);
```

## Running inference

```sql
-- Materialize all graphrag_enrichment rules
SELECT pg_ripple.infer('graphrag_enrichment') AS triples_derived;
```

## Querying derived triples

```sql
-- Find all coworkers of alice
SELECT * FROM pg_ripple.sparql(
    'SELECT ?b WHERE { <https://example.org/entities/alice> <https://graphrag.org/ns/coworker> ?b }'
);

-- Find transitive reports
SELECT * FROM pg_ripple.sparql(
    'SELECT ?report WHERE { <https://example.org/entities/ceo> <https://graphrag.org/ns/indirectReport> ?report }'
);
```

## Lifecycle management

```sql
-- List active rule sets
SELECT * FROM pg_ripple.list_rules();

-- Disable without deleting
SELECT pg_ripple.disable_rule_set('graphrag_enrichment');

-- Re-enable
SELECT pg_ripple.enable_rule_set('graphrag_enrichment');

-- Drop when no longer needed
SELECT pg_ripple.drop_rules('graphrag_enrichment');
```
