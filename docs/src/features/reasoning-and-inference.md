# §2.5 Reasoning and Inference

## What and Why

Inference lets pg_ripple derive **new facts** from existing data using logical rules.
If Alice works at MIT and MIT is located in Massachusetts, inference can conclude that
Alice is located in Massachusetts — without anyone explicitly inserting that triple.

pg_ripple ships a full Datalog reasoning engine that supports:

- **Built-in rule sets**: RDFS and OWL RL entailment out of the box.
- **Custom rules**: domain-specific inference in a Turtle-flavoured Datalog syntax.
- **Stratified negation**: "flag people without an email address."
- **Aggregation**: COUNT, SUM, MIN, MAX, AVG over grouped triple patterns.
- **Magic sets**: goal-directed inference that only materialises relevant facts.
- **Semi-naive evaluation**: efficient fixpoint iteration that skips unchanged rows.
- **Well-Founded Semantics**: handle programs with cyclic negation (v0.32.0).

Derived triples are stored with `source = 1` (inferred) alongside explicit triples
(`source = 0`), so you can always distinguish asserted from derived facts.

---

## How It Works

### The Datalog Pipeline

1. **Parse** — rules are parsed from a Turtle-flavoured Datalog syntax into an internal Rule IR.
2. **Stratify** — the dependency graph is analyzed; rules are grouped into strata such that negated predicates are fully computed in lower strata.
3. **Compile** — each stratum is compiled to PostgreSQL SQL: non-recursive rules become `INSERT ... SELECT`, recursive rules become `WITH RECURSIVE ... CYCLE`.
4. **Execute** — strata run bottom-up; each stratum's SQL is executed via SPI, inserting derived triples into VP delta tables.
5. **Fixpoint** — recursive strata iterate until no new facts are derived (semi-naive evaluation).

### Rule Syntax

Rules use a Prolog-like notation with RDF terms. The prefix registry from `register_prefix()` is available:

```
head_triple :- body_triple1 , body_triple2 .
```

Variables start with `?`. Constants are IRIs (prefixed or full). Negation uses `NOT`.

### Built-in Rule Sets

| Name | Rules | What it covers |
|---|---|---|
| `rdfs` | ~12 rules | `rdfs:subClassOf` transitivity, `rdfs:subPropertyOf` transitivity, `rdf:type` propagation via subclass/subproperty, `rdfs:domain`/`rdfs:range` inference |
| `owl-rl` | ~80 rules | OWL RL profile: symmetric/transitive/inverse properties, `owl:equivalentClass`, `owl:sameAs`, `owl:unionOf`, `owl:intersectionOf`, property chains, and more |

---

## Worked Examples

### Loading Built-in RDFS Rules

```sql
-- Load the RDFS entailment rules
SELECT pg_ripple.load_rules_builtin('rdfs');
-- Returns: 12 (number of rules)

-- Load some class hierarchy data
SELECT pg_ripple.load_turtle('
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix bibo: <http://purl.org/ontology/bibo/> .
@prefix ex:   <https://example.org/> .

bibo:AcademicArticle rdfs:subClassOf bibo:Article .
bibo:Article rdfs:subClassOf bibo:Document .
bibo:Document rdfs:subClassOf rdfs:Resource .

ex:paper/42 rdf:type bibo:AcademicArticle .
');

-- Run inference
SELECT pg_ripple.infer('rdfs');

-- Now ex:paper/42 is also a bibo:Article, bibo:Document, and rdfs:Resource
SELECT * FROM pg_ripple.sparql('
PREFIX rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX bibo: <http://purl.org/ontology/bibo/>

SELECT ?type
WHERE {
    <https://example.org/paper/42> rdf:type ?type .
}
');
```

### Loading OWL RL Rules

