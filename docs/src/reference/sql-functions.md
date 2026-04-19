# SQL Function Reference

All 157 SQL functions exposed by pg_ripple, grouped by use case. Every function lives in the `pg_ripple` schema.

```admonish tip title="Schema qualification"
All examples assume `SET search_path TO pg_ripple, public;`. If you prefer explicit qualification, prefix every call with `pg_ripple.`.
```

---

## Loading

Functions for inserting and bulk-loading RDF data.

---

### `insert_triple`

Insert a single triple into the default graph.

```sql
pg_ripple.insert_triple(
    subject   TEXT,
    predicate TEXT,
    object    TEXT
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
);
```

---

### `load_turtle`

Parse a Turtle string and load all triples into the default graph.

```sql
pg_ripple.load_turtle(
    data   TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_turtle('
@prefix ex: <https://example.org/> .
ex:alice ex:name "Alice" ;
         ex:knows ex:bob .
');
```

---

### `load_turtle_file`

Load Turtle from a server-side file path.

```sql
pg_ripple.load_turtle_file(
    path   TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_turtle_file('/data/ontology.ttl');
```

---

### `load_ntriples`

Parse an N-Triples string and load all triples into the default graph.

```sql
pg_ripple.load_ntriples(
    data   TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_ntriples('
<https://example.org/alice> <https://example.org/name> "Alice" .
<https://example.org/alice> <https://example.org/knows> <https://example.org/bob> .
');
```

---

### `load_ntriples_file`

Load N-Triples from a server-side file path.

```sql
pg_ripple.load_ntriples_file(
    path   TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_ntriples_file('/data/dump.nt');
```

---

### `load_nquads`

Parse an N-Quads string and load triples into their respective named graphs.

```sql
pg_ripple.load_nquads(
    data   TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_nquads('
<https://example.org/alice> <https://example.org/name> "Alice" <https://example.org/g1> .
');
```

---

### `load_nquads_file`

Load N-Quads from a server-side file path.

```sql
pg_ripple.load_nquads_file(
    path   TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_nquads_file('/data/dump.nq');
```

---

### `load_trig`

Parse a TriG string and load triples into their named graphs.

```sql
pg_ripple.load_trig(
    data   TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_trig('
@prefix ex: <https://example.org/> .
ex:g1 { ex:alice ex:name "Alice" . }
');
```

---

### `load_trig_file`

Load TriG from a server-side file path.

```sql
pg_ripple.load_trig_file(
    path   TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_trig_file('/data/dataset.trig');
```

---

### `load_rdfxml`

Parse an RDF/XML string and load all triples into the default graph.

```sql
pg_ripple.load_rdfxml(
    data   TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_rdfxml('
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:ex="https://example.org/">
  <rdf:Description rdf:about="https://example.org/alice">
    <ex:name>Alice</ex:name>
  </rdf:Description>
</rdf:RDF>
');
```

---

### `load_rdfxml_file`

Load RDF/XML from a server-side file path.

```sql
pg_ripple.load_rdfxml_file(
    path   TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_rdfxml_file('/data/ontology.rdf');
```

---

### `load_ntriples_into_graph`

Parse N-Triples and load into a specific named graph.

```sql
pg_ripple.load_ntriples_into_graph(
    data  TEXT,
    graph TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_ntriples_into_graph(
    '<https://example.org/alice> <https://example.org/name> "Alice" .',
    '<https://example.org/people>'
);
```

---

### `load_turtle_into_graph`

Parse Turtle and load into a specific named graph.

```sql
pg_ripple.load_turtle_into_graph(
    data  TEXT,
    graph TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_turtle_into_graph(
    '@prefix ex: <https://example.org/> . ex:alice ex:name "Alice" .',
    '<https://example.org/people>'
);
```

---

### `load_rdfxml_into_graph`

Parse RDF/XML and load into a specific named graph.

```sql
pg_ripple.load_rdfxml_into_graph(
    data  TEXT,
    graph TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_rdfxml_into_graph(
    '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
              xmlns:ex="https://example.org/">
       <rdf:Description rdf:about="https://example.org/alice">
         <ex:name>Alice</ex:name>
       </rdf:Description>
     </rdf:RDF>',
    '<https://example.org/people>'
);
```

---

### `load_ntriples_file_into_graph`

Load N-Triples from a server-side file into a named graph.

```sql
pg_ripple.load_ntriples_file_into_graph(
    path  TEXT,
    graph TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_ntriples_file_into_graph(
    '/data/people.nt',
    '<https://example.org/people>'
);
```

---

### `load_turtle_file_into_graph`

Load Turtle from a server-side file into a named graph.

```sql
pg_ripple.load_turtle_file_into_graph(
    path  TEXT,
    graph TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_turtle_file_into_graph(
    '/data/people.ttl',
    '<https://example.org/people>'
);
```

---

### `load_rdfxml_file_into_graph`

Load RDF/XML from a server-side file into a named graph.

```sql
pg_ripple.load_rdfxml_file_into_graph(
    path  TEXT,
    graph TEXT,
    strict BOOLEAN DEFAULT false
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_rdfxml_file_into_graph(
    '/data/people.rdf',
    '<https://example.org/people>'
);
```

---

### `load_owl_ontology`

Load an OWL ontology from Turtle, extracting class and property declarations for use by the Datalog reasoner.

```sql
pg_ripple.load_owl_ontology(
    data TEXT
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.load_owl_ontology('
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex: <https://example.org/> .
ex:Person a owl:Class .
ex:knows a owl:ObjectProperty ;
    owl:inverseOf ex:knownBy .
');
```

---

### `apply_patch`

Apply an RDF patch (additions and deletions) atomically.

```sql
pg_ripple.apply_patch(
    additions TEXT,
    deletions TEXT
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.apply_patch(
    '<https://example.org/alice> <https://example.org/age> "31"^^<http://www.w3.org/2001/XMLSchema#integer> .',
    '<https://example.org/alice> <https://example.org/age> "30"^^<http://www.w3.org/2001/XMLSchema#integer> .'
);
```

