# pg-ripple

[![CI](https://github.com/trickle-labs/pg-ripple/actions/workflows/ci.yml/badge.svg)](https://github.com/trickle-labs/pg-ripple/actions/workflows/ci.yml)
[![Release](https://github.com/trickle-labs/pg-ripple/actions/workflows/release.yml/badge.svg)](https://github.com/trickle-labs/pg-ripple/actions/workflows/release.yml)
[![Roadmap](https://img.shields.io/badge/Roadmap-view-informational)](ROADMAP.md)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![PostgreSQL 18](https://img.shields.io/badge/PostgreSQL-18-blue?logo=postgresql&logoColor=white)](https://www.postgresql.org/)
[![pgrx 0.18](https://img.shields.io/badge/pgrx-0.18-orange)](https://github.com/pgcentralfoundation/pgrx)
[![WatDiv evidence](https://img.shields.io/badge/WatDiv-versioned%20evidence-informational)](docs/src/reference/watdiv-results.md)
[![LUBM conformance](https://img.shields.io/badge/LUBM-required%20gate-informational)](docs/src/reference/lubm-results.md)
[![Jena conformance](https://img.shields.io/badge/Jena-informational%20suite-informational)](docs/src/reference/jena-results.md)
[![OWL 2 RL conformance](https://img.shields.io/badge/OWL%202%20RL-informational%20suite-informational)](docs/src/reference/owl2rl-results.md)
[![PPRL](https://img.shields.io/badge/PPRL-CLK%20%2B%20DP-purple)](docs/src/cookbook/pprl.md)
[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/trickle-labs/pg-ripple)

**A knowledge graph engine built into PostgreSQL.**

pg_ripple is a PostgreSQL 18 extension that turns your database into a knowledge graph store. You can model data as a web of connected facts — entities, relationships, and properties — and then query, validate, and reason over those connections, all from within the database you already run.

The [shared project glossary](GLOSSARY.md) defines the terms used in the code, documentation, and conversations around pg_ripple.

No separate graph database. No data pipelines. No extra infrastructure.

> **New to knowledge graphs?** Think of a knowledge graph as a smarter, more connected way to store data. Instead of rows in tables, you store facts: *Alice knows Bob*, *Bob works at Acme Corp*, *Acme Corp is in Oslo*. You can then ask questions that span many hops: *"Who are all the people in Alice's extended professional network?"* — the kind of question that is painful in SQL but natural in a graph.

---

## What works today (v0.135.0)

pg_ripple publishes immutable, versioned conformance and feature evidence for
each release. W3C SPARQL 1.1 smoke tests and LUBM are required gates; Jena,
WatDiv, and OWL 2 RL are explicitly informational until their locked suites
meet the release thresholds.

| What you can do | How it works |
|---|---|
| **Import knowledge** | Load data in standard formats: Turtle, N-Triples, N-Quads, TriG, or RDF/XML — from files, inline text, or remote URLs. Named graphs let you organize facts into logical groups (e.g. one graph per data source or topic). |
| **Query with SPARQL** | Ask complex questions using SPARQL 1.1 — the W3C standard query language for linked data (similar to SQL, but designed for graphs). Follow chains of relationships, apply filters, aggregate results, and query across multiple graphs. Typed application bindings are available on SELECT, ASK, CONSTRUCT, DESCRIBE, and cursor overloads. Prefix lookup stays strict by default; `pg_ripple.sparql_prefix_mode = 'registered'` enables the governed registry. |
| **AI and LLM integration** | Store vector embeddings alongside graph facts. Combine semantic similarity search (*"find things similar to X"*) with SPARQL graph traversal in one query. Built-in RAG pipeline retrieves graph-contextualized context for language model prompts. Use `sparql_construct_jsonld()` with a JSON-LD frame to generate structured, token-efficient system prompts directly from a SPARQL CONSTRUCT query. |
| **Microsoft GraphRAG** | Export entities and relationships in GraphRAG's BYOG (Bring Your Own Graph) Parquet format. Enrich the graph with Datalog rules. Validate export quality with SHACL. Connect your knowledge graph to Microsoft's GraphRAG pipeline with a single SQL call. |
| **Validate data quality** | Define quality rules with SHACL: *"every Person must have exactly one name"*, *"age must be a positive integer"*. Violations are caught on insert (immediate feedback) or checked in the background. Full SHACL Core conformance, including `sh:equals`, `sh:disjoint`, and complex property path traversal (inverse, alternative, sequence, zero-or-more, one-or-more). Violation reports include decoded focus-node IRIs for easy debugging. |
| **Infer new facts automatically** | Write Datalog rules to derive conclusions from what you already know — *"if Alice manages Bob and Bob manages Carol, then Alice indirectly manages Carol"*. Includes built-in support for standard RDFS and OWL reasoning. Goal-directed mode (`infer_goal()`) and demand-filtered mode (`infer_demand()`) derive only the facts relevant to your query, reducing inference work by 50–90% on large programs. `owl:sameAs` entity canonicalization is applied automatically before inference, so equivalent entities are treated as one. Well-founded semantics (`infer_wfs()`) handles non-stratifiable programs with mutual negation. Tabling caches repeated inference sub-goals (2–5× speedup). Parallel stratum evaluation runs independent rule groups concurrently. Worst-case optimal joins accelerate cyclic graph queries. Incremental retraction (DRed) keeps derived predicates consistent after deletions without full recomputation. |
| **Stream and inspect queries** | Use `sparql_cursor()` to stream large result sets page-by-page via the PostgreSQL portal API — peak memory is bounded by `pg_ripple.export_batch_size`, not the full result set. Export results as W3C CSV or TSV via `sparql_csv()` / `sparql_tsv()`. Use `explain_sparql()` and `explain_datalog()` to introspect query plans and rule compilation. Pass `citus := true` to `explain_sparql()` for a Citus shard-pruning section showing which shard the query was pruned to and how many rows were avoided. `streaming_metrics()` returns live atomic counters for cursor pages, Arrow batches, and ticket rejections. OpenTelemetry span tracing is available, with a configurable OTLP endpoint (`pg_ripple.tracing_otlp_endpoint`). |
| **Live change notifications** | Subscribe to graph changes via PostgreSQL NOTIFY or Server-Sent Events (SSE). `create_subscription(name, filter_sparql)` fires `pg_notify` on the `pg_ripple_cdc_{name}` channel when matching triples change. `subscribe_sparql(id, query, graph_iri)` registers a SPARQL-query subscription that re-executes after each graph write and streams results as SSE via `GET /subscribe/{id}` in `pg_ripple_http`. `unsubscribe_sparql(id)` removes a subscription; `list_sparql_subscriptions()` enumerates active ones. CDC lifecycle events (`pg_ripple.cdc_lifecycle_events`) record subscription activity. |
| **Export and share** | Export your graph as Turtle, N-Triples, JSON-LD, or RDF/XML. Use JSON-LD framing to produce nested documents shaped for REST APIs or LLM prompts. `export_jsonld_node(iri)` returns all triples for a given subject as a JSON-LD document. `json_ld_load(document, default_graph)` ingests multi-graph JSON-LD documents in one call. `COPY rdf FROM` loads bulk RDF files directly via PostgreSQL's COPY protocol. Arrow IPC bulk export via `pg_ripple_http`: HMAC-SHA256 signed tickets with nonce replay protection, binary IPC stream over `POST /flight/do_get`. |
| **Standard HTTP endpoint** | The companion `pg_ripple_http` service exposes a W3C SPARQL Protocol endpoint over HTTP/HTTPS. Supports JSON, XML, CSV, Turtle, and JSON-LD responses; typed initial bindings at `POST /sparql/bindings`; authentication; Prometheus metrics (`/metrics`); extension-level metrics via `/metrics/extension` (triple count, active graphs, GUC settings); Docker Compose for easy deployment; full OpenAPI 3.1 specification; and an Arrow/Flight bulk-export endpoint. |
| **Query remote graph services** | Use the SPARQL `SERVICE` keyword to query external SPARQL endpoints as part of a single query — your local data and a remote public dataset in one request. Includes connection pooling, result caching, safe timeouts, and a circuit breaker (`pg_ripple.federation_circuit_breaker_threshold`) that stops retrying failed endpoints. |
| **Horizontal scaling with Citus** | Enable `pg_ripple.citus_sharding_enabled` to distribute VP tables across Citus worker nodes. Bound-subject SPARQL patterns are automatically pruned to the correct shard (10–100× speedup). `citus_rebalance()` emits NOTIFY signals so pg-trickle can pause CDC during rebalancing. `citus_rebalance_progress()` reports live shard-move status. |
| **Temporal RDF queries** | `point_in_time(ts TIMESTAMPTZ)` restricts all SPARQL queries in the current session to facts that existed at the given timestamp — enabling as-of queries, audit trails, and temporal joins without schema changes. |
| **PROV-O data provenance** | Enable `pg_ripple.prov_enabled` to automatically record W3C PROV-O `prov:Activity` + `prov:Entity` triples for every bulk-load operation. `prov_stats()` summarises load history. |
| **Geospatial queries** | GeoSPARQL 1.1: filter by `geof:within`, `geof:intersects`, and `geof:distance`; compute `geof:buffer`, `geof:convexHull`, `geof:envelope`. Geometry values stored as WKT literals and processed via PostGIS. |
| **OWL 2 EL/QL reasoning profiles** | Activate `load_rules_builtin('owl-el')` or `load_rules_builtin('owl-ql')` for profile-specific reasoning. OWL 2 QL rewrites SPARQL BGPs at translation time for DL-Lite ontologies. Control with `pg_ripple.owl_profile`. |
| **Knowledge graph embeddings** | Enable `pg_ripple.kge_enabled` to train TransE or RotatE entity embeddings stored in `_pg_ripple.kge_embeddings` with an HNSW index. Use `find_alignments()` to propose cross-graph `owl:sameAs` candidates by cosine similarity. |
| **SPARQL audit log** | Enable `pg_ripple.audit_log_enabled` to record all SPARQL UPDATE operations (role, transaction ID, query text) in `_pg_ripple.audit_log`. `purge_audit_log(before)` cleans up old entries. |
| **Multi-tenant graph isolation** | `create_tenant()` registers a named graph with a triple-count quota. Triggers enforce the quota on insert; `tenant_stats()` reports usage per tenant. |
| **SPARQL-DL OWL axiom queries** | `sparql_dl_subclasses(IRI)` and `sparql_dl_superclasses(IRI)` route OWL vocabulary BGPs (`owl:subClassOf`, `owl:equivalentClass`, `owl:disjointWith`) directly to VP table T-Box data — no separate index required. |
| **SHACL-SPARQL rules** | SHACL Advanced Features: `sh:SPARQLRule` and `sh:SPARQLConstraint` are evaluated as native SPARQL queries against the VP store, enabling complex cross-shape validation that cannot be expressed with pure property-path SHACL. |
| **JSON↔RDF mapping registry** | Register named bidirectional JSON↔RDF mappings with `register_json_mapping(name, context_jsonb, shape_iri)`. `ingest_json(mapping, document)` converts a JSON document to RDF triples using the stored context; `export_json_node(mapping, iri)` converts a graph node back to JSON. Mapping inconsistencies with the optional SHACL shape are recorded in `_pg_ripple.json_mapping_warnings`. `writeback_json_row()` / `writeback_json_row_delete()` push RDF graph changes back into the source relational table (conflict policies `replace`/`skip`/`error`); `enable_json_writeback()` installs triggers — covering both not-yet-promoted and already-promoted predicates — so changes are queued and applied automatically. |
| **R2RML direct mapping** | `pg_ripple.r2rml_load(mapping_ttl)` applies an R2RML mapping document to convert relational tables in the same database into RDF triples, inserted directly into the VP store. |
| **Graph analytics (PageRank)** | Datalog-native iterative PageRank via `pg_ripple.pagerank_run()`. Supports topic-sensitive, personalized, confidence-weighted, and temporal-decay variants. Incremental refresh via IVM dirty-edge queue. Four centrality measures (betweenness, closeness, degree, Katz). `pg:pagerank()` SPARQL function. Score-explanation trees via `explain_pagerank()`. Sketch-based approximate top-N. SHACL-aware ranking. Standard-format export (CSV, Turtle, N-Triples, JSON-LD). |
| **Probabilistic reasoning** | `@weight(FLOAT)` annotations on Datalog rules for probabilistic inference with noisy-OR confidence propagation. `pg:confidence()`, `pg:fuzzy_match()`, `pg:token_set_ratio()` SPARQL functions. Soft SHACL scoring (`shacl_score()`). Confidence-weighted bulk load. PROV-O source-trust propagation. Cyclic probabilistic programs with well-founded convergence guarantees. |
| **Bayesian confidence updates** | `update_confidence(subject, predicate, object, evidence)` applies Bayes' theorem in odds form for dynamic belief revision. `bulk_update_confidence()` ingests CSV or JSON-L updates in batches. An append-only `_pg_ripple.evidence_log` records every update with full audit trail. Downstream confidence propagation walks the derivation DAG up to a configurable depth. Configurable strategy: `'bayesian'`, `'noisy-or'`, or `'manual'`. |
| **Temporal knowledge graphs** | Register predicates as temporal via `mark_temporal()`. A first-class `_pg_ripple.temporal_facts` store tracks `(s, p, o, valid_from, valid_to)`. `insert_triple_temporal()` inserts time-stamped facts in snapshot or versioned mode. Datalog temporal filters: `AFTER`, `BEFORE`, `DURING`. Sequential operators: `temporal_within()`, `temporal_sequence()`, `temporal_consecutive()`. `sh:validFor` SHACL constraint. CDC auto-recording wired into `insert_triple()`. `retract_triple_temporal()` closes open intervals. |
| **Proof trees & justification** | Enable `pg_ripple.record_derivations` to record provenance for every Datalog-derived fact in `_pg_ripple.derivations`. `justify(subject, predicate, object)` returns the full backward-chaining proof tree as JSONB with cycle protection (depth limit 64). `vacuum_derivations()` cleans up orphaned rows. Proof trees survive DRed retraction cleanly. |
| **Natural-language explanation** | `explain_inference(subject, predicate, object)` returns a plain-English narrative of why a fact was derived — via the configured LLM endpoint or a deterministic fallback renderer. `explain_inference_jsonb()` returns the combined proof tree and narrative for programmatic consumers. Results cached in `_pg_ripple.explanation_cache` with configurable TTL. `vacuum_explanation_cache()` prunes expired entries. |
| **What-if (hypothetical) reasoning** | `hypothetical_inference(hypotheses, rules)` runs Datalog inference on speculative graph modifications without persisting changes. Returns `{"derived": [...], "retracted": [...]}`. All changes are wrapped in PostgreSQL internal sub-transactions — VP tables are never modified. `hypothetical_max_assertions` GUC bounds resource usage. REST endpoint `POST /hypothetical` in `pg_ripple_http`. |
| **Conflict detection** | `rule_conflicts(ruleset, mode)` detects contradictory Datalog rules. Static mode: structural analysis (same-head opposing values, rule-vs-SHACL constraint). Runtime mode: scans `_pg_ripple.derivations` for already-derived contradictions. `block_on_conflict` GUC halts `infer()` when conflicts are found. `rule_conflict_check_on_load` GUC enables automatic static analysis at load time. REST endpoint `GET /rule-conflicts/{ruleset}` in `pg_ripple_http`. |
| **Domain rule libraries** | `install_rule_library(source)` installs a versioned Datalog rule set and SHACL shapes from a URL or local Turtle file. Libraries carry metadata (`dcterms:title`, `owl:versionInfo`, license IRI) and dependency declarations. SSRF protection, license checks, and topological dependency resolution built in. `upgrade_rule_library()` / `uninstall_rule_library()` manage lifecycle. `list_rule_libraries()` enumerates installed libraries. REST endpoint `GET /rule-libraries` in `pg_ripple_http`. |
| **Guided rule authoring** | `validate_rule(rule)` performs static analysis — detects syntax errors, unbound head variables, unsafe negation, and stratification cycles. `draft_rule_from_nl(description)` calls a configured LLM to translate natural-language descriptions into ranked Datalog candidates. `suggest_rules(graph_iri)` scans predicate co-occurrence patterns to propose candidate rules. REST endpoints `POST /rules/draft` and `POST /rules/validate` in `pg_ripple_http`. |
| **Neuro-symbolic record linkage (NS-RL)** | Six SPARQL and Datalog string-similarity built-ins: `pg:trigram_similarity()`, `pg:levenshtein()`, `pg:levenshtein_less_equal()`, `pg:soundex()`, `pg:metaphone()`, `pg:jaro_winkler()`. Three built-in ER blocking templates (`email`, `postal_name`, `name_prefix`). `resolve_entities(source_graph, target_graph)` orchestrates a five-stage pipeline: symbolic blocking, embedding-based candidate generation, SHACL validation gate, `owl:sameAs` canonicalization, RDF-star provenance annotation. Dry-run mode returns a JSON summary without writing. |
| **NS-RL evaluation & monitoring** | `evaluate_resolution(gold_graph)` computes pairwise (precision/recall/F1), blocking (pairs_completeness/reduction_ratio/F-PQ), and B³ cluster metrics against a gold standard. `enable_er_monitoring()` creates three live stream tables: unresolved entities, cluster sizes, and a resolution dashboard. `explain_rule(rule_id)` narrates a rule in plain English. `owl:sameAs` anomaly detection logs PT550-triggering assertions to `_pg_ripple.sameas_anomaly_log`. Magellan ER benchmark CI gate (Abt-Buy F1 ≥ 0.78, DBLP-ACM F1 ≥ 0.90). |
| **Privacy-Preserving Record Linkage (PPRL)** | `bloom_encode(value, key, hash_count, length)` produces CLK Bloom-filter encodings using HMAC-SHA-256 — enabling entity matching without exposing raw PII. `dice_similarity(a, b)` computes Dice coefficient for two Bloom-filter hex bit vectors. `pg:dice_similarity(?a, ?b)` SPARQL FILTER function and Datalog built-in predicate for cross-graph privacy-safe entity matching. `dp_noisy_count(query, epsilon)` and `dp_noisy_histogram(query, key_col, count_col, epsilon)` add Laplace differential-privacy noise to aggregate queries for privacy-preserving analytics. |
| **Temporal graph snapshots** | `graph_at(graph_iri, snapshot_time)` materialises a point-in-time named-graph snapshot from `_pg_ripple.temporal_facts`, registering a deterministic `urn:snapshot:…` IRI usable directly in `GRAPH <iri> { … }` SPARQL queries. `graph_diff(graph_iri, from_ts, to_ts)` returns `'added'`/`'removed'` delta rows between two timestamps for audit-compliant change logs and incremental exports. Snapshots are automatically garbage-collected after `pg_ripple.snapshot_retention_days` (default 30). HTTP endpoints `GET /temporal/graphs/{iri}/snapshot` and `GET /temporal/graphs/{iri}/diff` expose both operations. Prometheus gauge `pg_ripple_graph_snapshots_total` tracks live snapshot count. |
| **Per-endpoint federation credentials** | `set_federation_credential(endpoint_iri, auth_type, token)` registers or atomically replaces an encrypted Bearer or API-key credential for any federation endpoint; tokens are stored at-rest via `pgcrypto.pgp_sym_encrypt` and are never returned in plaintext. `rotate_federation_credential(endpoint_iri, new_token)` replaces a token and records the rotation timestamp atomically. `federation_credential_audit()` returns operational metadata (token age, last used) without exposing secrets. The federation executor automatically injects the correct HTTP header after SSRF validation — existing SPARQL queries require no changes. HTTP endpoint `GET /federation/{endpoint}/auth-status` exposes credential status for monitoring. |
| **Live, auto-updating views** | Define a SPARQL query as a view; pg_ripple (with the optional `pg_trickle` companion) keeps it automatically up to date as data changes. |
| **Access control** | Named graphs have row-level security backed by PostgreSQL's built-in permission system. Each graph can be granted to specific database roles, just like a table. Read-replica routing sends read queries to replicas automatically when `pg_ripple.read_replica_dsn` is configured. |
| **Full-text search** | Search the text of literal values (names, descriptions, notes) using PostgreSQL's fast full-text search indexes. |

Here is a taste of what working with pg_ripple looks like from SQL:

```sql
CREATE EXTENSION pg_ripple;

-- Import a Turtle file (a standard text format for RDF knowledge graphs)
SELECT pg_ripple.load_turtle(pg_read_file('/data/people.ttl'));

-- Query with a property path: find everyone Alice can reach via "knows"
-- (follows the chain Alice→Bob→Carol→… automatically)
SELECT * FROM pg_ripple.sparql('
  PREFIX foaf: <http://xmlns.com/foaf/0.1/>
  SELECT ?name WHERE {
    <http://example.org/Alice> foaf:knows+ ?person .
    ?person foaf:name ?name .
  }
');

-- Enforce a SHACL constraint: every Person must have exactly one name
SELECT pg_ripple.load_shacl('
  @prefix sh: <http://www.w3.org/ns/shacl#> .
  <http://example.org/PersonShape> a sh:NodeShape ;
    sh:targetClass <http://example.org/Person> ;
    sh:property [ sh:path foaf:name ; sh:minCount 1 ; sh:maxCount 1 ] .
');

-- Export the whole graph as Turtle
SELECT pg_ripple.export_turtle();

-- SPARQL CONSTRUCT → JSON-LD for a REST API
SELECT pg_ripple.sparql_construct_jsonld('
  CONSTRUCT { ?s ?p ?o } WHERE { ?s a <http://schema.org/Person> ; ?p ?o }
');

-- Load RDFS entailment rules and run inference
-- After this, if :Dog is a subclass of :Animal, and :Rex is a Dog,
-- then SPARQL will also return :Rex when you ask for Animals.
SELECT pg_ripple.load_rules_builtin('rdfs');
SELECT pg_ripple.infer('rdfs');

-- Write custom rules (transitive management chain)
SELECT pg_ripple.load_rules(
  '?x ex:indirectManager ?z :- ?x ex:manager ?z .
   ?x ex:indirectManager ?z :- ?x ex:manager ?y, ?y ex:indirectManager ?z .',
  'org_rules'
);
SELECT pg_ripple.infer('org_rules');

-- ── AI / LLM integration ──────────────────────────────────────────────

-- Hybrid retrieval: graph pattern + vector similarity in one query
-- Find papers semantically similar to a topic, authored by co-authors
SELECT * FROM pg_ripple.sparql('
  PREFIX ex: <http://example.org/>
  PREFIX pg:  <http://pg-ripple.io/fn/>
  SELECT ?paper ?title ?score WHERE {
    <http://example.org/Alice> ex:coAuthor+ ?colleague .
    ?colleague ex:authored ?paper .
    ?paper ex:title ?title .
    BIND(pg:similar(?paper, "graph neural networks") AS ?score)
    FILTER(?score > 0.75)
  }
  ORDER BY DESC(?score)
');

-- Generate a structured JSON-LD system prompt for an LLM
-- The frame shapes the output to exactly the JSON your prompt template expects
SELECT pg_ripple.sparql_construct_jsonld(
  'CONSTRUCT { ?s ex:name ?name ; ex:role ?role ; ex:manages ?report }
   WHERE   { ?s a ex:Person ; ex:name ?name ; ex:role ?role .
             OPTIONAL { ?s ex:manages ?report } }',
  -- JSON-LD frame: produces nested {"name":..., "manages":[...]} objects
  '{"@type": "ex:Person", "ex:manages": {}}'
);

-- Graph-contextualized RAG retrieval
-- Returns a JSONB context block ready for use as an LLM system prompt
SELECT pg_ripple.rag_retrieve(
  query_embedding  => ai.embed('Who manages the Oslo team?'),
  graph_patterns   => ARRAY['?s ex:locatedIn ex:Oslo', '?s ex:role ?role'],
  top_k            => 10
);
```

---

## AI and LLM use cases

pg_ripple is a natural fit for AI applications that need structured, explainable context — not just a bag of vectors. Here are three concrete scenarios.

### Knowledge-augmented RAG

Pure vector search finds *similar* documents but loses the *relationships* between them. pg_ripple lets you combine both: a SPARQL graph pattern selects entities by relationship ("papers authored by Alice's co-authors in the last two years"), and a vector similarity filter (`pg:similar()`) ranks them by semantic closeness to the query. Reciprocal Rank Fusion merges the two result lists. The retrieval context sent to the LLM is more precise and more explainable than a flat top-k vector search.

### Entity resolution before embedding

Enterprise data has duplicates: `"Alice Smith"`, `"A. Smith"`, and `"alice.smith@example.com"` may all refer to the same person. pg_ripple's `owl:sameAs` entity canonicalization collapses these into a single canonical entity before inference or embedding. When the LLM asks about Alice, it gets a unified view — not three contradictory fragments.

### Structured prompts via JSON-LD framing

Token budgets matter. `sparql_construct_jsonld()` takes a SPARQL CONSTRUCT query and a JSON-LD frame — a template describing the exact shape of JSON you want — and produces a compact, structured prompt context with no redundant triples, no flat dumps, and no post-processing needed. The frame defines which properties to include, in what order, and how to nest them. The output plugs directly into a system prompt.

---

## Where we're headed

v0.128.1 was an out-of-band safety patch — a router-construction crash in the
`pg_ripple_http` companion, a Docker image default that accepted passwordless
remote database connections, and an async JSON-writeback path that could
report itself enabled without full coverage. v0.129.0 repairs that async
writeback path for real: a wrong dictionary column name had made it silently
non-functional, and enqueue coverage now extends to not-yet-promoted
predicates and main-resident deletes (see "JSON↔RDF mapping registry" above).
v0.131.0 added secure-by-default HTTP startup, typed secret/TLS configuration,
central route authorization, outbound request policy, and versioned conformance
evidence. v0.132.0 added immutable corpus locks, non-skippable required
conformance semantics, feature evidence, route/OpenAPI truth checks, bounded
variable-predicate expansion, keyset exports, and a complete release evidence
bundle. v0.134.0 adds direct, bounded HTTP streaming for JSON, CSV, TSV, and
N-Triples responses, with typed RDF terms, backpressure, deadlines,
cancellation, and connection cleanup. See
[roadmap/v0.134.0.md](roadmap/v0.134.0.md).

v0.135.0 adds typed initial bindings for the SQL and HTTP query APIs, governed
prefix resolution with strict defaults, and the stable v1 query manifest. See
[roadmap/v0.135.0.md](roadmap/v0.135.0.md).

v0.133.0 added deterministic crash-recovery qualification, backup/
restore and failover checks, resource-pressure diagnostics, and the
`pg_ripple.health()` operations API. The release also keeps the independent migration-graph,
schema-fingerprint, and upgrade recovery checks plus validated writeback
configuration. See
[ROADMAP.md](ROADMAP.md#production-readiness--ga-qualification-v01281--v01370)
for the remaining v0.134.0–v0.137.0 production-readiness sequence ahead of
v1.0.0.

The v0.91.0–v0.111.0 development cycle adds deep reasoning capabilities: proof trees and justification infrastructure (v0.100.0), natural-language explanation of derived facts via LLM or deterministic fallback (v0.101.0), what-if hypothetical inference (v0.102.0), Datalog conflict detection (v0.103.0), versioned domain rule libraries (v0.104.0), guided rule authoring with LLM-backed NL-to-Datalog translation (v0.105.0), first-class temporal fact store with AFTER/BEFORE/DURING operators and CDC integration (v0.106.0–v0.107.0), Bayesian confidence updates with evidence log and derivation-DAG propagation (v0.108.0), neuro-symbolic record linkage with six string-similarity built-ins and a five-stage `resolve_entities()` pipeline (v0.109.0), NS-RL evaluation harness with live ER monitoring stream tables and rule explainability (v0.110.0), and Privacy-Preserving Record Linkage via CLK Bloom-filter encoding and differential-privacy aggregates (v0.111.0). Every row in `pg_ripple.feature_status()` shows `implemented`.

The v0.112.0–v0.126.0 cycle focuses on hardening, correctness, and operational completeness: security audit remediation including annotated unsafe blocks and a wired SHACL validation gate in entity resolution (v0.112.0), 5–10× bulk-load throughput via the default-on COPY path (v0.113.0), HTTP API parity and module decomposition (v0.114.0–v0.116.0), Temporal Allen's interval relations and a privacy budget registry (v0.118.0), OWL `propertyChainAxiom` n-hop chains, SERVICE circuit breaker, and schema-aware NL→SPARQL translation (v0.119.0), PageRank explain trees, admin diagnostic snapshot, tenant quota API, and rule-library federation (v0.120.0), observability hardening and test coverage closure (v0.121.0–v0.123.0), a surgical fix for the SPARQL property-path Cartesian-product bug that inflated compound-path results (v0.124.0), point-in-time named-graph snapshots and a diff API via `graph_at()` / `graph_diff()` (v0.125.0), and encrypted per-endpoint federation credentials with automatic header injection and atomic rotation (v0.126.0).

### v1.0.0 — Production Release

The final milestone: full API and documentation freeze, long-term support commitment, and public production dossier. All conformance suites (SPARQL 1.1, SHACL Core, OWL 2 RL, LUBM) remain required gates; performance regression CI and the release evidence dashboard are mandatory artifacts.

---

## Why pg_ripple?

Most RDF triple stores are standalone systems — separate processes, separate storage, separate administration. pg_ripple takes a different approach: it brings the triple store *into* PostgreSQL.

This means you get:

- **One database** for both your relational data and your knowledge graph
- **PostgreSQL's full toolbox** — MVCC, WAL replication, `pg_dump`/`pg_restore`, `EXPLAIN`, monitoring, connection pooling — all work out of the box
- **No data movement** — your RDF data lives alongside your existing tables; SPARQL queries can coexist with SQL in the same transaction
- **Familiar operations** — any DBA who knows PostgreSQL can operate pg_ripple

### How it compares

> **Note**: pg_ripple features marked "Yes" in the table below are implemented across v0.1.0–v0.133.0. Current conformance status is defined by the versioned release-evidence bundle. Competitor capabilities reflect publicly documented feature sets.

| Capability | pg_ripple | Blazegraph | Virtuoso | Apache Fuseki |
|---|---|---|---|---|
| Runs inside PostgreSQL | Yes | No | No | No |
| SPARQL 1.1 Query | Yes | Yes | Yes | Yes |
| SPARQL 1.1 Update | Yes | Yes | Yes | Yes |
| SHACL validation | Yes (sync + async) | No | No | Plugin |
| Datalog reasoning (RDFS, OWL RL) | Yes | No | Limited | Partial |
| Incremental SPARQL views (IVM) | Yes (via pg_trickle) | No | No | No |
| RDF-star / RDF 1.2 | Yes | No | No | Yes |
| Temporal RDF queries | Yes | No | Limited | No |
| Temporal sequential patterns (WITHIN, SEQUENCE) | Yes | No | No | No |
| Proof trees & justification | Yes | No | No | No |
| Hypothetical (what-if) reasoning | Yes | No | No | No |
| Bayesian confidence updates | Yes | No | No | No |
| Neuro-symbolic record linkage (NS-RL) | Yes | No | No | No |
| Privacy-Preserving Record Linkage (PPRL) | Yes | No | No | No |
| Differential-privacy aggregates | Yes | No | No | No |
| Temporal graph snapshots | Yes | No | No | No |
| Encrypted federation credentials | Yes | No | No | No |
| Horizontal sharding (Citus) | Yes | No | No | No |
| SPARQL Federation | Yes | No | Yes | Yes |
| Named graph access control | Yes (PostgreSQL RLS) | No | ACL | Apache Shiro |
| Full-text search | Yes (PostgreSQL GIN) | Yes | Yes | Yes |
| Backup & replication | PostgreSQL WAL | Custom | Custom | Custom |
| Language | Rust | Java | C | Java |

---

## Architecture

pg_ripple is built from the ground up for performance inside PostgreSQL.

> The diagram below shows the internal pipeline: a query enters as SPARQL text, is optimised, translated to SQL, and executed against the storage layer — all inside a single PostgreSQL session.

```
 SPARQL Query / Update                   HTTP API
        │                                   │
        ▼                                   ▼
 ┌─────────────────┐              ┌──────────────────┐
 │  SPARQL Parser   │              │  pg_ripple_http   │
 │  (spargebra)     │              │  (Rust binary)    │
 └────────┬────────┘              └────────┬─────────┘
          │                                │
          ▼                                │
 ┌─────────────────┐                       │
 │  Algebra         │◄──────────────────────┘
 │  Optimizer       │
 │  · Self-join     │
 │    elimination   │
 │  · Filter        │
 │    pushdown      │
 │  · SHACL hints   │
 └────────┬────────┘
          │
          ▼
 ┌─────────────────┐    ┌──────────────────┐
 │  SQL Generator   │───▶│  PostgreSQL       │
 │  (integer joins) │    │  Executor (SPI)   │
 └─────────────────┘    └────────┬─────────┘
                                 │
                    ┌────────────┴────────────┐
                    │                         │
              ┌─────▼─────┐           ┌───────▼──────┐
              │ VP Tables  │           │  Dictionary   │
              │ (per-      │           │  (XXH3-128    │
              │ predicate) │           │   → i64)      │
              │            │           │              │
              │ Delta      │           │  Sharded LRU │
              │ (writes)   │           │  Cache (shmem)│
              │ Main       │           └──────────────┘
              │ (reads)    │
              └────────────┘
```

### How data is stored

- **Compact IDs for everything**: every value — URIs, labels, literals — is assigned a short integer ID. Internal joins use these integers, not raw strings, which keeps storage small and queries fast.
- **One table per relationship type**: facts about `worksAt`, `knows`, `birthDate`, etc. are stored in separate tables. A query asking only about `worksAt` scans only that table, not your entire dataset.
- **Separate lanes for reads and writes**: new data goes into a fast "delta" area; a background worker continuously moves it to an optimised "main" area. Heavy insert workloads and complex queries never slow each other down.

### Performance targets

| Operation | Target | At scale |
|---|---|---|
| Bulk load | >100,000 facts/sec | Batch import with deferred indexing |
| Transactional insert | >10,000 facts/sec | Delta partition, async validation |
| Simple query | <5 ms | 10 million facts |
| Multi-hop query (5 patterns) | <20 ms | 10 million facts |
| Deep path traversal (depth 10) | <100 ms | 10 million facts |
| Dictionary lookup (cache hit) | <1 μs | Sharded in-memory cache |

---

## Technology Stack

| Component | Technology |
|---|---|
| Language | Rust (Edition 2024) |
| PostgreSQL binding | [pgrx](https://github.com/pgcentralfoundation/pgrx) 0.18 |
| PostgreSQL version | 18.x |
| SPARQL parser | [spargebra](https://crates.io/crates/spargebra) — W3C-compliant SPARQL 1.1 algebra |
| SPARQL optimizer | [sparopt](https://crates.io/crates/sparopt) — first-pass algebra optimizer (filter pushdown, constant folding) |
| RDF parsers | [rio_turtle](https://crates.io/crates/rio_turtle), [rio_xml](https://crates.io/crates/rio_xml) — Turtle, N-Triples, RDF/XML; [oxttl](https://crates.io/crates/oxttl) / [oxrdf](https://crates.io/crates/oxrdf) — RDF-star / Turtle-star |
| Hashing | [xxhash-rust](https://crates.io/crates/xxhash-rust) (XXH3-128) — fast non-cryptographic hash for dictionary dedup |
| Serialization | [serde](https://crates.io/crates/serde) + [serde_json](https://crates.io/crates/serde_json) — SHACL reports, SPARQL results, config |
| HTTP server | [axum](https://crates.io/crates/axum) (built on [tokio](https://tokio.rs/)) — SPARQL Protocol HTTP endpoint (`pg_ripple_http` binary) |
| PG client (HTTP service) | [tokio-postgres](https://crates.io/crates/tokio-postgres) + [deadpool-postgres](https://crates.io/crates/deadpool-postgres) — async connection pool from HTTP service to PostgreSQL |
| HTTP client (federation) | [ureq](https://crates.io/crates/ureq) 2.12 — outbound calls to remote SPARQL endpoints (`SERVICE` keyword); connection-pooled `Agent` per backend session |
| IVM / stream tables | [pg_trickle](https://github.com/trickle-labs/pg-trickle) *(optional companion extension)* — incremental SPARQL views, ExtVP, live statistics |
| Dictionary cache | [lru](https://crates.io/crates/lru) — backend-local LRU cache (v0.1.0–v0.5.1); replaced by sharded shared-memory map in v0.6.0 |
| Error handling | [thiserror](https://crates.io/crates/thiserror) — typed error enums with PT error code constants (PT001–PT799) |
| Testing | pgrx `#[pg_test]`, `cargo pgrx regress`, [proptest](https://crates.io/crates/proptest), [cargo-fuzz](https://crates.io/crates/cargo-fuzz) |

---

## pg_trickle dependency matrix

[pg_trickle](https://github.com/trickle-labs/pg-trickle) is an optional companion extension.
The table below shows which pg_ripple features require pg_trickle and which ship standalone.

| Feature | Ships standalone | Requires pg_trickle |
|---|---|---|
| SPARQL SELECT / ASK / CONSTRUCT / DESCRIBE | ✓ | — |
| SPARQL UPDATE (INSERT/DELETE/CLEAR/LOAD) | ✓ | — |
| Property paths (ZeroOrMorePath, InversePath, …) | ✓ | — |
| Federation (SERVICE, parallel, cost-based) | ✓ | — |
| SHACL validation (Core + SPARQL constraints) | ✓ | — |
| Datalog rules (RDFS/OWL RL, seminaïve, magic sets) | ✓ | — |
| HTAP merge worker | ✓ | — |
| Bulk load (Turtle / N-Triples / RDF-XML) | ✓ | — |
| GeoSPARQL 1.1 (geof:distance, ST_DWithin, …) | ✓ | — |
| Full-text search (RDF-FTS) | ✓ | — |
| Vector + SPARQL hybrid search | ✓ | — |
| pg_dump / pg_restore round-trip | ✓ | — |
| CDC subscriptions (NOTIFY on triple changes) | ✓ | — |
| Incremental SPARQL views (IVM) | — | ✓ required |
| ExtVP materialised statistics | — | ✓ required |
| Live auto-updating CONSTRUCT views | — | ✓ required |
| Citus rebalance pause/resume during CDC | ✓ (NOTIFY signal) | ✓ (pause/resume logic) |
| Read-replica routing for federation | ✓ | — |

---

## Getting Started

### Prerequisites

- PostgreSQL 18
- Rust stable toolchain (pg_ripple is a compiled extension)
- [pgrx](https://github.com/pgcentralfoundation/pgrx) 0.18

### Build and install

```bash
git clone https://github.com/trickle-labs/pg-ripple.git
cd pg-ripple

# Initialise pgrx for PostgreSQL 18
cargo pgrx init --pg18 $(which pg_config)

# Run tests
cargo pgrx test pg18

# Install into your local PostgreSQL
cargo pgrx install --pg-config $(which pg_config)
```

### Enable the extension

```sql
CREATE EXTENSION pg_ripple;
```



---

## Quality & Testing

pg_ripple is built to production-grade standards:

- **W3C conformance** — 100% pass rate on the official SPARQL 1.1 Query, SPARQL 1.1 Update, and SHACL Core test suites (~3 000 tests, parallelized, complete in under 2 minutes)
- **Apache Jena test suite** — ~1 000 additional tests covering XSD numeric promotions, timezone-aware date/time, blank-node scoping, and all SPARQL string functions
- **WatDiv benchmark** — all 100 WatDiv query templates (star, chain, snowflake, complex) validated for correctness against a 10 M-triple dataset with ±0.1% row-count baselines
- **LUBM conformance suite** — all 14 canonical LUBM queries pass against a synthetic university OWL ontology; includes a Datalog validation sub-suite confirming that `infer('owl-rl')` produces correct supertype entailments (v0.44.0)
- **W3C OWL 2 RL conformance suite** — W3C OWL 2 RL test manifests (entailment, consistency, and inconsistency tests) run in CI; **100% pass rate (66/66) achieved at v0.51.0** — blocking gate in CI (v0.51.0)
- **Property-based testing** — `proptest` suites assert algebraic invariants: SPARQL algebra round-trips produce byte-identical SQL, dictionary encode/decode is always stable and collision-free for 10,000 random distinct terms, JSON-LD framing preserves all matching IRIs (v0.51.0)
- **Extensive test suite** — 300+ pg_regress tests and property-based (`proptest`) suites cover every SQL-exposed function, every feature, and every edge case (as of v0.126.0)
- **Security testing** — resistance to injection attacks, malformed inputs, and resource exhaustion
- **Fuzz testing** — the federation result decoder, query pipeline, and URL host parser are continuously fuzz-tested (nightly, 120 s per target); arbitrary XML/JSON from remote SERVICE endpoints cannot cause a crash or panic (v0.51.0)
- **Performance regression CI** — BSBM benchmark (1M-triple product dataset, 12 explore queries) and automated throughput benchmarks fail the build if performance drops by more than 10% (v0.51.0)
- **Security CI** — `cargo audit --deny warnings` runs on every pull request; SBOM (CycloneDX) generated and attached to every release; GitHub Actions refs pinned to full SHA; Docker release images scanned via Trivy with immutable digest
- **Stability** — 72-hour soak test with published artifacts (memory trend, merge latency, query p50/p95/p99, error counts), memory leak detection, and crash recovery testing (v0.67.0)
- **Upgrade and backup acceptance** — migration chain from all supported 0.x versions, `pg_dump`/restore round trip, and rollback guidance tested in CI (v0.67.0)
- **Public benchmark baselines** — BSBM, WatDiv, LUBM, bulk N-Triples/Turtle load, HTAP merge throughput, construct-rule incremental maintenance, Datalog DRed, vector hybrid search, Arrow IPC export, Citus fan-out, bidi relay throughput, and Magellan ER benchmark (Abt-Buy, DBLP-ACM) published with hardware, dataset size, and raw output; baselines refreshed to v0.126.0

---

## Contributing

Contributions, feedback, and design discussions are welcome. Please open an issue to discuss before submitting a pull request.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE) for details.
