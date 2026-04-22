# Neuro-Symbolic Record Linkage × pg_ripple × pg_trickle: Synergy Analysis

> **Date**: 2026-04-22
> **Status**: Research report
> **Audience**: pg_ripple developers, knowledge-graph practitioners, data-integration architects

---

## Executive Summary

**Neuro-symbolic record linkage** (NS-RL) is an emerging paradigm that fuses neural learning (embedding-based similarity, pre-trained language models) with symbolic reasoning (ontology axioms, logical rules, formal constraints) to identify records across heterogeneous data sources that refer to the same real-world entity. The approach overcomes the brittleness of purely rule-based systems and the opacity of purely neural matchers by combining the best of both worlds: neural models provide high-recall fuzzy similarity, while symbolic components enforce hard constraints, inject domain knowledge, and guarantee explainable decisions.

**pg_ripple** — a PostgreSQL 18 extension implementing a high-performance RDF triple store with native SPARQL, Datalog, SHACL, OWL 2 RL reasoning, pgvector hybrid search, and RDF-star provenance — is exceptionally well-positioned to serve as the runtime platform for NS-RL pipelines.

**pg_trickle** — a companion PostgreSQL 18 extension providing declarative, automatically-refreshing materialized views (stream tables) powered by Incremental View Maintenance (IVM) — adds a critical missing dimension: **real-time reactivity**. Where pg_ripple provides the symbolic and neural reasoning substrate, pg_trickle makes it *live*, turning batch entity resolution into a continuous, event-driven pipeline that reacts to data changes within milliseconds.

Together, the two extensions form a **complete, in-database NS-RL platform** with no external orchestration required. This report identifies 18 concrete synergies across the three-way intersection and proposes an end-to-end architecture.

---

## Table of Contents