---

## Querying

Functions for querying triples with SPARQL and text search.

---

### `sparql`

Execute a SPARQL SELECT query and return results as a set of JSON objects.

```sql
pg_ripple.sparql(
    query TEXT
) RETURNS SETOF JSON
```

```sql
SELECT * FROM pg_ripple.sparql('
    PREFIX ex: <https://example.org/>
    SELECT ?name WHERE { ex:alice ex:name ?name }
');
```

---

### `sparql_ask`

Execute a SPARQL ASK query and return a boolean result.

```sql
pg_ripple.sparql_ask(
    query TEXT
) RETURNS BOOLEAN
```

```sql
SELECT pg_ripple.sparql_ask('
    PREFIX ex: <https://example.org/>
    ASK { ex:alice ex:knows ex:bob }
');
```

---

### `sparql_explain`

Return the SQL execution plan for a SPARQL query without executing it.

```sql
pg_ripple.sparql_explain(
    query TEXT
) RETURNS TEXT
```

```sql
SELECT pg_ripple.sparql_explain('
    PREFIX ex: <https://example.org/>
    SELECT ?x WHERE { ?x ex:knows ex:bob }
');
```

---

### `explain_sparql`

Return a detailed query plan showing SPARQL algebra and generated SQL.

```sql
pg_ripple.explain_sparql(
    query TEXT
) RETURNS TEXT
```

```sql
SELECT pg_ripple.explain_sparql('
    PREFIX ex: <https://example.org/>
    SELECT ?x ?y WHERE { ?x ex:knows ?y }
');
```

---

### `sparql_construct`

Execute a SPARQL CONSTRUCT query and return triples as JSON.

```sql
pg_ripple.sparql_construct(
    query TEXT
) RETURNS SETOF JSON
```

```sql
SELECT * FROM pg_ripple.sparql_construct('
    PREFIX ex: <https://example.org/>
    CONSTRUCT { ?x ex:friendOf ?y }
    WHERE { ?x ex:knows ?y }
');
```

---

### `sparql_describe`

Execute a SPARQL DESCRIBE query and return all triples about a resource.

```sql
pg_ripple.sparql_describe(
    query TEXT
) RETURNS SETOF JSON
```

```sql
SELECT * FROM pg_ripple.sparql_describe('
    PREFIX ex: <https://example.org/>
    DESCRIBE ex:alice
');
```

---

### `sparql_construct_turtle`

Execute a SPARQL CONSTRUCT query and return the result as a Turtle string.

```sql
pg_ripple.sparql_construct_turtle(
    query TEXT
) RETURNS TEXT
```

```sql
SELECT pg_ripple.sparql_construct_turtle('
    PREFIX ex: <https://example.org/>
    CONSTRUCT { ?x ex:friendOf ?y }
    WHERE { ?x ex:knows ?y }
');
```

---

### `sparql_construct_jsonld`

Execute a SPARQL CONSTRUCT query and return the result as a JSON-LD string.

```sql
pg_ripple.sparql_construct_jsonld(
    query TEXT
) RETURNS TEXT
```

```sql
SELECT pg_ripple.sparql_construct_jsonld('
    PREFIX ex: <https://example.org/>
    CONSTRUCT { ?x ex:friendOf ?y }
    WHERE { ?x ex:knows ?y }
');
```

---

### `sparql_describe_turtle`

Execute a SPARQL DESCRIBE query and return the result as Turtle.

```sql
pg_ripple.sparql_describe_turtle(
    query TEXT
) RETURNS TEXT
```

```sql
SELECT pg_ripple.sparql_describe_turtle('
    PREFIX ex: <https://example.org/>
    DESCRIBE ex:alice
');
```

---

### `sparql_describe_jsonld`

Execute a SPARQL DESCRIBE query and return the result as JSON-LD.

```sql
pg_ripple.sparql_describe_jsonld(
    query TEXT
) RETURNS TEXT
```

```sql
SELECT pg_ripple.sparql_describe_jsonld('
    PREFIX ex: <https://example.org/>
    DESCRIBE ex:alice
');
```

---

### `sparql_update`

Execute a SPARQL Update operation (INSERT DATA, DELETE DATA, etc.).

```sql
pg_ripple.sparql_update(
    query TEXT
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.sparql_update('
    PREFIX ex: <https://example.org/>
    INSERT DATA { ex:alice ex:age 30 }
');
```

---

### `find_triples`

Find triples matching a pattern in the default graph. Pass `NULL` for wildcards.

```sql
pg_ripple.find_triples(
    subject   TEXT DEFAULT NULL,
    predicate TEXT DEFAULT NULL,
    object    TEXT DEFAULT NULL
) RETURNS TABLE(subject TEXT, predicate TEXT, object TEXT)
```

```sql
SELECT * FROM pg_ripple.find_triples(
    '<https://example.org/alice>', NULL, NULL
);
```

---

### `find_triples_in_graph`

Find triples matching a pattern in a specific named graph.

```sql
pg_ripple.find_triples_in_graph(
    subject   TEXT DEFAULT NULL,
    predicate TEXT DEFAULT NULL,
    object    TEXT DEFAULT NULL,
    graph     TEXT DEFAULT NULL
) RETURNS TABLE(subject TEXT, predicate TEXT, object TEXT, graph TEXT)
```

```sql
SELECT * FROM pg_ripple.find_triples_in_graph(
    NULL, NULL, NULL, '<https://example.org/people>'
);
```

---

### `triple_count`

Return the total number of triples in the default graph.

```sql
pg_ripple.triple_count() RETURNS BIGINT
```

```sql
SELECT pg_ripple.triple_count();
```

---

### `triple_count_in_graph`

Return the number of triples in a specific named graph.