```sql
-- Load the OWL RL entailment rules
SELECT pg_ripple.load_rules_builtin('owl-rl');

-- Load ontology with OWL constructs
SELECT pg_ripple.load_turtle('
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix ex:   <https://example.org/> .

ex:cites owl:inverseOf ex:citedBy .
ex:collaboratesWith a owl:SymmetricProperty .
ex:influencedBy a owl:TransitiveProperty .

ex:paper/42 ex:cites ex:paper/99 .
ex:person/alice ex:collaboratesWith ex:person/bob .
ex:person/carol ex:influencedBy ex:person/alice .
ex:person/alice ex:influencedBy ex:person/dave .
');

-- Run OWL RL inference
SELECT pg_ripple.infer('owl-rl');

-- Derived: ex:paper/99 ex:citedBy ex:paper/42  (inverse)
-- Derived: ex:person/bob ex:collaboratesWith ex:person/alice  (symmetric)
-- Derived: ex:person/carol ex:influencedBy ex:person/dave  (transitive)
```

### Writing Custom Rules

Define domain-specific rules for the bibliographic dataset:

```sql
SELECT pg_ripple.load_rules('
# Derive co-authorship: two people who authored the same paper
?a ex:coAuthor ?b :- ?paper dct:creator ?a , ?paper dct:creator ?b .

# Derive institutional collaboration
?inst1 ex:collaboratesWith ?inst2 :-
    ?paper dct:creator ?a ,
    ?paper dct:creator ?b ,
    ?a schema:affiliation ?inst1 ,
    ?b schema:affiliation ?inst2 .

# Derive prolific author (authored 5+ papers)
# Uses arithmetic guard: at least 5 papers
?author ex:isProlific "true"^^xsd:boolean :-
    ?paper1 dct:creator ?author ,
    ?paper2 dct:creator ?author ,
    ?paper3 dct:creator ?author ,
    ?paper4 dct:creator ?author ,
    ?paper5 dct:creator ?author .
', 'biblio');

-- Run the custom rule set
SELECT pg_ripple.infer('biblio');
```

### Negation-as-Failure

Flag entities that are missing expected properties:

```sql
SELECT pg_ripple.load_rules('
# Flag papers without a date
?paper ex:missingDate "true"^^xsd:boolean :-
    ?paper rdf:type bibo:AcademicArticle ,
    NOT ?paper dct:date ?_ .

# Flag people without an affiliation
?person ex:missingAffiliation "true"^^xsd:boolean :-
    ?person rdf:type foaf:Person ,
    NOT ?person schema:affiliation ?_ .
', 'quality');

SELECT pg_ripple.infer('quality');

-- Query the derived quality flags
SELECT * FROM pg_ripple.sparql('
PREFIX ex: <https://example.org/>
SELECT ?paper WHERE { ?paper ex:missingDate "true"^^<http://www.w3.org/2001/XMLSchema#boolean> }
');
```

### Named Graph Scoping

Write derived triples into a separate graph:

```sql
SELECT pg_ripple.load_rules('
# All RDFS inference goes into the "inferred" graph
GRAPH ex:graph/inferred { ?x rdf:type ?c } :-
    ?x rdf:type ?b , ?b rdfs:subClassOf ?c .
', 'scoped-rdfs');

SELECT pg_ripple.infer('scoped-rdfs');

-- Query only inferred types
SELECT * FROM pg_ripple.sparql('
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
SELECT ?x ?type WHERE {
    GRAPH <https://example.org/graph/inferred> {
        ?x rdf:type ?type .
    }
}
');
```

### Semi-Naive Evaluation with Statistics

Get detailed inference statistics:

```sql
SELECT pg_ripple.infer_with_stats('rdfs');
```

Returns JSONB:

```json
{
  "derived": 156,
  "iterations": 4,
  "eliminated_rules": [
    "?x rdf:type rdfs:Resource :- ?x ?p ?o ."
  ]
}
```

The `eliminated_rules` field shows rules removed by subsumption checking — rules
whose body is a superset of another rule's body.

### Goal-Directed Inference with Magic Sets

When you only need a subset of derived facts, magic sets avoids materialising everything:

```sql
-- Only derive facts relevant to: "What types does paper/42 have?"
SELECT pg_ripple.infer_goal('rdfs', '?x rdf:type <http://xmlns.com/foaf/0.1/Person>');
```

Returns JSONB:

```json
{
  "derived": 12,
  "iterations": 3,
  "matching": 5
}
```

Compare with full inference:

