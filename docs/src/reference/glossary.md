# Glossary

Plain-language definitions of terms used throughout the pg_ripple documentation.

---

### Blank node

An anonymous node in an RDF graph — it has no IRI. Used when the identity of a resource does not matter, only its connections. Written as `_:label` in N-Triples/Turtle. Internally stored as a dictionary-encoded `BIGINT` like any other term.

### CDC (Change Data Capture)

A mechanism for subscribing to insert and delete events on the triple store. pg_ripple exposes CDC via `subscribe()` and `unsubscribe()`, backed by PostgreSQL `LISTEN`/`NOTIFY`.

### Dictionary encoding

The process of mapping every IRI, blank node, and literal to a unique `BIGINT` (i64) integer using an XXH3-128 hash. All VP tables store only integer IDs, never raw strings. This makes joins fast and storage compact.

### Embedding

A fixed-length numeric vector (typically 256–1536 dimensions) representing the semantic meaning of an entity or text. pg_ripple stores embeddings via pgvector and uses them for similarity search and RAG retrieval.

### Federation

Distributing a SPARQL query across multiple endpoints. When a query contains a `SERVICE <url> { … }` block, pg_ripple sends that subquery to the remote SPARQL endpoint and joins the results locally.

### Frame (JSON-LD)

A JSON template that reshapes a flat RDF graph into a tree-structured JSON-LD document. pg_ripple's `jsonld_frame()` and `export_jsonld_framed()` functions apply frames to produce nested, application-friendly JSON.

### GraphRAG

A retrieval-augmented generation (RAG) approach that uses a knowledge graph as the retrieval backend instead of (or in addition to) a vector store. pg_ripple exports data in Microsoft GraphRAG-compatible formats via `export_graphrag_entities()`, `export_graphrag_relationships()`, and `export_graphrag_text_units()`.

### GUC (Grand Unified Configuration)

PostgreSQL's configuration parameter system. pg_ripple exposes settings like `pg_ripple.max_path_depth` and `pg_ripple.dictionary_cache_size` as GUC parameters. Set them with `SET`, `ALTER SYSTEM SET`, or in `postgresql.conf`.

### HNSW (Hierarchical Navigable Small World)

An approximate nearest-neighbor index algorithm used by pgvector. pg_ripple creates HNSW indices on embedding columns for fast similarity search.

### HTAP (Hybrid Transactional/Analytical Processing)

pg_ripple's storage split (since v0.6.0) where writes go to a delta partition (heap + B-tree) and reads scan `(main EXCEPT tombstones) UNION ALL delta`. A background merge worker periodically combines delta into main with BRIN indices for analytical scan performance.

### IRI (Internationalized Resource Identifier)

A globally unique identifier for a resource in an RDF graph, like `<https://example.org/alice>`. Written in angle brackets in SPARQL and N-Triples. The RDF equivalent of a URL.

### JSON-LD

A JSON-based serialization of RDF. It represents triples as nested JSON objects using `@context` for namespace mapping and `@id` for node identifiers. pg_ripple can export to JSON-LD and apply JSON-LD frames.

### Literal

A data value in an RDF graph — a string, number, date, or boolean. Can have a datatype (`"42"^^xsd:integer`) or a language tag (`"hello"@en`). Stored as a dictionary-encoded integer in VP tables.

### Magic sets

A Datalog optimization technique that rewrites a program to focus computation on only the tuples needed to answer a specific query, rather than computing all possible derivations. Used by `infer_demand()`.

### Materialization

The process of computing all triples derivable from a set of Datalog rules and storing them explicitly in VP tables. `infer()` runs full materialization using semi-naive evaluation. Materialized triples have `source = 1` in VP tables.

### Merge worker

A pgrx background worker that periodically combines HTAP delta partitions into main partitions. It runs as a separate PostgreSQL backend process, configured via `pg_ripple.worker_database`.

### Named graph

A sub-graph of an RDF dataset identified by an IRI. Triples in the default graph have graph ID `0`; named graphs have IDs > 0. Named graphs are used for provenance tracking, access control, and dataset organization.