```sql
pg_ripple.triple_count_in_graph(
    graph TEXT
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.triple_count_in_graph('<https://example.org/people>');
```

---

### `fts_index`

Build or rebuild the full-text search index over literal values.

```sql
pg_ripple.fts_index() RETURNS VOID
```

```sql
SELECT pg_ripple.fts_index();
```

---

### `fts_search`

Search for triples containing a term in literal values via full-text search.

```sql
pg_ripple.fts_search(
    query TEXT,
    limit_rows INTEGER DEFAULT 100
) RETURNS TABLE(subject TEXT, predicate TEXT, object TEXT, rank REAL)
```

```sql
SELECT * FROM pg_ripple.fts_search('knowledge graph', 10);
```

---

## Graphs

Functions for managing named graphs.

---

### `create_graph`

Create a named graph.

```sql
pg_ripple.create_graph(
    graph TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.create_graph('<https://example.org/people>');
```

---

### `drop_graph`

Drop a named graph and all its triples.

```sql
pg_ripple.drop_graph(
    graph TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.drop_graph('<https://example.org/people>');
```

---

### `list_graphs`

List all named graphs.

```sql
pg_ripple.list_graphs() RETURNS TABLE(graph TEXT, triple_count BIGINT)
```

```sql
SELECT * FROM pg_ripple.list_graphs();
```

---

### `clear_graph`

Remove all triples from a graph without dropping it.

```sql
pg_ripple.clear_graph(
    graph TEXT
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.clear_graph('<https://example.org/people>');
```

---

## Dictionary

Functions for interacting with the dictionary encoder that maps IRIs, blank nodes, and literals to integer IDs.

```admonish note title="Internal use"
Most users never need to call dictionary functions directly. They are useful for debugging, performance tuning, and understanding storage internals.
```

---

### `encode_term`

Encode an IRI, literal, or blank node to its integer ID.

```sql
pg_ripple.encode_term(
    term TEXT
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.encode_term('<https://example.org/alice>');
```

---

### `decode_id`

Decode an integer ID back to its string representation.

```sql
pg_ripple.decode_id(
    id BIGINT
) RETURNS TEXT
```

```sql
SELECT pg_ripple.decode_id(42);
```

---

### `encode_triple`

Encode a full triple (subject, predicate, object) to integer IDs.

```sql
pg_ripple.encode_triple(
    subject   TEXT,
    predicate TEXT,
    object    TEXT
) RETURNS TABLE(s BIGINT, p BIGINT, o BIGINT)
```

```sql
SELECT * FROM pg_ripple.encode_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
);
```

---

### `decode_triple`

Decode a triple from integer IDs back to string form.

```sql
pg_ripple.decode_triple(
    s BIGINT,
    p BIGINT,
    o BIGINT
) RETURNS TABLE(subject TEXT, predicate TEXT, object TEXT)
```

```sql
SELECT * FROM pg_ripple.decode_triple(1, 2, 3);
```

---

### `decode_id_full`

Decode an integer ID returning the full term with type information.

```sql
pg_ripple.decode_id_full(
    id BIGINT
) RETURNS JSON
```

```sql
SELECT pg_ripple.decode_id_full(42);
```

---

### `lookup_iri`

Look up the integer ID for a specific IRI without inserting.

```sql
pg_ripple.lookup_iri(
    iri TEXT
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.lookup_iri('<https://example.org/alice>');
```

---

### `dictionary_stats`

Return statistics about the dictionary table.

```sql
pg_ripple.dictionary_stats() RETURNS JSON
```

```sql
SELECT pg_ripple.dictionary_stats();
```

---

### `prewarm_dictionary_hot`

Load the most frequently accessed dictionary entries into the shared cache.

```sql
pg_ripple.prewarm_dictionary_hot(
    limit_rows INTEGER DEFAULT 10000
) RETURNS INTEGER
```

```sql
SELECT pg_ripple.prewarm_dictionary_hot(50000);
```

---

### `cache_stats`

Return cache hit/miss statistics for the dictionary LRU cache.

```sql
pg_ripple.cache_stats() RETURNS JSON
```

```sql
SELECT pg_ripple.cache_stats();
```

---

## Prefixes

Functions for managing namespace prefix abbreviations.

---

### `register_prefix`

Register a namespace prefix for use in SPARQL queries and output.

```sql
pg_ripple.register_prefix(
    prefix TEXT,
    iri    TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.register_prefix('ex', 'https://example.org/');
```

---

### `prefixes`

List all registered prefixes.

```sql
pg_ripple.prefixes() RETURNS TABLE(prefix TEXT, iri TEXT)
```

```sql
SELECT * FROM pg_ripple.prefixes();
```

---

## Validating

Functions for loading SHACL shapes, validating data, and managing async validation.

---

### `load_shacl`

Load SHACL shapes from a Turtle string.

```sql
pg_ripple.load_shacl(
    shapes TEXT
) RETURNS INTEGER
```

```sql
SELECT pg_ripple.load_shacl('
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://example.org/> .
ex:PersonShape a sh:NodeShape ;
    sh:targetClass ex:Person ;
    sh:property [ sh:path ex:name ; sh:minCount 1 ; sh:datatype xsd:string ] .
');
```

---

### `validate`

Run SHACL validation and return a validation report.

```sql
pg_ripple.validate() RETURNS TABLE(
    focus_node TEXT,
    shape      TEXT,
    path       TEXT,
    severity   TEXT,
    message    TEXT
)
```

```sql
SELECT * FROM pg_ripple.validate();
```

---

### `list_shapes`

List all loaded SHACL shapes.

```sql
pg_ripple.list_shapes() RETURNS TABLE(shape TEXT, target TEXT, property_count INTEGER)
```

```sql
SELECT * FROM pg_ripple.list_shapes();
```

---

### `drop_shape`

Drop a SHACL shape by IRI.

```sql
pg_ripple.drop_shape(
    shape TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.drop_shape('<https://example.org/PersonShape>');
```