```sql
-- Full materialization: derives ALL facts
SELECT pg_ripple.infer('rdfs');
-- derived: 156

-- Goal-directed: derives only what's needed for the goal
SELECT pg_ripple.infer_goal('rdfs', '?x rdf:type foaf:Person');
-- derived: 12 (much fewer)
```

```admonish tip
Magic sets are controlled by the GUC `pg_ripple.magic_sets`. When set to `false`,
`infer_goal()` falls back to full materialization and filters post-hoc.
```

### Demand-Filtered Inference

For multiple goals at once, use demand-filtered inference:

```sql
SELECT pg_ripple.infer_demand('rdfs', '[
    {"p": "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>"},
    {"s": "<https://example.org/paper/42>"}
]'::jsonb);
```

Returns:

```json
{
  "derived": 45,
  "iterations": 3,
  "demand_predicates": [
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
  ]
}
```

### Aggregate Rules (Datalog^agg)

Derive facts using aggregate functions:

```sql
SELECT pg_ripple.load_rules('
# Count papers per author
?author ex:paperCount ?count :-
    COUNT(?paper WHERE ?paper dct:creator ?author) = ?count .

# Sum citation counts per paper
?paper ex:totalCitations ?total :-
    COUNT(?citing WHERE ?citing bibo:cites ?paper) = ?total .
', 'metrics');

-- Use the aggregate-aware inference function
SELECT pg_ripple.infer_agg('metrics');
```

Returns:

```json
{
  "derived": 25,
  "aggregate_derived": 25,
  "iterations": 1
}
```

### Well-Founded Semantics (v0.32.0)

For programs with cyclic negation (where standard stratification fails):

```sql
SELECT pg_ripple.load_rules('
# Cyclic negation: a node is "in" if it is not "out", and vice versa
?x ex:in "true"^^xsd:boolean :- ?x rdf:type ex:Node , NOT ?x ex:out "true"^^xsd:boolean .
?x ex:out "true"^^xsd:boolean :- ?x rdf:type ex:Node , NOT ?x ex:in "true"^^xsd:boolean .
', 'wfs-demo');

-- Standard infer() would fail with "unstratifiable" error
-- WFS handles it gracefully
SELECT pg_ripple.infer_wfs('wfs-demo');
```

Returns:

```json
{
  "derived": 6,
  "certain": 0,
  "unknown": 6,
  "iterations": 3,
  "stratifiable": false
}
```

Facts with `certainty = 'unknown'` are reported but NOT materialised into VP tables.

---

## Common Patterns

### Pattern: Layered Inference

Run rule sets in order — base entailment first, then domain rules:

```sql
-- Layer 1: RDFS entailment
SELECT pg_ripple.load_rules_builtin('rdfs');
SELECT pg_ripple.infer('rdfs');

-- Layer 2: OWL RL (builds on RDFS-derived facts)
SELECT pg_ripple.load_rules_builtin('owl-rl');
SELECT pg_ripple.infer('owl-rl');

-- Layer 3: Custom domain rules
SELECT pg_ripple.load_rules('...', 'domain');
SELECT pg_ripple.infer('domain');
```

### Pattern: Incremental Re-Inference

After adding new data, re-run inference. Semi-naive evaluation only derives new facts:

```sql
-- Load new data
SELECT pg_ripple.load_turtle('
@prefix ex:  <https://example.org/> .
@prefix bibo: <http://purl.org/ontology/bibo/> .
ex:paper/newOne a bibo:AcademicArticle .
');

-- Re-run inference — only new derivations are computed
SELECT pg_ripple.infer_with_stats('rdfs');
```

### Pattern: Explicit vs Inferred Triples

All VP tables have a `source` column: `0` = explicit, `1` = inferred. You can query
this distinction via SPARQL or check the full triple store:

```sql
-- Find all inferred type assertions
SELECT * FROM pg_ripple.find_triples(
    NULL,
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    NULL
);
```

### Pattern: owl:sameAs Canonicalization

When `pg_ripple.sameas_reasoning = 'on'` (default), `owl:sameAs` links are
canonicalized before inference. All mentions of equivalent entities are collapsed to
a single canonical ID, reducing redundant derivations.