### OWL RL (Web Ontology Language — Rule Language profile)

A subset of OWL that can be implemented as Datalog rules. pg_ripple ships a built-in `owl-rl` rule set covering class and property reasoning (subclass, inverse, transitive, symmetric, `owl:sameAs` canonicalization).

### Predicate

The middle element of an RDF triple — the relationship between subject and object. For example, in `<alice> <knows> <bob>`, `<knows>` is the predicate. Each unique predicate gets its own VP table.

### Property path

A SPARQL syntax for traversing chains of predicates in a graph. Supports sequence (`/`), alternative (`|`), inverse (`^`), zero-or-more (`*`), one-or-more (`+`), and zero-or-one (`?`). Compiled to `WITH RECURSIVE … CYCLE` SQL.

### RAG (Retrieval-Augmented Generation)

An AI pattern that retrieves relevant context from a knowledge base before generating a response with a language model. pg_ripple's `rag_retrieve()` combines graph traversal and vector similarity for context retrieval.

### RDFS (RDF Schema)

A vocabulary for defining classes and properties in RDF. pg_ripple ships a built-in `rdfs` rule set that implements subclass inference (`rdfs:subClassOf`), domain/range inference (`rdfs:domain`, `rdfs:range`), and other RDFS entailment rules.

### RDF-star

An extension to RDF that allows triples to be subjects or objects of other triples (quoted triples). Written as `<< :alice :knows :bob >> :certainty 0.9` in Turtle-star. pg_ripple stores quoted triples via `qt_s`, `qt_p`, `qt_o` columns in the dictionary.

### RRF (Reciprocal Rank Fusion)

A score fusion method that combines rankings from multiple retrieval systems (e.g., SPARQL results and vector similarity). Used by `hybrid_search()` with a tunable `alpha` parameter.

### Semi-naive evaluation

The standard Datalog materialization algorithm. Instead of re-evaluating all rules each iteration, it only considers tuples derived in the *previous* iteration (the delta) joined with all known tuples. This avoids redundant computation.

### SHACL (Shapes Constraint Language)

A W3C standard for validating RDF graphs against a set of constraints (shapes). pg_ripple supports SHACL Core for data quality validation via `load_shacl()` and `validate()`, plus trigger-based and async DAG-aware monitoring.

### SID (Statement Identifier)

A globally unique `BIGINT` assigned to every triple from a shared PostgreSQL sequence (`statement_id_seq`). Stored in the `i` column of VP tables. Used by CDC, provenance tracking, and `get_statement()`.

### SPARQL

The W3C standard query language for RDF graphs. pg_ripple translates SPARQL to SQL and executes it against VP tables via SPI. Supports SELECT, CONSTRUCT, ASK, DESCRIBE, and the full Update language.

### Stratification

The process of ordering Datalog rules into strata so that negation and aggregation are evaluated in the correct sequence. Rules in stratum *n* depend only on predicates fully computed in strata < *n*. Programs with negation cycles through the same stratum are unstratifiable (use `infer_wfs()` instead).

### Tabling

A memoization technique for Datalog evaluation that caches intermediate results to avoid redundant computation and handle left-recursive rules. pg_ripple's tabling engine stores results in a memo table and checks for subsumption.

### Triple

The fundamental unit of data in RDF: a (subject, predicate, object) statement. For example, `<alice> <knows> <bob>` asserts that Alice knows Bob. pg_ripple stores triples as `(s, o, g)` integer tuples in VP tables, one table per predicate.

### VP table (Vertical Partitioning table)

pg_ripple's primary storage structure. Each unique predicate gets its own table (`_pg_ripple.vp_{id}`) with columns `s` (subject), `o` (object), `g` (graph), `i` (SID), and `source`. This layout optimizes predicate-specific scans and star-pattern joins.

### Well-founded semantics (WFS)

A three-valued semantics for Datalog programs with negation. Unlike stratification (which rejects some programs), WFS assigns every atom a value of *true*, *false*, or *undefined*. pg_ripple implements WFS via `infer_wfs()` for programs that cannot be stratified.