---

### `enable_shacl_monitors`

Enable trigger-based SHACL validation on all VP tables.

```sql
pg_ripple.enable_shacl_monitors() RETURNS VOID
```

```sql
SELECT pg_ripple.enable_shacl_monitors();
```

---

### `enable_shacl_dag_monitors`

Enable DAG-aware SHACL monitors using pg_trickle for async validation.

```sql
pg_ripple.enable_shacl_dag_monitors() RETURNS VOID
```

```sql
SELECT pg_ripple.enable_shacl_dag_monitors();
```

---

### `disable_shacl_dag_monitors`

Disable DAG-aware SHACL monitors.

```sql
pg_ripple.disable_shacl_dag_monitors() RETURNS VOID
```

```sql
SELECT pg_ripple.disable_shacl_dag_monitors();
```

---

### `list_shacl_dag_monitors`

List all active DAG SHACL monitors.

```sql
pg_ripple.list_shacl_dag_monitors() RETURNS TABLE(shape TEXT, predicate TEXT, enabled BOOLEAN)
```

```sql
SELECT * FROM pg_ripple.list_shacl_dag_monitors();
```

---

### `process_validation_queue`

Process pending items in the async SHACL validation queue.

```sql
pg_ripple.process_validation_queue(
    batch_size INTEGER DEFAULT 100
) RETURNS INTEGER
```

```sql
SELECT pg_ripple.process_validation_queue(500);
```

---

### `validation_queue_length`

Return the number of items pending in the validation queue.

```sql
pg_ripple.validation_queue_length() RETURNS BIGINT
```

```sql
SELECT pg_ripple.validation_queue_length();
```

---

### `dead_letter_count`

Return the number of items in the validation dead-letter queue.

```sql
pg_ripple.dead_letter_count() RETURNS BIGINT
```

```sql
SELECT pg_ripple.dead_letter_count();
```

---

### `dead_letter_queue`

Return the contents of the validation dead-letter queue.

```sql
pg_ripple.dead_letter_queue() RETURNS TABLE(
    id         BIGINT,
    triple_id  BIGINT,
    shape      TEXT,
    error      TEXT,
    created_at TIMESTAMPTZ
)
```

```sql
SELECT * FROM pg_ripple.dead_letter_queue();
```

---

### `drain_dead_letter_queue`

Remove and return all items from the dead-letter queue.

```sql
pg_ripple.drain_dead_letter_queue() RETURNS INTEGER
```

```sql
SELECT pg_ripple.drain_dead_letter_queue();
```

---

## Reasoning

Functions for Datalog rule management and inference.

```admonish tip title="Built-in rule sets"
pg_ripple ships with RDFS and OWL RL rule sets. Load them with `load_rules_builtin('rdfs')` or `load_rules_builtin('owl-rl')`.
```

---

### `load_rules`

Load a named Datalog rule set from a program string.

```sql
pg_ripple.load_rules(
    name    TEXT,
    program TEXT
) RETURNS INTEGER
```

```sql
SELECT pg_ripple.load_rules('transitive-knows', '
    knows(X, Z) :- knows(X, Y), knows(Y, Z).
');
```

---

### `load_rules_builtin`

Load a built-in rule set (rdfs, owl-rl).

```sql
pg_ripple.load_rules_builtin(
    name TEXT
) RETURNS INTEGER
```

```sql
SELECT pg_ripple.load_rules_builtin('owl-rl');
```

---

### `list_rules`

List all loaded rule sets.

```sql
pg_ripple.list_rules() RETURNS TABLE(name TEXT, rule_count INTEGER, enabled BOOLEAN)
```

```sql
SELECT * FROM pg_ripple.list_rules();
```

---

### `drop_rules`

Drop a rule set by name.

```sql
pg_ripple.drop_rules(
    name TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.drop_rules('transitive-knows');
```

---

### `enable_rule_set`

Enable a rule set for inference.

```sql
pg_ripple.enable_rule_set(
    name TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.enable_rule_set('owl-rl');
```

---

### `disable_rule_set`

Disable a rule set (triples already inferred are not removed).

```sql
pg_ripple.disable_rule_set(
    name TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.disable_rule_set('owl-rl');
```

---

### `infer`

Run materialization using all enabled rule sets (semi-naive evaluation).

```sql
pg_ripple.infer() RETURNS BIGINT
```

```sql
SELECT pg_ripple.infer();
```

---

### `infer_with_stats`

Run materialization and return iteration statistics.

```sql
pg_ripple.infer_with_stats() RETURNS JSON
```

```sql
SELECT pg_ripple.infer_with_stats();
```

---

### `infer_goal`

Run goal-directed inference for a specific query pattern.

```sql
pg_ripple.infer_goal(
    subject   TEXT DEFAULT NULL,
    predicate TEXT DEFAULT NULL,
    object    TEXT DEFAULT NULL
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.infer_goal(
    '<https://example.org/alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    NULL
);
```

---

### `infer_agg`

Run Datalog aggregation rules (min, max, sum, count).

```sql
pg_ripple.infer_agg() RETURNS BIGINT
```

```sql
SELECT pg_ripple.infer_agg();
```

---

### `infer_demand`

Run demand-driven inference with magic sets optimization.

```sql
pg_ripple.infer_demand(
    subject   TEXT DEFAULT NULL,
    predicate TEXT DEFAULT NULL,
    object    TEXT DEFAULT NULL
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.infer_demand(
    '<https://example.org/alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    NULL
);
```

---

### `infer_wfs`

Run well-founded semantics evaluation for programs with negation.

```sql
pg_ripple.infer_wfs() RETURNS BIGINT
```

```sql
SELECT pg_ripple.infer_wfs();
```

---

### `tabling_stats`

Return statistics about the tabling memo store.

```sql
pg_ripple.tabling_stats() RETURNS JSON
```