1. [What Is Neuro-Symbolic Record Linkage?](#1-what-is-neuro-symbolic-record-linkage)
2. [Background: Record Linkage Fundamentals](#2-background-record-linkage-fundamentals)
3. [The Neuro-Symbolic Paradigm](#3-the-neuro-symbolic-paradigm)
4. [Key Research and Systems](#4-key-research-and-systems)
5. [pg_ripple Capability Map](#5-pg_ripple-capability-map)
6. [pg_trickle Capability Map](#6-pg_trickle-capability-map)
7. [Synergy Analysis: pg_ripple × NS-RL](#7-synergy-analysis-pg_ripple--ns-rl)
8. [Synergy Analysis: pg_trickle × NS-RL](#8-synergy-analysis-pg_trickle--ns-rl)
9. [Synergy Analysis: pg_ripple × pg_trickle × NS-RL](#9-synergy-analysis-pg_ripple--pg_trickle--ns-rl)
10. [End-to-End NS-RL Architecture](#10-end-to-end-ns-rl-architecture)
11. [Worked Examples](#11-worked-examples)
12. [Competitive Landscape](#12-competitive-landscape)
13. [Gaps and Future Work](#13-gaps-and-future-work)
14. [References](#14-references)

---

## 1. What Is Neuro-Symbolic Record Linkage?

**Record linkage** (also called entity resolution, deduplication, entity matching, or identity resolution) is the task of finding records across one or more data sources that refer to the same real-world entity. It is a foundational problem in data integration, master data management, fraud detection, healthcare informatics, and knowledge graph construction.

**Neuro-symbolic record linkage** combines two complementary approaches:

| Component | Role | Strengths | Weaknesses |
|-----------|------|-----------|------------|
| **Neural** (embeddings, PLMs, GNNs) | Learn fuzzy similarity from data | High recall, handles variation and noise, transfers across domains | Opaque decisions, no hard-constraint enforcement, needs labeled data |
| **Symbolic** (OWL axioms, Datalog rules, SHACL constraints) | Encode domain knowledge as logical rules | Explainable, guarantees correctness invariants, zero-shot for known patterns | Brittle to noise, cannot handle unseen variation, combinatorial scaling |

The key insight of NS-RL is that these weaknesses are **complementary**: neural models handle the "long tail" of fuzzy variation that rules cannot anticipate, while symbolic rules enforce hard constraints that neural models may violate (e.g., "two entities with different social security numbers cannot be the same person").

---

## 2. Background: Record Linkage Fundamentals

### 2.1 The Classical Pipeline

Record linkage follows a well-established pipeline:

```
┌─────────────┐    ┌────────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│   Data       │───▶│  Blocking   │───▶│  Pairwise     │───▶│ Classification│───▶│ Canonicali-  │
│   Ingestion  │    │  (Candidate │    │  Comparison   │    │ (Match /      │    │ zation       │
│              │    │   Pairing)  │    │              │    │  Non-match)   │    │              │
└─────────────┘    └────────────┘    └──────────────┘    └──────────────┘    └──────────────┘
```

1. **Data Ingestion**: Load records from heterogeneous sources, normalize, standardize.
2. **Blocking**: Reduce the quadratic comparison space $O(n^2)$ to a manageable set of candidate pairs by grouping records that share a coarse key (e.g., same first letter of surname, same postal code).
3. **Pairwise Comparison**: Compute similarity features between candidate pairs (string similarity, phonetic codes, numeric distance, date proximity).
4. **Classification**: Decide match / non-match / possible-match based on feature vectors.
5. **Canonicalization**: Merge matched records into a single canonical entity, resolve attribute conflicts.

### 2.2 The Fellegi–Sunter Foundation

The mathematical foundation was laid by Fellegi and Sunter (1969). For record pair $(a, b)$, a comparison vector $\gamma = (\gamma_1, \ldots, \gamma_K)$ encodes agreement/disagreement on $K$ attributes. The match weight is:

$$w(\gamma) = \log_2 \frac{m(\gamma)}{u(\gamma)}$$

where $m(\gamma) = P(\gamma \mid (a,b) \in M)$ and $u(\gamma) = P(\gamma \mid (a,b) \in U)$. Pairs with composite weight above an upper threshold are declared matches; below a lower threshold, non-matches; between the two, possible matches requiring human review.

### 2.3 Limitations of Classical Approaches

| Limitation | Impact |
|-----------|--------|
| **Conditional independence assumption** | The Fellegi–Sunter model assumes attribute comparisons are independent given match status — rarely true in practice |
| **Manual feature engineering** | Practitioners must hand-craft similarity functions for each attribute type |
| **Rule maintenance burden** | Deterministic linkage rules become unmanageable as data complexity grows |
| **No structural context** | Classical methods compare records in isolation, ignoring graph neighborhood |
| **No cross-source reasoning** | Cannot leverage ontological knowledge (e.g., "email is functionally unique") as hard constraints |
| **Batch-only processing** | No mechanism for continuous, event-driven entity resolution as new records arrive |

---

## 3. The Neuro-Symbolic Paradigm

### 3.1 Neural Components

Modern neural approaches to entity matching use:

**Pre-trained Language Models (PLMs)**: Systems like Ditto (Li et al., VLDB 2021) serialize record pairs as text sequences and fine-tune BERT/RoBERTa for binary classification. This captures deep semantic similarity ("Bill" ≈ "William") without manual feature engineering. Ditto achieves up to 29% F1 improvement over prior SOTA on standard benchmarks.

**Knowledge Graph Embeddings (KGE)**: Methods like TransE, RotatE, and ComplEx learn dense vector representations of entities and relations. Entity pairs with high embedding similarity are match candidates. This is especially powerful for cross-lingual and cross-schema matching.

**Graph Neural Networks (GNNs)**: R-GCN and CompGCN aggregate neighborhood information, so structurally similar entities (similar neighbors, similar relation patterns) have similar representations even if their attribute values differ.

**Contrastive Learning**: Self-supervised approaches learn entity representations by contrasting positive (same-entity) and negative (different-entity) pairs, reducing dependence on labeled data.

### 3.2 Symbolic Components

The symbolic side of NS-RL draws on:

**OWL 2 RL Axioms**: `owl:FunctionalProperty` and `owl:InverseFunctionalProperty` provide deterministic matching rules. If `ex:taxId` is declared an inverse-functional property, then any two entities sharing the same tax ID are necessarily the same entity — no neural scoring needed.

**Datalog Rules**: Custom blocking and matching rules expressed in Datalog can encode complex domain logic:
```prolog
% Block: only compare entities in the same postal region
candidate(?x, ?y) :- ?x ex:postalCode ?z, ?y ex:postalCode ?z, ?x != ?y.

% Match: shared inverse-functional property implies identity
?x owl:sameAs ?y :- ?x ex:taxId ?id, ?y ex:taxId ?id, ?x != ?y.
```

**SHACL Constraints**: Shape constraints act as validation gates: a proposed `owl:sameAs` link that would violate `sh:disjoint` (e.g., merging a Person with an Organization) is rejected, regardless of the neural model's confidence.

**Ontology Alignment**: OWL axioms like `owl:equivalentClass` and `owl:equivalentProperty` enable cross-schema matching: if Source A uses `schema:name` and Source B uses `foaf:name`, an `owl:equivalentProperty` declaration allows the system to compare them without manual mapping.

### 3.3 Interaction Patterns

There are four main patterns for combining neural and symbolic components:

| Pattern | Description | Example |
|---------|-------------|---------|
| **Neural→Symbolic** | Neural model proposes candidates; symbolic rules validate | Embedding similarity finds candidates; `owl:InverseFunctionalProperty` confirms; `sh:disjoint` rejects invalid merges |
| **Symbolic→Neural** | Symbolic rules generate blocking candidates; neural model scores | Datalog blocking rules restrict comparison space; BERT-based matcher scores pairs |
| **Interleaved** | Alternating neural and symbolic steps in a fixpoint loop | Neural embeddings propose sameAs → OWL RL inference propagates → new triples update embeddings → repeat |
| **Joint** | Single model with differentiable logic and neural components | Logical Neural Networks (LNN) — differentiable first-order logic gates with learned weights (IBM Research) |

### 3.4 Key Research Contributions

**LNN-EL** (Jiang et al., ACL 2021) — A neuro-symbolic entity linking system using Logical Neural Networks. First-order logic rules with learned real-valued weights achieve competitive performance with black-box neural approaches while providing interpretable, transferable decision rules. Demonstrated 3%+ F1 improvement on LC-QuAD-1.0.

**Ditto** (Li et al., VLDB 2021) — Deep entity matching with pre-trained language models. Achieves 96.5% F1 on real-world company matching (789K × 412K records). Domain knowledge injection via highlighting important input tokens bridges toward neuro-symbolic integration.

**Entity Resolution with Markov Logic** (Singla and Domingos, ICDM 2006) — Pioneered the use of probabilistic first-order logic for entity resolution, combining logical rules with probabilistic weights in a unified framework.

**Ontology Embedding** (Chen et al., TKDE 2025) — Comprehensive survey of methods that embed OWL ontologies into vector spaces for entity resolution, query answering, and knowledge retrieval. Bridges the OWL reasoning world with neural embedding spaces.

**The Ontological Compliance Gateway** (van Hurne et al., 2026) — A neuro-symbolic architecture using formal ontologies and knowledge graphs for entity resolution in the context of verifiable agentic AI, demonstrating the trend toward NS-RL in production systems.

**Semantic Web: Past, Present, and Future** (Scherp et al., TGDK 2024) — Hypothesizes that advances in neuro-symbolic AI will revitalize entity resolution in the Semantic Web, with knowledge graphs serving as the integration substrate.

---

## 4. Key Research and Systems

### 4.1 Taxonomy of NS-RL Approaches

```
Neuro-Symbolic Record Linkage
├── Embedding-Based Blocking
│   ├── KGE blocking (TransE, RotatE similarity)
│   ├── PLM blocking (BERT CLS-token similarity)
│   └── GNN blocking (R-GCN neighborhood embedding)
├── Neural Matching with Symbolic Constraints
│   ├── Ditto + domain rules (token highlighting)
│   ├── PLM matcher + OWL validation gate
│   └── Contrastive learning + SHACL shape compliance
├── Symbolic Matching with Neural Features
│   ├── Datalog rules with embedding-distance predicates
│   ├── Fellegi–Sunter with neural feature extractors
│   └── SHACL + pgvector similarity thresholds
├── Logical Neural Networks
│   ├── LNN-EL (entity linking)
│   ├── Differentiable Datalog
│   └── Neural Theorem Provers for RL
└── Knowledge Graph Alignment
    ├── Cross-KG entity alignment (MTransE, AliNet)
    ├── Ontology matching (LogMap, AML)
    └── Federated entity resolution (SERVICE + remote KGs)
```

### 4.2 Tools and Frameworks

| System | Neural | Symbolic | Reactive (IVM) | Integration |
|--------|--------|----------|-----------------|-------------|
| **Ditto** (Megagon Labs) | BERT/RoBERTa fine-tuning | Domain knowledge injection | ❌ Batch only | Neural→Symbolic |
| **LNN** (IBM Research) | Differentiable logic gates | First-order logic rules | ❌ Batch only | Joint |
| **Magellan/DeepMatcher** (UW–Madison) | Deep learning matchers | Rule-based blocking | ❌ Batch only | Symbolic→Neural |
| **Splink** (MoJ, UK) | — | Probabilistic Fellegi–Sunter | ❌ Batch only | Classical |
| **LogMap** (OAEI) | Word embeddings | OWL reasoning | ❌ Batch only | Neural→Symbolic |
| **pg_ripple + pg_trickle** | pgvector HNSW, hybrid RRF | OWL 2 RL, Datalog, SHACL | ✅ Real-time IVM | Full-stack NS-RL |

---

## 5. pg_ripple Capability Map

pg_ripple provides a remarkably complete set of building blocks for NS-RL:

### 5.1 Data Ingestion and Representation

| NS-RL Requirement | pg_ripple Capability | Status |
|-------------------|---------------------|--------|
| Multi-source data loading | `load_turtle()`, `load_ntriples()`, `load_rdfxml()`, `load_jsonld()` with named graphs | ✅ Shipped |
| Schema-agnostic storage | VP (Vertical Partitioning) tables with dictionary encoding | ✅ Shipped |
| Cross-source isolation | Named graphs (`g BIGINT` column in every VP table) | ✅ Shipped |
| Unique entity identifiers | XXH3-128 dictionary encoding (IRI → i64) | ✅ Shipped |
| High-throughput ingestion | HTAP delta/main architecture, batch `ON CONFLICT DO NOTHING` | ✅ Shipped |

### 5.2 Blocking (Candidate Pair Generation)

| NS-RL Requirement | pg_ripple Capability | Status |
|-------------------|---------------------|--------|
| Rule-based blocking | Datalog rules for custom blocking predicates | ✅ Shipped |
| Embedding-based blocking | pgvector HNSW k-NN self-join | ✅ Shipped |
| Hybrid blocking (rules + embeddings) | `hybrid_search()` with Reciprocal Rank Fusion | ✅ Shipped |
| Demand-filtered blocking | `infer_demand()` restricts inference to relevant predicates | ✅ Shipped (v0.31.0) |
| Federated blocking | SPARQL `SERVICE` joins local entities with remote reference KGs | ✅ Shipped |

### 5.3 Classification (Match Decision)

| NS-RL Requirement | pg_ripple Capability | Status |
|-------------------|---------------------|--------|
| Deterministic matching (shared unique key) | `owl:InverseFunctionalProperty` → automatic `owl:sameAs` inference | ✅ Shipped |
| Functional property uniqueness | `owl:FunctionalProperty` → `owl:sameAs` from shared object values | ✅ Shipped |
| Probabilistic matching (confidence scores) | RDF-star: `<< ex:A owl:sameAs ex:B >> ex:confidence 0.87` | ✅ Shipped |
| Constraint-based validation gate | SHACL shapes reject invalid merges (`sh:disjoint`, `sh:class`) | ✅ Shipped |
| Transitive closure | `owl:sameAs` transitivity via OWL 2 RL built-in rules | ✅ Shipped |

### 5.4 Canonicalization (Entity Merging)

| NS-RL Requirement | pg_ripple Capability | Status |
|-------------------|---------------------|--------|
| Equivalence class computation | Union-find over `owl:sameAs` pairs (lowest-ID canonical) | ✅ Shipped (v0.31.0) |
| Transparent query rewriting | SPARQL queries on non-canonical aliases → automatic redirect | ✅ Shipped |
| Cluster size safety | PT550 warning when equivalence class exceeds `sameas_max_cluster_size` | ✅ Shipped (v0.42.0) |
| Pre-embedding canonicalization | Canonical entities before `embed_entities()` | ✅ Shipped |

### 5.5 Provenance and Explainability

| NS-RL Requirement | pg_ripple Capability | Status |
|-------------------|---------------------|--------|
| Linkage provenance | RDF-star: annotate `owl:sameAs` with source model, confidence, timestamp | ✅ Shipped |
| Explicit vs. inferred triples | `source` column in VP tables: `0` = explicit, `1` = inferred | ✅ Shipped |
| Named graph isolation | Predicted links in a separate named graph from ground truth | ✅ Shipped |
| Linkage rule explanation | Datalog `explain()` traces derivation chains | ✅ Shipped |
| JSON-LD export for audit | `export_jsonld()` with custom framing for compliance reports | ✅ Shipped |

---

## 6. pg_trickle Capability Map

pg_trickle is a PostgreSQL 18 extension (Rust/pgrx) that provides **declarative, automatically-refreshing materialized views** — called *stream tables* — powered by Incremental View Maintenance (IVM). When base-table rows change, pg_trickle computes only the delta, not the full result.

### 6.1 Core Capabilities Relevant to NS-RL

| Capability | Description | NS-RL Relevance |
|-----------|-------------|-----------------|
| **Incremental View Maintenance** | Only changed rows processed (5–90× faster than full recompute at 1% change rate) | Entity resolution results stay fresh without full recomputation |
| **IMMEDIATE mode** | Stream table updated within the same transaction as the DML | Validate SHACL constraints and detect sameAs candidates in real time |
| **DAG-aware scheduling** | Stream tables depending on other stream tables refresh in topological order | Multi-stage NS-RL pipeline stages refresh in correct order |
| **Diamond consistency** | Diamond-shaped DAGs refreshed atomically | Blocking → matching → validation DAGs produce consistent results |
| **Full SQL coverage** | JOINs, aggregates, window functions, `WITH RECURSIVE`, EXISTS, LATERAL | Handles all SPARQL→SQL patterns including property paths |
| **Hybrid CDC** | Trigger-based (default) or WAL-based change capture | Captures triple insertions/deletions with low overhead |
| **Adaptive fallback** | Switches DIFFERENTIAL→FULL when change rate exceeds threshold | Handles both steady-state trickle and bulk-load bursts |
| **Change buffer compaction** | Cancelling INSERT/DELETE pairs collapsed automatically | Efficient handling of sameAs propagation's insert-delete churn |

### 6.2 Existing pg_ripple × pg_trickle Integrations

pg_ripple already integrates with pg_trickle in several areas (optional at runtime — degrades gracefully when pg_trickle is absent):

| Integration | Version | Description |
|-------------|---------|-------------|
| **Live statistics** | v0.6.0 | `_pg_ripple.vp_cardinality` stream table for real-time per-predicate row counts |
| **VP promotion detection** | v0.6.0 | `_pg_ripple.rare_predicate_candidates` IMMEDIATE stream table detects promotion threshold crossing |
| **Subject pattern index** | v0.6.0 | `_pg_ripple.subject_patterns` maintained as stream table between merge cycles |
| **SHACL violation monitors** | v0.7.0 | Per-shape IMMEDIATE stream tables for in-transaction constraint validation |
| **SHACL DAG validation** | v0.8.0 | Multi-shape validation compiled into per-shape stream tables with DAG-leaf aggregation |
| **Schema summary** | v0.14.0 | `_pg_ripple.inferred_schema` live schema summary via stream table |
| **SPARQL views** | v0.15.0+ | `create_sparql_view()` compiles SPARQL to pg_trickle stream tables |
| **Datalog views** | v0.15.0+ | `create_datalog_view()` materializes Datalog goal queries as stream tables |
| **Dictionary hot cache** | v0.15.0 | `_pg_ripple.dictionary_hot` maintained incrementally |
| **Materialized Datalog** | v0.15.0 | Derived predicates materialized as pg_trickle stream tables with stratum ordering |

---

## 7. Synergy Analysis: pg_ripple × NS-RL

### Synergy 1: OWL 2 RL as the Symbolic Backbone

pg_ripple ships 30+ OWL 2 RL inference rules as built-in Datalog programs. The rules most relevant to NS-RL:

```prolog
% Inverse-functional property: shared value → same entity
?x owl:sameAs ?y :- ?x ?p ?o, ?y ?p ?o,
    ?p rdf:type owl:InverseFunctionalProperty, ?x != ?y.

% Functional property: same subject + same property → same object
?o1 owl:sameAs ?o2 :- ?x ?p ?o1, ?x ?p ?o2,
    ?p rdf:type owl:FunctionalProperty, ?o1 != ?o2.

% sameAs symmetry, transitivity, and class propagation
?y owl:sameAs ?x :- ?x owl:sameAs ?y.
?x owl:sameAs ?z :- ?x owl:sameAs ?y, ?y owl:sameAs ?z.
?y rdf:type ?c  :- ?x rdf:type ?c, ?x owl:sameAs ?y.
```

Simply declaring `ex:taxId a owl:InverseFunctionalProperty` and running `SELECT pg_ripple.infer('owl-rl')` automatically discovers all entity pairs that share a tax ID. This is the symbolic half of NS-RL, implemented and production-ready.

### Synergy 2: pgvector Hybrid Search as Neural Blocking

pg_ripple's `hybrid_search()` implements Reciprocal Rank Fusion (RRF):

$$\text{RRF}(d) = \sum_{r \in R} \frac{1}{k + \text{rank}_r(d)}$$

where $k = 60$ and the sum is over a SPARQL-ranked set and a vector-ranked set. This directly serves as NS-RL blocking: SPARQL selects structurally compatible entities (symbolic), pgvector finds embeddings within cosine distance ε (neural), and RRF fuses both.

### Synergy 3: SHACL as the Validation Gate

SHACL shapes serve as hard constraints that filter out invalid linkages proposed by neural models. A proposed `owl:sameAs` link that would result in an entity having two conflicting blood types, or merging a Person with an Organization (`sh:disjoint`), is rejected regardless of neural confidence.

### Synergy 4: RDF-star for Linkage Provenance

Every `owl:sameAs` link can carry provenance metadata:

```turtle
<< ex:Alice owl:sameAs ex:ASmith >>
    ex:confidence    0.92 ;
    ex:source        "RotatE-v2" ;
    ex:matchedOn     "2026-04-22T10:30:00Z"^^xsd:dateTime ;
    ex:matchFeatures "name_sim=0.95, addr_sim=0.88, email_exact=true" .
```

SPARQL queries filter on provenance: `FILTER(?conf > 0.85)`. Deterministic links have confidence 1.0 and source "owl-rl".

### Synergy 5: Union-Find Canonicalization with Safety Guards

The `owl:sameAs` canonicalization engine (v0.31.0) processes all sameAs triples through union-find and rewrites every reference to the lowest-ID canonical representative. The v0.42.0 cluster-size guard (PT550) prevents runaway merges — essential for NS-RL where neural models occasionally produce false positives that transitive closure would amplify exponentially.

### Synergy 6: SPARQL Federation for Cross-Source Enrichment

The `SERVICE` clause enriches local entities with authoritative reference data from Wikidata, DBpedia, or domain-specific endpoints. In NS-RL, federation serves both enrichment (fetch additional blocking attributes) and validation (confirm proposed links against external reference graphs).

### Synergy 7: Dictionary Encoding for Efficient Comparison

All VP table joins operate on integer comparisons (i64) — never string comparisons. Candidate pair generation via integer self-joins, entity set operations, and blocking key comparisons run at native integer speed, even with millions of entities.

---

## 8. Synergy Analysis: pg_trickle × NS-RL

pg_trickle transforms NS-RL from a batch process into a **continuous, event-driven pipeline**. This is where the real differentiation lies.

### Synergy 8: Real-Time sameAs Candidate Detection

The most impactful NS-RL synergy: an IMMEDIATE-mode stream table that detects new `owl:sameAs` candidates **within the same transaction** that inserts a new triple:

```sql
SELECT pgtrickle.create_stream_table(
    name         => '_pg_ripple.sameas_candidates_ifp',
    query        => $$
        -- Detect pairs sharing an inverse-functional property value
        SELECT a.s AS entity1, b.s AS entity2, a.o AS shared_value, v.p AS property_id
        FROM _pg_ripple.vp_rare a
        JOIN _pg_ripple.vp_rare b ON a.p = b.p AND a.o = b.o AND a.s < b.s
        JOIN _pg_ripple.predicates v ON v.id = a.p
        WHERE NOT EXISTS (
            SELECT 1 FROM _pg_ripple.vp_rare sa
            WHERE sa.p = :sameas_pred_id AND sa.s = a.s AND sa.o = b.s
        )
    $$,
    refresh_mode => 'IMMEDIATE'
);
```

**Impact**: When a new record arrives with `ex:taxId "12345"` and another record already has the same tax ID, the stream table immediately contains a candidate row — no waiting for a batch inference run. The merge worker or an application trigger can act on it within the same transaction.

### Synergy 9: Continuous SHACL Violation Monitoring for Merge Validation

SHACL constraint checks compiled into IMMEDIATE stream tables validate merge proposals in real time:

```sql
-- Detect merges that would create conflicting blood types
SELECT pgtrickle.create_stream_table(
    name         => '_pg_ripple.shacl_merge_conflicts',
    query        => $$
        SELECT sa.s AS entity1, sa.o AS entity2,
               bt1.o AS blood_type_1, bt2.o AS blood_type_2
        FROM _pg_ripple.vp_sameas sa
        JOIN _pg_ripple.vp_bloodtype bt1 ON bt1.s = sa.s
        JOIN _pg_ripple.vp_bloodtype bt2 ON bt2.s = sa.o
        WHERE bt1.o != bt2.o
    $$,
    refresh_mode => 'IMMEDIATE'
);
-- Non-empty table = invalid merge detected in real time
```

### Synergy 10: DAG-Ordered NS-RL Pipeline

A multi-stage NS-RL pipeline maps naturally to pg_trickle's DAG scheduler:

```
Triple inserts (base VP tables)
    │
    ├── candidate_pairs_symbolic (IMMEDIATE, rule-based blocking)
    ├── candidate_pairs_neural (5s, embedding similarity refresh)
    │
    ├── merged_candidates (5s, UNION of symbolic + neural candidates)
    │       │
    │       ├── shacl_validation (IMMEDIATE, constraint check)
    │       │       │
    │       │       └── validated_sameas (5s, links passing validation)
    │       │               │
    │       │               └── sameas_provenance (5s, RDF-star metadata)
    │
    └── entity_statistics (30s, per-entity attribute counts for dashboard)
```

pg_trickle's DAG-aware scheduler ensures these stages refresh in topological order. Diamond-shaped dependencies (e.g., both symbolic and neural candidates feeding into merged_candidates) are refreshed atomically.

### Synergy 11: Incremental Canonicalization Statistics

The union-find canonicalization pass benefits from always-current statistics:

```sql
SELECT pgtrickle.create_stream_table(
    name     => '_pg_ripple.sameas_cluster_sizes',
    query    => $$
        WITH RECURSIVE closure(canon, member) AS (
            SELECT LEAST(s, o), GREATEST(s, o) FROM _pg_ripple.vp_sameas
            UNION
            SELECT c.canon, sa.o FROM closure c
            JOIN _pg_ripple.vp_sameas sa ON sa.s = c.member
        )
        SELECT canon, COUNT(*) AS cluster_size
        FROM closure
        GROUP BY canon
    $$,
    schedule => '30s'
);
```

The PT550 cluster-size guard can read from this stream table instead of recomputing the full union-find on every inference run.

### Synergy 12: Live Entity Resolution Dashboard

Stream tables power real-time NS-RL monitoring:

```sql
-- Resolution pipeline status — always fresh
SELECT pgtrickle.create_stream_table(
    name     => 'pg_ripple.resolution_dashboard',
    query    => $$
        SELECT
            (SELECT COUNT(*) FROM _pg_ripple.sameas_candidates_ifp) AS pending_candidates,
            (SELECT COUNT(*) FROM _pg_ripple.shacl_merge_conflicts) AS blocked_merges,
            (SELECT COUNT(*) FROM _pg_ripple.vp_sameas) AS total_sameas_links,
            (SELECT COUNT(DISTINCT canon) FROM _pg_ripple.sameas_cluster_sizes) AS entity_clusters,
            (SELECT MAX(cluster_size) FROM _pg_ripple.sameas_cluster_sizes) AS largest_cluster
    $$,
    schedule => '10s'
);
```

### Synergy 13: Adaptive Blocking Refresh

pg_trickle's adaptive fallback handles the two RL operating modes gracefully:

- **Steady state** (trickle of new records): DIFFERENTIAL mode processes only changed rows — 5–90× faster than full recompute
- **Bulk import** (ETL batch load): When change rate exceeds 50%, pg_trickle automatically switches to FULL recompute, then reverts to DIFFERENTIAL when the storm passes

No manual mode switching or pipeline reconfiguration needed.

---

## 9. Synergy Analysis: pg_ripple × pg_trickle × NS-RL

These synergies arise specifically from the **three-way** intersection — capabilities that neither extension provides alone.

### Synergy 14: Live SPARQL Views for Entity Resolution Queries

pg_ripple compiles SPARQL to SQL; pg_trickle materializes that SQL as a stream table. The result: a SPARQL entity resolution query that stays fresh automatically:

```sql
SELECT pg_ripple.create_sparql_view(
    name     => 'unresolved_entities',
    sparql   => $$
        SELECT ?entity ?name ?source WHERE {
            ?entity a ex:Customer ;
                    ex:name ?name ;
                    ex:source ?source .
            FILTER NOT EXISTS {
                ?entity owl:sameAs ?other .
            }
        }
    $$,
    schedule => '5s'
);

-- Always-fresh list of unresolved entities — simple table scan
SELECT * FROM unresolved_entities;
```

**Impact**: The entity resolution monitoring query — which would normally be a multi-join VP table scan with dictionary decoding — becomes a sub-millisecond table scan, refreshed incrementally.

### Synergy 15: Materialized Datalog Resolution Rules

Datalog entity resolution rules, materialized via pg_trickle stream tables:

```sql
-- Define blocking + matching rules
SELECT pg_ripple.load_rules('
    candidate(?x, ?y) :- ?x ex:postalCode ?z, ?y ex:postalCode ?z,
        ?x rdf:type ex:Customer, ?y rdf:type ex:Customer, ?x != ?y.
    match(?x, ?y) :- candidate(?x, ?y), ?x ex:email ?e, ?y ex:email ?e.
', 'entity_resolution');

-- Materialize as a pg_trickle stream table — always fresh
SELECT pg_ripple.create_datalog_view(
    name      => 'er_matches',
    rule_set  => 'entity_resolution',
    goal      => 'match(?x, ?y)',
    schedule  => '5s'
);
```

When a new customer record arrives, the Datalog view updates incrementally within 5 seconds. The matching results are available as a plain table.

### Synergy 16: Federation Health-Gated Entity Enrichment

pg_trickle maintains a live `_pg_ripple.federation_health` stream table. The NS-RL pipeline can gate cross-source enrichment on endpoint health:

```sql
-- Only enrich from healthy endpoints
SELECT pg_ripple.create_sparql_view(
    name     => 'enriched_entities',
    sparql   => $$
        SELECT ?entity ?wikidata_label WHERE {
            ?entity ex:sameAs ?wd_item .
            SERVICE <https://query.wikidata.org/sparql> {
                ?wd_item rdfs:label ?wikidata_label .
                FILTER(LANG(?wikidata_label) = "en")
            }
        }
    $$,
    schedule => '60s'
);
```

The federation executor skips unhealthy endpoints (success_rate < 10%) automatically, preventing timeout-induced stalls in the enrichment pipeline.

### Synergy 17: ExtVP for Entity Resolution Join Acceleration

Extended Vertical Partitioning (ExtVP) via pg_trickle stream tables pre-computes frequently-needed semi-joins:

```sql
-- Pre-computed: entities that have both a name AND an email
SELECT pgtrickle.create_stream_table(
    name  => '_pg_ripple.extvp_name_email_ss',
    query => $$
        SELECT n.s AS entity_id, n.o AS name_id, e.o AS email_id
        FROM _pg_ripple.vp_name n
        WHERE EXISTS (SELECT 1 FROM _pg_ripple.vp_email e WHERE e.s = n.s)
    $$,
    schedule => '10s'
);
```

Entity resolution blocking queries that filter on "entities with both name and email" can scan this compact stream table instead of joining two full VP tables — dramatically faster for the common blocking pattern.

### Synergy 18: Confidence-Gated Materialization Pipeline

The full NS-RL pipeline, orchestrated entirely within PostgreSQL:

```sql
-- Stage 1: Neural candidates (batch, external model predictions loaded as RDF-star)
-- Stage 2: Stream table filters by confidence threshold
SELECT pgtrickle.create_stream_table(
    name     => '_pg_ripple.high_confidence_matches',
    query    => $$
        SELECT qt_s AS entity1, qt_o AS entity2, d.value AS confidence
        FROM _pg_ripple.dictionary d
        WHERE d.kind = 5  -- quoted triples
          AND d.qt_p = :sameas_pred_id
          AND d.id IN (
              SELECT s FROM _pg_ripple.vp_confidence
              WHERE o > :threshold_id  -- encoded 0.85
          )
    $$,
    schedule => '10s'
);

-- Stage 3: SHACL validation (IMMEDIATE, runs when Stage 2 updates)
-- Stage 4: Approved links promoted to canonical sameAs graph
```

---

## 10. End-to-End NS-RL Architecture

### 10.1 Architecture Diagram

```
                   ┌─────────────────────────────────────────────────────────┐
                   │         PostgreSQL 18 + pg_ripple + pg_trickle          │
                   │                                                         │
Source A ──RDF──▶  │  ┌───────────┐  ┌────────────────────────────────────┐  │
Source B ──RDF──▶  │  │  Named    │  │ pg_trickle DAG Scheduler          │  │
Source C ──RDF──▶  │  │  Graphs   │  │                                    │  │
                   │  │  (VP      │  │  ┌─────────────┐  ┌────────────┐  │  │
                   │  │  tables)  │──│─▶│ Symbolic    │  │ Neural     │  │  │
                   │  └───────────┘  │  │ Blocking    │  │ Blocking   │  │  │
                   │                 │  │ (IMMEDIATE) │  │ (5s sched) │  │  │
                   │  ┌───────────┐  │  └──────┬──────┘  └─────┬──────┘  │  │
                   │  │ OWL 2 RL  │  │         │               │         │  │
                   │  │ Inference │  │         ▼               ▼         │  │
                   │  │ (Datalog) │  │  ┌──────────────────────────────┐ │  │
                   │  └───────────┘  │  │ Merged Candidates (RRF)     │ │  │
                   │                 │  └──────────────┬───────────────┘ │  │
                   │  ┌───────────┐  │                 │                 │  │
                   │  │ pgvector  │  │                 ▼                 │  │
                   │  │ HNSW     │  │  ┌──────────────────────────────┐ │  │
                   │  │ Embed    │  │  │ SHACL Validation Gate       │ │  │
                   │  └───────────┘  │  │ (IMMEDIATE stream table)    │ │  │
                   │                 │  └──────────────┬───────────────┘ │  │
                   │  ┌───────────┐  │                 │                 │  │
                   │  │ RDF-star  │  │                 ▼                 │  │
                   │  │ Provenance│  │  ┌──────────────────────────────┐ │  │
                   │  └───────────┘  │  │ Validated sameAs Links      │ │  │
                   │                 │  │ (RDF-star provenance)        │ │  │
                   │  ┌───────────┐  │  └──────────────┬───────────────┘ │  │
                   │  │ Union-Find│  │                 │                 │  │
                   │  │ Canon.   │  │                 ▼                 │  │
                   │  └───────────┘  │  ┌──────────────────────────────┐ │  │
                   │                 │  │ Canonical Entity Graph       │ │  │
                   │  ┌───────────┐  │  │ (SPARQL views, JSON-LD)     │ │  │
                   │  │ Federation│  │  └──────────────────────────────┘ │  │
                   │  └───────────┘  └────────────────────────────────────┘  │
                   └─────────────────────────────────────────────────────────┘
```

### 10.2 Data Flow

1. **Ingest**: Records from multiple sources arrive as RDF into named graphs via `load_turtle()`. Each source has its own named graph.

2. **Symbolic blocking (IMMEDIATE)**: pg_trickle IMMEDIATE stream tables detect `owl:InverseFunctionalProperty` matches within the same transaction. Zero latency.

3. **Neural blocking (scheduled)**: Embedding similarity candidates refresh every 5 seconds via pgvector HNSW self-join stream table.

4. **Candidate fusion**: Symbolic and neural candidates merge via Reciprocal Rank Fusion in a downstream stream table.

5. **SHACL validation (IMMEDIATE)**: Proposed merges checked against SHACL shapes — conflicting merges rejected instantly.

6. **Provenance annotation**: Validated links annotated with RDF-star metadata (confidence, source, timestamp).

7. **Canonicalization**: Union-find computes equivalence classes; all references rewritten to canonical IDs.

8. **Serving**: Canonical entities available via SPARQL views (pg_trickle stream tables), JSON-LD export, or federation to downstream systems.

---

## 11. Worked Examples

### 11.1 Healthcare Patient Matching

**Scenario**: Two hospital systems merge. Patients may appear in both with varying name spellings, dates of birth, and identifiers.

```sql
-- Load hospital A and B records
SELECT pg_ripple.load_turtle(:hospital_a_ttl, false, 'urn:hospital:A');
SELECT pg_ripple.load_turtle(:hospital_b_ttl, false, 'urn:hospital:B');

-- Symbolic: SSN is inverse-functional → deterministic matches
SELECT pg_ripple.insert_triple(
    '<https://example.org/ssn>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://www.w3.org/2002/07/owl#InverseFunctionalProperty>'
);
SELECT pg_ripple.load_rules_builtin('owl-rl');
SELECT pg_ripple.infer('owl-rl');

-- Neural: embed patient records for fuzzy matching
SELECT pg_ripple.embed_entities('urn:hospital:A');
SELECT pg_ripple.embed_entities('urn:hospital:B');

-- pg_trickle: live monitoring of unresolved patients
SELECT pg_ripple.create_sparql_view(
    name     => 'unresolved_patients',
    sparql   => $$
        SELECT ?patient ?name ?hospital WHERE {
            ?patient a ex:Patient ; ex:name ?name ; ex:source ?hospital .
            FILTER NOT EXISTS { ?patient owl:sameAs ?other }
        }
    $$,
    schedule => '5s'
);

-- pg_trickle: SHACL guard against conflicting blood types
SELECT pg_ripple.enable_shacl_dag_monitors();

-- Dashboard: always-fresh patient resolution status
SELECT * FROM unresolved_patients;
```

### 11.2 Enterprise Customer Deduplication with Live Monitoring

```sql
-- Define blocking + matching rules
SELECT pg_ripple.load_rules('
    block(?x, ?y) :- ?x ex:postalCode ?z, ?y ex:postalCode ?z,
        ?x rdf:type ex:Customer, ?y rdf:type ex:Customer, ?x != ?y.
    ?x owl:sameAs ?y :- block(?x, ?y), ?x ex:email ?e, ?y ex:email ?e.
', 'customer_er');

-- Materialize as a live Datalog view (pg_trickle stream table)
SELECT pg_ripple.create_datalog_view(
    name      => 'customer_matches',
    rule_set  => 'customer_er',
    goal      => '?x owl:sameAs ?y',
    schedule  => '5s'
);

-- Federation: validate against external business registry
SELECT pg_ripple.create_sparql_view(
    name     => 'registry_confirmed',
    sparql   => $$
        SELECT ?local ?remote WHERE {
            ?local ex:taxId ?tid .
            SERVICE <https://registry.example.com/sparql> {
                ?remote ex:registrationNumber ?tid .
            }
        }
    $$,
    schedule => '60s'
);

-- The customer_matches view updates within 5 seconds of any new customer record
-- The registry_confirmed view refreshes every 60 seconds against the external registry
```

---

## 12. Competitive Landscape

No existing system combines all the NS-RL capabilities that pg_ripple + pg_trickle provide:

| Capability | pg_ripple + pg_trickle | Neo4j | Stardog | Amazon Neptune | Dedicated RL Tools |
|-----------|------------------------|-------|---------|----------------|-------------------|
| OWL 2 RL inference | ✅ Native Datalog | ❌ | ✅ | ❌ | ❌ |
| owl:sameAs canonicalization | ✅ Union-find | ❌ | ✅ | ❌ | ❌ |
| SPARQL 1.1 | ✅ Full | ❌ | ✅ | ✅ | ❌ |
| Datalog with negation + aggregation | ✅ | ❌ | ✅ (limited) | ❌ | ❌ |
| SHACL validation | ✅ Core + extensions | ❌ | ✅ | ✅ | ❌ |
| pgvector embedding search | ✅ HNSW | ❌ | ❌ | ❌ | ❌ |
| Hybrid SPARQL + vector search | ✅ RRF | ❌ | ❌ | ❌ | ❌ |
| RDF-star provenance | ✅ | ❌ | ✅ | Partial | ❌ |
| SPARQL federation | ✅ Cost-based | ❌ | ✅ | ✅ | ❌ |
| **Incremental View Maintenance** | **✅ pg_trickle** | ❌ | ❌ | ❌ | ❌ |
| **Real-time SHACL validation** | **✅ IMMEDIATE mode** | ❌ | ❌ | ❌ | ❌ |
| **Live SPARQL/Datalog views** | **✅ Stream tables** | ❌ | ❌ | ❌ | ❌ |
| **DAG-ordered pipeline refresh** | **✅ Topological** | ❌ | ❌ | ❌ | ❌ |
| **Continuous entity resolution** | **✅ Event-driven** | ❌ | ❌ | ❌ | ❌ |
| PostgreSQL ecosystem | ✅ Full | ❌ | ❌ | ❌ | Varies |
| **Full NS-RL stack** | **✅** | ❌ | Partial | ❌ | Partial |

**Key differentiator**: pg_ripple + pg_trickle is the only system where the entire NS-RL pipeline — from data ingestion through blocking, matching, validation, canonicalization, provenance, and continuous monitoring — executes within a single ACID-compliant PostgreSQL instance, with automatic incremental maintenance. No ETL, no external services, no data movement, no batch scheduling.

---

## 13. Gaps and Future Work

### 13.1 Current Gaps

| Gap | Description | Mitigation |
|-----|-------------|------------|
| **Built-in string similarity** | No native Jaro-Winkler, Soundex, or Metaphone in SPARQL/Datalog | Use PostgreSQL's `pg_trgm` and `fuzzystrmatch` extensions via SQL; expose as custom `pg_extern` |
| **suggest_sameas() function** | Planned HNSW self-join to propose sameAs candidates automatically | Implement as `pg_ripple.suggest_sameas(threshold, k)` using pgvector self-join |
| **Active learning loop** | No built-in human-in-the-loop for labeling uncertain pairs | Expose uncertain pairs (confidence 0.5–0.85) via SPARQL view; feedback loads via `load_turtle()` |
| **Pre-trained RL model** | No bundled PLM for entity matching | Integration point: export pairs → external Ditto/BERT matcher → load predictions back |
| **Privacy-preserving RL** | No Bloom filter or secure multi-party computation for PPRL | Out of scope; defer to application layer |
| **Incremental canonicalization** | Current union-find recomputes from scratch on each inference run | pg_trickle stream table could maintain cluster statistics incrementally |

### 13.2 Recommended Roadmap Items

**Priority 1 — `suggest_sameas(threshold, k)`**:
- Self-join on `_pg_ripple.embeddings` using pgvector HNSW
- Returns candidate pairs with cosine similarity above threshold
- Outputs RDF-star triples with confidence annotations
- Can be backed by a pg_trickle stream table for continuous candidate generation

**Priority 2 — String similarity builtins**:
- Expose `pg_trgm` similarity, `levenshtein()`, `soundex()`, and `metaphone()` as SPARQL FILTER functions
- Enables symbolic matching rules like `FILTER(jaro_winkler(?name1, ?name2) > 0.85)`

**Priority 3 — NS-RL pipeline function**:
- High-level SQL function: `pg_ripple.resolve_entities(source_graph, target_graph, options JSONB)`
- Orchestrates: OWL 2 RL inference → embedding blocking → SHACL validation → canonicalization
- Creates pg_trickle stream tables for each pipeline stage automatically

**Priority 4 — pg_trickle IMMEDIATE mode for sameAs propagation**:
- When a new `owl:sameAs` triple is inserted, IMMEDIATE stream tables propagate the transitive closure and detect cluster-size violations within the same transaction
- Eliminates the current batch-inference latency for sameAs chains

**Priority 5 — Benchmark on standard RL datasets**:
- Load Magellan benchmark datasets (Abt-Buy, Amazon-Google, DBLP-Scholar) as RDF
- Measure precision/recall/F1 against Ditto, DeepMatcher, and Splink baselines
- Track continuous-mode latency metrics via pg_trickle monitoring

---

## 14. References

1. Fellegi, I. P., & Sunter, A. B. (1969). "A Theory for Record Linkage." *Journal of the American Statistical Association*, 64(328), 1183–1210.

2. Singla, P., & Domingos, P. (2006). "Entity Resolution with Markov Logic." *ICDM 2006*, 572–582.

3. Wilson, D. R. (2011). "Beyond Probabilistic Record Linkage: Using Neural Networks and Complex Features to Improve Genealogical Record Linkage." *IJCNN 2011*.

4. Li, Y., Li, J., Suhara, Y., Doan, A., & Tan, W.-C. (2020). "Deep Entity Matching with Pre-Trained Language Models (Ditto)." *VLDB 2021*. arXiv:2004.00584.

5. Jiang, H., Gurajada, S., Lu, Q., Neelam, S., Popa, L., Sen, P., Li, Y., & Gray, A. (2021). "LNN-EL: A Neuro-Symbolic Approach to Short-text Entity Linking." *ACL 2021*, 775–787.

6. Chen, J., Mashkova, O., Zhapa-Camacho, F., et al. (2025). "Ontology Embedding: A Survey of Methods, Applications and Resources." *IEEE TKDE*.

7. Scherp, A., Groener, G., Skoda, P., & Hose, K. (2024). "Semantic Web: Past, Present, and Future." *Transactions on Graph Data and Knowledge*.

8. Hofer, M., Obraczka, D., Saeedi, A., Köpcke, H., & Rahm, E. (2024). "Construction of Knowledge Graphs: Current State and Challenges." *Information*, 15(8), 509.

9. van Hurne, M., Valk, J., de Koning, H., & van der Laan, V. (2026). "The Ontological Compliance Gateway: A Neuro-Symbolic Architecture for Verifiable Agentic AI." Technical Report.

10. Hitzler, P., Ebrahimi, M., & Sarker, M. K. (2024). "Neuro-Symbolic AI and the Semantic Web." *Semantic Web Journal*.

11. Bhuyan, B. P., Ramdane-Cherif, A., & Tomar, R. (2024). "Neuro-Symbolic Artificial Intelligence: A Survey." *Neural Computing and Applications*.

12. Christen, P., Ranbaduge, T., & Schnell, R. (2020). *Linking Sensitive Data: Methods and Techniques for Practical Privacy-Preserving Information Sharing*. Springer.

13. Elmagarmid, A., Ipeirotis, P. G., & Verykios, V. (2007). "Duplicate Record Detection: A Survey." *IEEE TKDE*, 19(1), 1–16.

14. Budnitsky, A., et al. (2023). "DBSP: Automatic Incremental View Maintenance." *VLDB 2023*. arXiv:2203.16684.

---

*This report was prepared as part of the pg_ripple project's research into neuro-symbolic AI integration opportunities. pg_ripple is a PostgreSQL 18 extension implementing a high-performance RDF triple store with native SPARQL query execution. pg_trickle is a companion PostgreSQL 18 extension providing streaming tables with incremental view maintenance.*