```sql
-- Two IRIs refer to the same entity
SELECT pg_ripple.insert_triple(
    '<https://example.org/person/alice>',
    '<http://www.w3.org/2002/07/owl#sameAs>',
    '<https://other.org/people/a-johnson>'
);

-- After inference, both IRIs are treated as identical
SELECT pg_ripple.infer('owl-rl');
```

---

## Performance and Trade-offs

### Full Materialization vs Goal-Directed

| Strategy | Pros | Cons |
|---|---|---|
| Full (`infer()`) | Complete; all derived facts available | May derive millions of unneeded facts |
| Goal-directed (`infer_goal()`) | Only derives relevant facts | Must specify the goal pattern |
| Demand-filtered (`infer_demand()`) | Multiple goals; partial materialization | Slightly more setup |
| On-demand (query-time) | Zero materialization cost | Slower queries |

### Semi-Naive Evaluation

Semi-naive evaluation tracks which facts are new in each iteration and only joins
new facts with existing facts. This reduces the work per iteration from O(n^2) to
O(n * delta), where delta is the number of new facts per round.

### Subsumption Checking

When two rules have the same head and one rule's body is a subset of the other's, the
subsumed rule is eliminated. This reduces the number of SQL statements per iteration.

### Tabling / Memoisation (v0.32.0)

Goal-directed inference results and WFS results are cached in `_pg_ripple.tabling_cache`.
Cache entries are automatically invalidated when data changes (inserts or deletes).

```sql
-- Check tabling cache statistics
SELECT * FROM pg_ripple.tabling_stats();
```

### Rule Set Management

```sql
-- List all rules and their metadata
SELECT pg_ripple.list_rules();

-- Enable/disable a rule set without deleting it
SELECT pg_ripple.enable_rule_set('rdfs');
SELECT pg_ripple.disable_rule_set('quality');

-- Drop all rules in a set
SELECT pg_ripple.drop_rules('quality');
```

---

## Gotchas and Debugging

### Unstratifiable Programs

If your rules contain cyclic negation, standard `infer()` will fail:

```
ERROR: unstratifiable rule set — negation cycle detected
DETAIL: ex:in negates ex:out, which depends on ex:in
HINT: remove the negation cycle or use infer_wfs() for well-founded semantics
```

Fix: either restructure the rules to eliminate the cycle, or use `infer_wfs()`.

### Prefix Registration

Rules use the prefix registry from `register_prefix()`. If a prefix is not registered,
the parser treats it as a parse error:

```sql
-- Register required prefixes BEFORE loading rules
SELECT pg_ripple.register_prefix('ex', 'https://example.org/');
SELECT pg_ripple.register_prefix('dct', 'http://purl.org/dc/terms/');

-- Now load rules that use these prefixes
SELECT pg_ripple.load_rules('?x ex:rel ?y :- ?x dct:creator ?y .', 'test');
```

```admonish note
Built-in rule sets (`rdfs`, `owl-rl`) automatically register standard RDF/RDFS/OWL
prefixes.
```

### Checking What Was Derived

After inference, check the statistics:

```sql
-- How many triples total?
SELECT pg_ripple.triple_count();

-- Detailed stats including inferred count
SELECT pg_ripple.stats();

-- Check constraint rules for violations
SELECT pg_ripple.check_constraints();
```

### Performance Diagnosis

If inference is slow:

1. Check iteration count with `infer_with_stats()` — many iterations suggest deep recursive chains.
2. Use goal-directed inference (`infer_goal()`) if you only need a subset.
3. Check for redundant rules with subsumption (`eliminated_rules` in stats output).
4. Run `pg_ripple.vacuum()` after inference to update planner statistics.

```sql
-- Get inference diagnostics
SELECT pg_ripple.infer_with_stats('rdfs');

-- Check rule plan cache
SELECT * FROM pg_ripple.rule_plan_cache_stats();
```

---

## Next Steps

- **[§2.4 Validating Data Quality](../features/validating-data-quality.md)** — SHACL shapes interact with inference; validate derived facts.
- **[§2.6 Exporting and Sharing](../features/exporting-and-sharing.md)** — export inferred facts; Datalog enrichment for GraphRAG.
- **[§2.3 Querying with SPARQL](../features/querying-with-sparql.md)** — query both explicit and inferred facts with SPARQL.