```sql
SELECT pg_ripple.tabling_stats();
```

---

### `rule_plan_cache_stats`

Return statistics about the Datalog rule plan cache.

```sql
pg_ripple.rule_plan_cache_stats() RETURNS JSON
```

```sql
SELECT pg_ripple.rule_plan_cache_stats();
```

---

### `check_constraints`

Run Datalog constraint rules and report violations.

```sql
pg_ripple.check_constraints() RETURNS TABLE(rule TEXT, subject TEXT, message TEXT)
```

```sql
SELECT * FROM pg_ripple.check_constraints();
```

---

## Exporting

Functions for serializing triples to various formats.

---

### `export_ntriples`

Export all triples as an N-Triples string.

```sql
pg_ripple.export_ntriples() RETURNS TEXT
```

```sql
SELECT pg_ripple.export_ntriples();
```

---

### `export_nquads`

Export all triples (with named graphs) as an N-Quads string.

```sql
pg_ripple.export_nquads() RETURNS TEXT
```

```sql
SELECT pg_ripple.export_nquads();
```

---

### `export_turtle`

Export all triples as a Turtle string.

```sql
pg_ripple.export_turtle() RETURNS TEXT
```

```sql
SELECT pg_ripple.export_turtle();
```

---

### `export_jsonld`

Export all triples as a JSON-LD string.

```sql
pg_ripple.export_jsonld() RETURNS TEXT
```

```sql
SELECT pg_ripple.export_jsonld();
```

---

### `export_turtle_stream`

Export triples as a streaming set of Turtle chunks for large datasets.

```sql
pg_ripple.export_turtle_stream(
    batch_size INTEGER DEFAULT 1000
) RETURNS SETOF TEXT
```

```sql
SELECT * FROM pg_ripple.export_turtle_stream(5000);
```

---

### `export_jsonld_stream`

Export triples as a streaming set of JSON-LD chunks for large datasets.

```sql
pg_ripple.export_jsonld_stream(
    batch_size INTEGER DEFAULT 1000
) RETURNS SETOF TEXT
```

```sql
SELECT * FROM pg_ripple.export_jsonld_stream(5000);
```

---

### `export_graphrag_entities`

Export entities in GraphRAG entity format for Microsoft GraphRAG or compatible tools.

```sql
pg_ripple.export_graphrag_entities() RETURNS SETOF JSON
```

```sql
SELECT * FROM pg_ripple.export_graphrag_entities();
```

---

### `export_graphrag_relationships`

Export relationships in GraphRAG relationship format.

```sql
pg_ripple.export_graphrag_relationships() RETURNS SETOF JSON
```

```sql
SELECT * FROM pg_ripple.export_graphrag_relationships();
```

---

### `export_graphrag_text_units`

Export text units in GraphRAG text-unit format.

```sql
pg_ripple.export_graphrag_text_units() RETURNS SETOF JSON
```

```sql
SELECT * FROM pg_ripple.export_graphrag_text_units();
```

---

## JSON-LD Framing

Functions for JSON-LD framing and tree-shaped output.

---

### `jsonld_frame_to_sparql`

Convert a JSON-LD frame to a SPARQL CONSTRUCT query.

```sql
pg_ripple.jsonld_frame_to_sparql(
    frame JSON
) RETURNS TEXT
```

```sql
SELECT pg_ripple.jsonld_frame_to_sparql('{
    "@type": "https://example.org/Person",
    "https://example.org/name": {}
}'::json);
```

---

### `export_jsonld_framed`

Export triples shaped by a JSON-LD frame as a JSON-LD string.

```sql
pg_ripple.export_jsonld_framed(
    frame JSON
) RETURNS TEXT
```

```sql
SELECT pg_ripple.export_jsonld_framed('{
    "@type": "https://example.org/Person",
    "https://example.org/name": {},
    "https://example.org/knows": { "@type": "https://example.org/Person" }
}'::json);
```

---

### `export_jsonld_framed_stream`

Export framed JSON-LD as a streaming set of chunks.

```sql
pg_ripple.export_jsonld_framed_stream(
    frame      JSON,
    batch_size INTEGER DEFAULT 100
) RETURNS SETOF TEXT
```

```sql
SELECT * FROM pg_ripple.export_jsonld_framed_stream('{
    "@type": "https://example.org/Person"
}'::json, 50);
```

---

### `jsonld_frame`

Apply a JSON-LD frame to an existing JSON-LD document.

```sql
pg_ripple.jsonld_frame(
    document JSON,
    frame    JSON
) RETURNS JSON
```

```sql
SELECT pg_ripple.jsonld_frame(
    pg_ripple.export_jsonld()::json,
    '{"@type": "https://example.org/Person"}'::json
);
```

---

## Views

Functions for creating and managing materialized SPARQL, Datalog, CONSTRUCT, DESCRIBE, ASK, and framing views.

```admonish note title="View lifecycle"
Views are backed by PostgreSQL tables or views. Use the corresponding `drop_*_view` function to remove them. Dropping the extension also removes all views.
```

---

### `create_sparql_view`

Create a PostgreSQL view backed by a SPARQL SELECT query.

```sql
pg_ripple.create_sparql_view(
    name  TEXT,
    query TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.create_sparql_view('people', '
    PREFIX ex: <https://example.org/>
    SELECT ?name WHERE { ?person a ex:Person ; ex:name ?name }
');
```

---

### `drop_sparql_view`

Drop a SPARQL view.

```sql
pg_ripple.drop_sparql_view(
    name TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.drop_sparql_view('people');
```

---

### `list_sparql_views`

List all SPARQL views.

```sql
pg_ripple.list_sparql_views() RETURNS TABLE(name TEXT, query TEXT)
```

```sql
SELECT * FROM pg_ripple.list_sparql_views();
```

---

### `create_datalog_view`

Create a PostgreSQL view backed by a Datalog rule.

```sql
pg_ripple.create_datalog_view(
    name  TEXT,
    rule  TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.create_datalog_view('ancestor',
    'ancestor(X, Z) :- parent(X, Y), ancestor(Y, Z).'
);
```

---

### `create_datalog_view_from_rule_set`

Create a view from a named rule set's head predicate.

```sql
pg_ripple.create_datalog_view_from_rule_set(
    view_name     TEXT,
    rule_set_name TEXT,
    head_predicate TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.create_datalog_view_from_rule_set(
    'inferred_types', 'owl-rl', 'rdf:type'
);
```

---

### `drop_datalog_view`

Drop a Datalog view.

```sql
pg_ripple.drop_datalog_view(
    name TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.drop_datalog_view('ancestor');
```

---

### `list_datalog_views`

List all Datalog views.

```sql
pg_ripple.list_datalog_views() RETURNS TABLE(name TEXT, rule_set TEXT, head TEXT)
```

```sql
SELECT * FROM pg_ripple.list_datalog_views();
```

---

### `create_framing_view`

Create a PostgreSQL view backed by a JSON-LD frame.

```sql
pg_ripple.create_framing_view(
    name  TEXT,
    frame JSON
) RETURNS VOID
```

```sql
SELECT pg_ripple.create_framing_view('person_frame', '{
    "@type": "https://example.org/Person",
    "https://example.org/name": {}
}'::json);
```

---

### `drop_framing_view`

Drop a framing view.

```sql
pg_ripple.drop_framing_view(
    name TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.drop_framing_view('person_frame');
```

---

### `list_framing_views`

List all framing views.

```sql
pg_ripple.list_framing_views() RETURNS TABLE(name TEXT, frame JSON)
```

```sql
SELECT * FROM pg_ripple.list_framing_views();
```

---

### `create_construct_view`

Create a view backed by a SPARQL CONSTRUCT query.

```sql
pg_ripple.create_construct_view(
    name  TEXT,
    query TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.create_construct_view('friends', '
    PREFIX ex: <https://example.org/>
    CONSTRUCT { ?a ex:friendOf ?b }
    WHERE { ?a ex:knows ?b }
');
```

---

### `drop_construct_view`

Drop a CONSTRUCT view.

```sql
pg_ripple.drop_construct_view(
    name TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.drop_construct_view('friends');
```

---

### `list_construct_views`

List all CONSTRUCT views.

```sql
pg_ripple.list_construct_views() RETURNS TABLE(name TEXT, query TEXT)
```

```sql
SELECT * FROM pg_ripple.list_construct_views();
```

---

### `create_describe_view`

Create a view backed by a SPARQL DESCRIBE query.

```sql
pg_ripple.create_describe_view(
    name  TEXT,
    query TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.create_describe_view('alice_detail', '
    PREFIX ex: <https://example.org/>
    DESCRIBE ex:alice
');
```

---

### `drop_describe_view`

Drop a DESCRIBE view.

```sql
pg_ripple.drop_describe_view(
    name TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.drop_describe_view('alice_detail');
```

---

### `list_describe_views`

List all DESCRIBE views.

```sql
pg_ripple.list_describe_views() RETURNS TABLE(name TEXT, query TEXT)
```

```sql
SELECT * FROM pg_ripple.list_describe_views();
```

---

### `create_ask_view`

Create a view backed by a SPARQL ASK query.

```sql
pg_ripple.create_ask_view(
    name  TEXT,
    query TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.create_ask_view('has_alice', '
    PREFIX ex: <https://example.org/>
    ASK { ex:alice ex:name ?n }
');
```

---

### `drop_ask_view`

Drop an ASK view.

```sql
pg_ripple.drop_ask_view(
    name TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.drop_ask_view('has_alice');
```

---

### `list_ask_views`

List all ASK views.

```sql
pg_ripple.list_ask_views() RETURNS TABLE(name TEXT, query TEXT)
```

```sql
SELECT * FROM pg_ripple.list_ask_views();
```

---

### `create_extvp`

Create an Extended VP (ExtVP) index for a predicate pair to accelerate star-pattern joins.

```sql
pg_ripple.create_extvp(
    predicate1 TEXT,
    predicate2 TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.create_extvp(
    '<https://example.org/name>',
    '<https://example.org/age>'
);
```

---

### `drop_extvp`

Drop an ExtVP index.

```sql
pg_ripple.drop_extvp(
    predicate1 TEXT,
    predicate2 TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.drop_extvp(
    '<https://example.org/name>',
    '<https://example.org/age>'
);
```

---

### `list_extvp`

List all ExtVP indices.

```sql
pg_ripple.list_extvp() RETURNS TABLE(predicate1 TEXT, predicate2 TEXT, row_count BIGINT)
```

```sql
SELECT * FROM pg_ripple.list_extvp();
```

---

## Federation

Functions for managing SPARQL federation endpoints.

---

### `register_endpoint`

Register a remote SPARQL endpoint for federated queries.

```sql
pg_ripple.register_endpoint(
    name TEXT,
    url  TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.register_endpoint('wikidata', 'https://query.wikidata.org/sparql');
```

---

### `set_endpoint_complexity`

Set the complexity weight for a federated endpoint (used by the query planner).

```sql
pg_ripple.set_endpoint_complexity(
    name       TEXT,
    complexity REAL
) RETURNS VOID
```

```sql
SELECT pg_ripple.set_endpoint_complexity('wikidata', 2.5);
```

---

### `remove_endpoint`

Remove a registered endpoint.

```sql
pg_ripple.remove_endpoint(
    name TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.remove_endpoint('wikidata');
```

---

### `disable_endpoint`

Temporarily disable an endpoint without removing it.

```sql
pg_ripple.disable_endpoint(
    name TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.disable_endpoint('wikidata');
```

---

### `list_endpoints`

List all registered federation endpoints.

```sql
pg_ripple.list_endpoints() RETURNS TABLE(name TEXT, url TEXT, enabled BOOLEAN, complexity REAL)
```

```sql
SELECT * FROM pg_ripple.list_endpoints();
```

---

### `register_vector_endpoint`

Register a vector similarity search endpoint for hybrid SPARQL+vector queries.

```sql
pg_ripple.register_vector_endpoint(
    name  TEXT,
    url   TEXT,
    model TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.register_vector_endpoint(
    'openai', 'https://api.openai.com/v1/embeddings', 'text-embedding-3-small'
);
```

---

## Vector / Hybrid Search

Functions for vector embeddings, similarity search, and RAG retrieval.

```admonish warning title="pgvector required"
All vector functions require pgvector to be installed. Set `pg_ripple.pgvector_enabled = off` to disable without uninstalling.
```

---

### `store_embedding`

Store a precomputed embedding vector for an entity.

```sql
pg_ripple.store_embedding(
    entity TEXT,
    model  TEXT,
    vector VECTOR
) RETURNS VOID
```

```sql
SELECT pg_ripple.store_embedding(
    '<https://example.org/alice>',
    'text-embedding-3-small',
    '[0.1, 0.2, 0.3]'::vector
);
```

---

### `similar_entities`

Find entities similar to a given entity by vector distance.

```sql
pg_ripple.similar_entities(
    entity TEXT,
    model  TEXT DEFAULT 'text-embedding-3-small',
    k      INTEGER DEFAULT 10
) RETURNS TABLE(entity TEXT, distance REAL)
```

```sql
SELECT * FROM pg_ripple.similar_entities('<https://example.org/alice>');
```

---

### `embed_entities`

Generate and store embeddings for entities matching a SPARQL pattern.

```sql
pg_ripple.embed_entities(
    query TEXT,
    model TEXT DEFAULT 'text-embedding-3-small'
) RETURNS INTEGER
```

```sql
SELECT pg_ripple.embed_entities('
    PREFIX ex: <https://example.org/>
    SELECT ?entity WHERE { ?entity a ex:Person }
');
```

---

### `refresh_embeddings`

Recompute embeddings for entities whose underlying data has changed.

```sql
pg_ripple.refresh_embeddings(
    model TEXT DEFAULT 'text-embedding-3-small'
) RETURNS INTEGER
```

```sql
SELECT pg_ripple.refresh_embeddings();
```

---

### `list_embedding_models`

List all embedding models with stored vectors.

```sql
pg_ripple.list_embedding_models() RETURNS TABLE(model TEXT, entity_count BIGINT, dimensions INTEGER)
```

```sql
SELECT * FROM pg_ripple.list_embedding_models();
```

---

### `add_embedding_triples`

Materialize similarity relationships as RDF triples.

```sql
pg_ripple.add_embedding_triples(
    model     TEXT DEFAULT 'text-embedding-3-small',
    threshold REAL DEFAULT 0.8,
    predicate TEXT DEFAULT '<https://example.org/similarTo>'
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.add_embedding_triples('text-embedding-3-small', 0.9);
```

---

### `contextualize_entity`

Return a text summary of an entity's neighborhood for use as LLM context.

```sql
pg_ripple.contextualize_entity(
    entity TEXT,
    hops   INTEGER DEFAULT 2
) RETURNS TEXT
```

```sql
SELECT pg_ripple.contextualize_entity('<https://example.org/alice>', 3);
```

---

### `hybrid_search`

Combine SPARQL graph pattern matching with vector similarity (Reciprocal Rank Fusion).

```sql
pg_ripple.hybrid_search(
    sparql_query TEXT,
    vector_query TEXT,
    k            INTEGER DEFAULT 10,
    alpha        REAL DEFAULT 0.5
) RETURNS TABLE(entity TEXT, score REAL, sparql_rank INTEGER, vector_rank INTEGER)
```

```sql
SELECT * FROM pg_ripple.hybrid_search(
    'PREFIX ex: <https://example.org/>
     SELECT ?person WHERE { ?person a ex:Person ; ex:knows ex:bob }',
    'researchers in knowledge graphs',
    10,
    0.7
);
```

---

### `rag_retrieve`

Retrieve context for RAG (Retrieval-Augmented Generation) using graph + vector search.

```sql
pg_ripple.rag_retrieve(
    query TEXT,
    k     INTEGER DEFAULT 5,
    hops  INTEGER DEFAULT 2
) RETURNS TABLE(entity TEXT, context TEXT, score REAL)
```

```sql
SELECT * FROM pg_ripple.rag_retrieve('Who knows about knowledge graphs?', 5, 2);
```

---

## Admin

Functions for maintenance, statistics, and administrative operations.

---

### `compact`

Compact the triple store by removing unreferenced VP tables and dictionary entries.

```sql
pg_ripple.compact() RETURNS JSON
```

```sql
SELECT pg_ripple.compact();
```

---

### `vacuum`

Vacuum all VP tables to reclaim space and update statistics.

```sql
pg_ripple.vacuum() RETURNS VOID
```

```sql
SELECT pg_ripple.vacuum();
```

---

### `reindex`

Rebuild all B-tree and BRIN indices on VP tables.

```sql
pg_ripple.reindex() RETURNS VOID
```

```sql
SELECT pg_ripple.reindex();
```

---

### `vacuum_dictionary`

Vacuum the dictionary table, removing entries not referenced by any VP table.

```sql
pg_ripple.vacuum_dictionary() RETURNS BIGINT
```

```sql
SELECT pg_ripple.vacuum_dictionary();
```

---

### `htap_migrate_predicate`

Migrate a predicate from the flat VP layout to the HTAP delta/main layout.

```sql
pg_ripple.htap_migrate_predicate(
    predicate TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.htap_migrate_predicate('<https://example.org/knows>');
```

---

### `stats`

Return overall triple store statistics.

```sql
pg_ripple.stats() RETURNS JSON
```

```sql
SELECT pg_ripple.stats();
```

---

### `canary`

Health-check function that returns true if the extension is loaded and functional.

```sql
pg_ripple.canary() RETURNS BOOLEAN
```

```sql
SELECT pg_ripple.canary();
```

---

### `enable_live_statistics`

Enable real-time statistics collection for VP tables.

```sql
pg_ripple.enable_live_statistics() RETURNS VOID
```

```sql
SELECT pg_ripple.enable_live_statistics();
```

---

### `promote_rare_predicates`

Promote predicates from `vp_rare` to dedicated VP tables if they exceed the threshold.

```sql
pg_ripple.promote_rare_predicates() RETURNS INTEGER
```

```sql
SELECT pg_ripple.promote_rare_predicates();
```

---

### `deduplicate_predicate`

Remove duplicate triples from a specific predicate's VP table.

```sql
pg_ripple.deduplicate_predicate(
    predicate TEXT
) RETURNS BIGINT
```

```sql
SELECT pg_ripple.deduplicate_predicate('<https://example.org/knows>');
```

---

### `deduplicate_all`

Remove duplicate triples from all VP tables.

```sql
pg_ripple.deduplicate_all() RETURNS BIGINT
```

```sql
SELECT pg_ripple.deduplicate_all();
```

---

### `delete_triple`

Delete a specific triple from the default graph.

```sql
pg_ripple.delete_triple(
    subject   TEXT,
    predicate TEXT,
    object    TEXT
) RETURNS BOOLEAN
```

```sql
SELECT pg_ripple.delete_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
);
```

---

### `delete_triple_from_graph`

Delete a specific triple from a named graph.

```sql
pg_ripple.delete_triple_from_graph(
    subject   TEXT,
    predicate TEXT,
    object    TEXT,
    graph     TEXT
) RETURNS BOOLEAN
```

```sql
SELECT pg_ripple.delete_triple_from_graph(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>',
    '<https://example.org/people>'
);
```

---

### `get_statement`

Retrieve a statement by its globally-unique statement ID (SID).

```sql
pg_ripple.get_statement(
    sid BIGINT
) RETURNS TABLE(subject TEXT, predicate TEXT, object TEXT, graph TEXT)
```

```sql
SELECT * FROM pg_ripple.get_statement(42);
```

---

## Security

Functions for row-level security, access control, and schema inspection.

---

### `enable_graph_rls`

Enable row-level security on VP tables, restricting access by named graph.

```sql
pg_ripple.enable_graph_rls() RETURNS VOID
```

```sql
SELECT pg_ripple.enable_graph_rls();
```

---

### `grant_graph`

Grant a user access to a named graph.

```sql
pg_ripple.grant_graph(
    username   TEXT,
    graph      TEXT,
    permission TEXT DEFAULT 'read'
) RETURNS VOID
```

```sql
SELECT pg_ripple.grant_graph('analyst', '<https://example.org/public>', 'read');
```

---

### `revoke_graph`

Revoke a user's access to a named graph.

```sql
pg_ripple.revoke_graph(
    username TEXT,
    graph    TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.revoke_graph('analyst', '<https://example.org/public>');
```

---

### `list_graph_access`

List all graph access grants.

```sql
pg_ripple.list_graph_access() RETURNS TABLE(username TEXT, graph TEXT, permission TEXT)
```

```sql
SELECT * FROM pg_ripple.list_graph_access();
```

---

### `enable_schema_summary`

Enable background schema summary generation (requires pg_trickle).

```sql
pg_ripple.enable_schema_summary() RETURNS VOID
```

```sql
SELECT pg_ripple.enable_schema_summary();
```

---

### `schema_summary`

Return a one-shot schema summary of all predicates, types, and counts.

```sql
pg_ripple.schema_summary() RETURNS JSON
```

```sql
SELECT pg_ripple.schema_summary();
```

---

## CDC

Functions for Change Data Capture subscriptions.

---

### `subscribe`

Subscribe to change events on the triple store. Returns a subscription ID.

```sql
pg_ripple.subscribe(
    channel  TEXT DEFAULT 'pg_ripple_changes',
    filter   TEXT DEFAULT NULL
) RETURNS TEXT
```

```sql
SELECT pg_ripple.subscribe('my_changes', 'predicate=<https://example.org/knows>');
```

---

### `unsubscribe`

Unsubscribe from a change event subscription.

```sql
pg_ripple.unsubscribe(
    subscription_id TEXT
) RETURNS VOID
```

```sql
SELECT pg_ripple.unsubscribe('sub_abc123');
```

---

## Index

Functions for querying predicate indices.

---

### `subject_predicates`

Return all predicates used by a given subject.

```sql
pg_ripple.subject_predicates(
    subject TEXT
) RETURNS TABLE(predicate TEXT)
```

```sql
SELECT * FROM pg_ripple.subject_predicates('<https://example.org/alice>');
```

---

### `object_predicates`

Return all predicates where a given resource appears as object.

```sql
pg_ripple.object_predicates(
    object TEXT
) RETURNS TABLE(predicate TEXT)
```

```sql
SELECT * FROM pg_ripple.object_predicates('<https://example.org/alice>');
```

---

## Cache

Functions for query plan cache management.

---

### `plan_cache_stats`

Return statistics about the SPARQL-to-SQL plan cache.

```sql
pg_ripple.plan_cache_stats() RETURNS JSON
```

```sql
SELECT pg_ripple.plan_cache_stats();
```

---

### `plan_cache_reset`

Clear the SPARQL-to-SQL plan cache.

```sql
pg_ripple.plan_cache_reset() RETURNS VOID
```

```sql
SELECT pg_ripple.plan_cache_reset();
```

---

### `pg_trickle_available`

Check whether the pg_trickle companion extension is installed and available.

```sql
pg_ripple.pg_trickle_available() RETURNS BOOLEAN
```

```sql
SELECT pg_ripple.pg_trickle_available();
```
