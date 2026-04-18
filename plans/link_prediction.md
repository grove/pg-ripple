# Link Prediction for Knowledge Graphs: Analysis & pg_ripple Integration Strategy

> **Date**: 2026-04-18
> **Status**: Research report
> **Audience**: pg_ripple developers and stakeholders

## Executive Summary

**Link prediction** is the task of predicting missing edges (facts) in a knowledge graph by learning low-dimensional vector representations (embeddings) of entities and relations. It is one of the most actively researched areas in graph machine learning, with direct applications in drug discovery, fraud detection, recommendation systems, and enterprise knowledge management.

For **pg_ripple**, link prediction represents a high-value integration opportunity: pg_ripple already stores knowledge graphs in a queryable, ACID-compliant, SPARQL-enabled format — the ideal substrate for both *training* link prediction models and *consuming* their predictions. By positioning pg_ripple as the **read–train–predict–write** hub for knowledge graph embedding (KGE) workflows, we differentiate decisively from every competing triplestore, property graph database, and vector-only store.

This report provides a comprehensive technical analysis of link prediction, surveys the ecosystem of tools, identifies concrete synergies with pg_ripple's architecture, and proposes an integration strategy with effort estimates.

---

## 1. What Is Link Prediction?

### 1.1 The Problem

A knowledge graph $G = (E, R, T)$ consists of entities $E$, relations $R$, and a set of observed triples $T \subseteq E \times R \times E$. In practice, $T$ is **always incomplete** — real-world knowledge graphs capture only a fraction of the true facts. Link prediction is the task of scoring unobserved triples $(h, r, t) \notin T$ to find those most likely to be true.

Formally: learn a scoring function $f(h, r, t) \rightarrow \mathbb{R}$ such that true triples score higher than false ones.

### 1.2 Why Knowledge Graphs Are Incomplete

| Source of incompleteness | Example |
|---|---|
| **Extraction limitations** | NLP/LLM pipelines miss implicit relationships |
| **Evolving knowledge** | New facts appear faster than the graph is updated |
| **Privacy/access** | Some facts exist but aren't recorded (e.g., internal org relationships) |
| **Cross-document gaps** | Fact A is in document 1, fact B in document 2; the link between them is in neither |
| **Schema design** | The ontology doesn't model certain relationship types |

Real-world incompleteness is substantial: Freebase is estimated to miss 71% of people's places of birth, and 75% of people's nationalities (Dong et al., 2014).

### 1.3 The Core Approach: Knowledge Graph Embeddings (KGE)

The dominant approach maps every entity $e \in E$ and every relation $r \in R$ to a dense vector in $\mathbb{R}^d$ (typically $d \in [100, 1000]$). A model-specific **interaction function** computes the plausibility of each triple:

$$f(h, r, t) = \text{interaction}(\mathbf{h}, \mathbf{r}, \mathbf{t})$$

Training minimizes a loss that pushes the score of observed triples above that of corrupted (negative) triples.

### 1.4 Taxonomy of Embedding Models

#### Translational Distance Models

These interpret relations as geometric translations in the embedding space.

| Model | Interaction | Relation patterns captured | Year |
|---|---|---|---|
| **TransE** | $\|\mathbf{h} + \mathbf{r} - \mathbf{t}\|$ | Composition, inversion | 2013 |
| **TransH** | Projects onto relation-specific hyperplane, then translates | Reflexive | 2014 |
| **TransR** | Entities and relations in separate spaces, projects via matrix | Hierarchy | 2015 |
| **TransD** | Dynamic projection matrices based on entity–relation pairs | Hierarchy | 2015 |
| **RotatE** | $\|\mathbf{h} \circ \mathbf{r} - \mathbf{t}\|$ in complex space (rotation) | Symmetry, antisymmetry, inversion, composition | 2019 |

**RotatE** (Sun et al., ICLR 2019) is the current state-of-the-art in the translational family. It models each relation as a rotation in complex vector space, naturally capturing symmetry ($r$ is 180° rotation), antisymmetry ($r$ is non-180°), inversion ($r_2 = \bar{r}_1$), and composition ($r_3 = r_1 \circ r_2$).

#### Semantic Matching / Factorization Models

These compute similarity via bilinear or tensor operations.

| Model | Interaction | Key property | Year |
|---|---|---|---|
| **RESCAL** | $\mathbf{h}^T \mathbf{M}_r \mathbf{t}$ (full matrix per relation) | Expressiveness; $O(d^2)$ per relation | 2011 |
| **DistMult** | $\sum_i h_i \cdot r_i \cdot t_i$ (diagonal bilinear) | Symmetric relations only | 2014 |
| **ComplEx** | $\text{Re}(\sum_i h_i \cdot r_i \cdot \bar{t}_i)$ in complex space | Symmetric + antisymmetric | 2016 |
| **TuckER** | Tucker decomposition of the 3-way binary tensor | Subsumes several models | 2019 |
| **SimplE** | Average of two CP decompositions (head→tail and tail→head) | Fully expressive in the limit | 2018 |

**ComplEx** and **DistMult** are workhorses: fast training, reasonable accuracy, well-understood behaviour.

#### Neural / Deep Models

| Model | Architecture | Key property | Year |
|---|---|---|---|
| **ConvE** | 2D convolution over reshaped entity+relation embeddings | Captures complex patterns | 2018 |
| **R-GCN** | Graph Neural Network with relation-specific aggregation | Leverages graph structure | 2018 |
| **CompGCN** | Composition-based GCN over the KG | Joint entity+relation learning | 2020 |
| **NodePiece** | Tokenizes entities into anchor-based tokens | Scalable to very large KGs | 2021 |

#### Foundation Models (Emerging, 2023–2025)

| Model | Key innovation | Year |
|---|---|---|
| **ULTRA** | Pre-trained on multiple KGs; zero-shot transfer to unseen graphs | 2024 (ICLR) |
| **InductiveNodePiece** | Inductive LP without retraining on new entities | 2021 |

**ULTRA** (Galkin et al., ICLR 2024) is a landmark: a single pre-trained model that performs zero-shot link prediction on 57 different KGs without fine-tuning, often matching or beating graph-specific baselines. This is the "foundation model" moment for KGE.

### 1.5 Training Protocol

1. **Positive triples**: the observed facts $(h, r, t) \in T$
2. **Negative sampling**: corrupt one of $h$, $r$, or $t$ to generate negatives
   - Basic: uniform random corruption
   - Bernoulli: corruption probability weighted by relation cardinality
   - Self-adversarial: weight negatives by their current score (RotatE's innovation)
3. **Loss function**: margin-ranking, binary cross-entropy, or self-adversarial negative sampling loss (NSSA)
4. **Optimization**: Adam or Adagrad; typical training: 100–1000 epochs on GPU
5. **Evaluation**: rank each test triple against all corrupted alternatives; report **MRR** (Mean Reciprocal Rank), **Hits@1/3/10**, **MR** (Mean Rank)

### 1.6 Standard Benchmarks

| Dataset | Entities | Relations | Triples | Domain |
|---|---|---|---|---|
| FB15k-237 | 14,505 | 237 | 310,079 | General (Freebase) |
| WN18RR | 40,559 | 11 | 92,583 | Lexical (WordNet) |
| YAGO3-10 | 123,143 | 37 | 1,089,000 | General (YAGO) |
| CoDEx-L | 77,951 | 69 | 612,437 | General (Wikidata/DBpedia) |
| Hetionet | 45,158 | 24 | 2,250,197 | Biomedical |
| DRKG | 97,238 | 107 | 5,874,257 | Drug repurposing |
| OGB WikiKG2 | 2,500,604 | 535 | 17,137,181 | Large-scale Wikidata |
| PharMeBINet | 2,869,407 | 208 | 15,883,653 | Pharmaceutical |

---

## 2. The Link Prediction Ecosystem

### 2.1 Major Frameworks

#### PyKEEN (Python KnowlEdge EmbeddiNgs)

- **GitHub**: 2k+ stars, 44 contributors, MIT license
- **Version**: 1.11.1 (April 2025)
- **Models**: 40 models (TransE, DistMult, ComplEx, RotatE, TuckER, ConvE, NodePiece, CompGCN, etc.)
- **Datasets**: 37 built-in datasets + 5 inductive datasets
- **Training**: sLCWA, LCWA, SymmetricLCWA training loops
- **Evaluation**: rank-based, classification, OGB-compatible evaluators, 44 metrics
- **HPO**: Optuna integration for hyperparameter optimization
- **Extensibility**: Bring Your Own Data, Bring Your Own Interaction
- **Tracking**: MLflow, Neptune, Weights & Biases, TensorBoard
- **Key paper**: Ali et al. (2021). "PyKEEN 1.0: A Python Library for Training and Evaluating Knowledge Graph Embeddings." JMLR.
- **Benchmarking**: Large-scale 24,804 GPU-hour study comparing 21 models across 4 datasets (Ali et al., IEEE TPAMI 2021).

PyKEEN's `TriplesFactory.from_path()` can load TSV files of (head, relation, tail) string triples — the primary integration point for pg_ripple.

#### AmpliGraph (Accenture)

- **GitHub**: 2.2k stars, 14 contributors, Apache 2.0
- **Version**: 2.1.0 (February 2024)
- **Backend**: TensorFlow 2 with Keras-style APIs
- **Models**: TransE, DistMult, ComplEx, HolE, RotatE (v1 also had ConvE, ConvKB)
- **Discovery module**: High-level APIs for knowledge discovery — predict new facts, cluster entities, detect near-duplicates
- **MRR benchmarks** (filtered): ComplEx 0.51 (FB15k-237), RotatE 0.95 (WN18RR)

AmpliGraph's `discover` module is particularly relevant: it provides ready-made clustering and duplicate-detection that could augment pg_ripple's SHACL deduplication.

#### DGL-KE (AWS)

- **GitHub**: 1.3k stars, Apache 2.0
- **Focus**: Distributed, large-scale training (86M nodes, 338M edges in 100 minutes on 8 GPUs)
- **Models**: TransE, TransR, RESCAL, DistMult, ComplEx, RotatE
- **Scaling**: Multi-GPU, multi-machine distributed training
- **Key paper**: Zheng et al. (2020). "DGL-KE: Training Knowledge Graph Embeddings at Scale." SIGIR.
- **Note**: AWS now recommends GraphStorm for new projects; DGL-KE is in maintenance mode.

DGL-KE's strength is **scale** — relevant for pg_ripple users with millions of entities.

#### LibKGE

- **GitHub**: ~550 stars
- **Focus**: Reproducible, highly-configurable KGE experiments
- **Models**: RESCAL, TransE, DistMult, ComplEx, ConvE, RotatE, plus combined architectures
- **Strength**: Rigorous evaluation protocols, YAML-based configuration, clean separation of concerns

### 2.2 Emerging Trends (2024–2026)

| Trend | Description | Relevance to pg_ripple |
|---|---|---|
| **Foundation models for KGs** | ULTRA pre-trains on multiple KGs, zero-shot transfer to unseen graphs | pg_ripple could export subgraphs for fine-tuning ULTRA |
| **Inductive link prediction** | NodePiece, GraIL: predict for entities not seen during training | Critical for continuously-growing pg_ripple graphs |
| **LLM + KGE hybrid** | LLMs as text encoders for entity descriptions; KGE for structure | pg_ripple's dictionary stores literal descriptions natively |
| **Temporal KGE** | TTransE, DE-SimplE: model time-varying facts | pg_ripple's RDF-star SIDs can timestamp triples |
| **Uncertainty quantification** | Monte Carlo dropout over KGE models; confidence intervals | RDF-star metadata can store prediction confidence + uncertainty |
| **Federated KGE** | Train embeddings across distributed KGs without sharing data | pg_ripple's SPARQL federation aligns naturally |

---

## 3. Why Link Prediction Is Relevant for pg_ripple

### 3.1 Architectural Fit

pg_ripple's architecture is **uniquely suited** to serve as the hub for KGE workflows:

| pg_ripple capability | Link prediction role |
|---|---|
| **VP storage (integer-encoded)** | Fast bulk export of $(s, p, o)$ integer triples → training data |
| **Dictionary table** | Maps integer IDs ↔ human-readable IRIs/literals for interpretation |
| **Named graphs** | Separate training/predicted triples into distinct graphs |
| **RDF-star (SIDs)** | Attach confidence scores, model metadata, timestamps to predictions |
| **SHACL validation** | Validate predicted triples against domain constraints before insertion |
| **Datalog reasoning** | Combine logical inference with probabilistic predictions |
| **HTAP architecture** | Write predictions to delta tables while reads continue uninterrupted |
| **SPARQL views** | Create live views over prediction results; auto-update on new predictions |
| **JSON-LD framing** | Frame prediction results for downstream consumption (LLM context, APIs) |
| **Federation** | Train on combined local + remote KG data without full replication |
| **GIN FTS** | Enrich entity features with text-derived signals |

### 3.2 The Completeness Gap in Practice

**Every knowledge graph stored in pg_ripple is incomplete.** This is unavoidable:

- **GraphRAG extractions** miss implicit relationships (see [plans/graphrag.md](graphrag.md))
- **Manual data entry** is always partial
- **RDF imports** from external sources have coverage gaps
- **Datalog reasoning** derives only logically entailed facts, not probabilistically likely ones

Link prediction fills the gap between what's **logically derivable** (Datalog) and what's **statistically likely** (KGE). Together, they provide both rigorous and probabilistic knowledge completion.

### 3.3 Concrete Value Propositions

#### 3.3.1 Drug Discovery / Biomedical

- **Dataset**: Hetionet (45k entities, 24 relations, 2.25M triples), DRKG (97k entities, 5.87M triples), PharMeBINet (2.87M entities, 15.8M triples)
- **Use case**: Predict drug–gene, drug–disease, gene–gene interactions
- **pg_ripple advantage**: Store biomedical ontologies as RDF (MeSH, SNOMED CT, Gene Ontology); load Hetionet; train KGE model; predict new drug targets; validate predictions with SHACL shapes constraining drug–disease links to approved entity types; use Datalog to derive transitive pathway relationships
- **Market**: Pharma companies spend $1–2B per approved drug; any reduction in candidate screening saves enormous cost

#### 3.3.2 Enterprise Knowledge Management

- **Use case**: Predict missing "reports-to", "works-with", "expert-in" relationships in an organizational knowledge graph
- **pg_ripple advantage**: GraphRAG-extracted org knowledge stored in pg_ripple (v0.26.0); run link prediction to fill gaps; feed enriched graph back to GraphRAG for better community detection and search
- **Value**: Improved expertise discovery, better org intelligence, reduced information silos

#### 3.3.3 Recommendation Systems

- **Use case**: Predict user–item interactions in an RDF knowledge graph (user → likes → item, item → hasGenre → genre, etc.)
- **pg_ripple advantage**: SPARQL queries to extract user neighbourhoods; KGE to predict new preferences; SHACL to validate recommendation quality (e.g., max recommended items per category)
- **Market**: Knowledge-graph-based recommendations (Alibaba, Amazon) outperform collaborative filtering on cold-start problems

#### 3.3.4 Fraud Detection / Compliance

- **Use case**: Predict suspicious links (entity → transfersTo → shell company) in a financial knowledge graph
- **pg_ripple advantage**: Named graphs separate confirmed vs. predicted links; RDF-star stores confidence and model provenance; SPARQL views create live dashboards; federation joins local KG with external sanctions lists
- **Value**: Financial institutions face $10B+ in annual fines for compliance failures

#### 3.3.5 Scientific Literature / Citation Analysis

- **Use case**: Predict missing citations, co-authorship, methodology links in an academic knowledge graph
- **pg_ripple advantage**: Store bibliographic RDF (Dublin Core, BIBO ontology); predict links between papers and concepts; use Datalog to derive "co-citation clusters"; export via JSON-LD framing to LLM context windows
- **Value**: Faster literature review, discovery of overlooked connections

---

## 4. Integration Architecture

### 4.1 Overview: The KGE Loop

```
┌──────────────────────────────────────────────────────┐
│                    pg_ripple                          │
│                                                      │
│  ┌────────────┐    ┌───────────────┐                 │
│  │ Observed    │    │ Predicted     │                 │
│  │ triples     │    │ triples       │                 │
│  │ (graph: g0) │    │ (graph: g_pred│)                │
│  └──────┬──────┘    └───────▲───────┘                │
│         │                   │                        │
│         │ 1. Export         │ 4. Load + validate     │
│         ▼                   │                        │
│  ┌──────────────┐    ┌──────┴───────┐                │
│  │ export_kge() │    │ load_kge()   │                │
│  │ → TSV/Parquet│    │ ← TSV/Parquet│                │
│  └──────┬───────┘    └──────▲───────┘                │
└─────────┼───────────────────┼────────────────────────┘
          │                   │
          ▼                   │
   ┌──────────────────────────┴──────┐
   │         PyKEEN / DGL-KE         │
   │                                 │
   │  2. Train embedding model       │
   │     (TransE, RotatE, ComplEx)   │
   │                                 │
   │  3. Predict top-k missing links │
   │     with confidence scores      │
   └─────────────────────────────────┘
```

### 4.2 Step-by-Step Workflow

#### Step 1: Export Training Data

```sql
-- Export observed triples in PyKEEN-compatible TSV format
-- Output: /tmp/kge_export/train.tsv  (head_iri  relation_iri  tail_iri)
SELECT pg_ripple.export_kge_triples(
    graph_iri   := 'http://example.org/my-graph',
    output_path := '/tmp/kge_export/train.tsv',
    format      := 'pykeen'   -- or 'dglke', 'ampligraph'
);

-- Optionally export with integer IDs (faster, requires ID mapping file)
SELECT pg_ripple.export_kge_triples(
    graph_iri   := 'http://example.org/my-graph',
    output_path := '/tmp/kge_export/',
    format      := 'pykeen_numeric',  -- train.tsv + entity2id.tsv + relation2id.tsv
    include_inferred := false          -- exclude Datalog-derived triples
);
```

**Implementation**: VP table scan → dictionary decode → write TSV. pg_ripple's VP storage (one table per predicate) makes this trivially parallelizable: each VP table can be scanned independently.

**Performance estimate**: With VP tables and integer encoding already in place, a 10M-triple graph exports in ~5 seconds (dominated by dictionary decode I/O).

#### Step 2: Train the Model

```python
# scripts/train_kge.py
from pykeen.pipeline import pipeline
from pykeen.triples import TriplesFactory

# Load pg_ripple export
tf = TriplesFactory.from_path('/tmp/kge_export/train.tsv')
training, testing = tf.split([0.8, 0.2])

result = pipeline(
    training=training,
    testing=testing,
    model='RotatE',           # Best overall translational model
    model_kwargs=dict(
        embedding_dim=256,
    ),
    training_kwargs=dict(
        num_epochs=200,
        batch_size=512,
    ),
    negative_sampler='bernoulli',
    loss='NSSALoss',          # Self-adversarial negative sampling
    optimizer='Adam',
    optimizer_kwargs=dict(lr=0.0001),
    evaluator='rankbased',
)

# Save the trained model
result.save_to_directory('/tmp/kge_model/')
print(f"MRR: {result.metric_results.get_metric('mean_reciprocal_rank'):.4f}")
print(f"Hits@10: {result.metric_results.get_metric('hits_at_10'):.4f}")
```

#### Step 3: Generate Predictions

```python
from pykeen.predict import predict_all

# Score all possible triples (or a filtered subset)
predictions = predict_all(
    model=result.model,
    triples_factory=training,
    k=10000,                  # Top 10,000 predictions
)

# Filter predictions with confidence threshold
high_confidence = predictions.filter_minimum_score(threshold=0.8)

# Export as TSV for pg_ripple import
high_confidence.to_df().to_csv(
    '/tmp/kge_predictions/predictions.tsv',
    sep='\t',
    columns=['head_label', 'relation_label', 'tail_label', 'score'],
    index=False,
)
```

#### Step 4: Load Predictions with Provenance

```sql
-- Load predictions into a dedicated named graph with RDF-star metadata
SELECT pg_ripple.load_kge_predictions(
    predictions_path := '/tmp/kge_predictions/predictions.tsv',
    graph_iri        := 'http://example.org/my-graph/predicted/2026-04-18',
    model_name       := 'RotatE',
    model_params     := '{"embedding_dim": 256, "epochs": 200}',
    min_confidence   := 0.75,
    validate_shacl   := true    -- Reject predictions that violate SHACL shapes
);

-- Result: new triples with RDF-star provenance metadata
-- << :Alice :colleague :Bob >>
--     lp:confidence    0.92 ;
--     lp:model         "RotatE" ;
--     lp:trainingDate  "2026-04-18"^^xsd:date ;
--     lp:source        1 .    -- source=1 marks inferred triples
```

### 4.3 RDF-star Provenance Model

pg_ripple's RDF-star support (v0.4.0+) is a natural fit for link prediction metadata. Define a small `lp:` vocabulary:

```turtle
@prefix lp:  <http://pg-ripple.dev/ontology/link-prediction#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

lp:confidence       a owl:DatatypeProperty ;
    rdfs:range xsd:float ;
    rdfs:comment "Confidence score from the KGE model (0.0–1.0)" .

lp:model            a owl:DatatypeProperty ;
    rdfs:range xsd:string ;
    rdfs:comment "Name of the KGE model (e.g., RotatE, ComplEx)" .

lp:modelVersion     a owl:DatatypeProperty ;
    rdfs:range xsd:string .

lp:trainingDate     a owl:DatatypeProperty ;
    rdfs:range xsd:date .

lp:trainingGraphIri a owl:DatatypeProperty ;
    rdfs:range xsd:anyURI ;
    rdfs:comment "IRI of the named graph used for training" .

lp:mrr              a owl:DatatypeProperty ;
    rdfs:range xsd:float ;
    rdfs:comment "Model MRR on the test split" .

lp:hitsAt10         a owl:DatatypeProperty ;
    rdfs:range xsd:float .
```

### 4.4 SHACL Shapes for Prediction Quality

Validate predicted triples before they pollute the graph:

```turtle
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix lp:  <http://pg-ripple.dev/ontology/link-prediction#> .
@prefix ex:  <http://example.org/> .

# Every predicted triple must have a confidence score ≥ 0.5
ex:PredictionMetadataShape a sh:NodeShape ;
    sh:targetClass lp:PredictedTriple ;
    sh:property [
        sh:path lp:confidence ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:datatype xsd:float ;
        sh:minInclusive 0.5 ;
        sh:maxInclusive 1.0 ;
    ] ;
    sh:property [
        sh:path lp:model ;
        sh:minCount 1 ;
        sh:datatype xsd:string ;
    ] .

# Domain-specific: predicted drug–disease links must reference
# entities that actually exist and have correct types
ex:DrugDiseasePredictionShape a sh:NodeShape ;
    sh:targetSubjectsOf ex:treats ;
    sh:property [
        sh:path rdf:type ;
        sh:hasValue ex:Drug ;
    ] ;
    sh:property [
        sh:path ex:treats ;
        sh:class ex:Disease ;
    ] .
```

### 4.5 Datalog + Link Prediction: Complementary Reasoning

pg_ripple's Datalog engine (v0.10.0+) provides **logical** inference. Link prediction provides **probabilistic** inference. Together they're more powerful than either alone:

| Approach | Derives | Example |
|---|---|---|
| **Datalog** (deterministic) | Logically entailed facts from explicit rules | If `A worksAt X` and `B worksAt X`, then `A coworker B` |
| **Link prediction** (probabilistic) | Statistically likely facts from embedding patterns | `A mentor B` with 87% confidence (no explicit rule, learned from graph structure) |
| **Combined** | Hybrid knowledge completion | Datalog derives `A coworker B`; KGE predicts `A mentor B` (0.87); together they create richer entity profiles for GraphRAG community detection |

**Implementation pattern**: Run Datalog first (logical closure), then train KGE on the enriched graph (Datalog-derived triples included). The enriched graph has higher density → better embedding quality → more accurate predictions.

```sql
-- 1. Run Datalog materialization
SELECT pg_ripple.datalog_materialize();

-- 2. Export the enriched graph (observed + Datalog-derived)
SELECT pg_ripple.export_kge_triples(
    graph_iri        := 'http://example.org/my-graph',
    output_path      := '/tmp/kge_export/enriched_train.tsv',
    include_inferred := true   -- Include Datalog-derived triples
);

-- 3. Train KGE model on enriched graph (external Python script)
-- 4. Load predictions back (source=1, separate named graph)
```

---

## 5. Competitive Landscape

### 5.1 How Competitors Handle Link Prediction

| System | Link prediction support | Limitations |
|---|---|---|
| **Neo4j** | GDS library has FastRP + kNN for similarity; no native KGE; community plugin `neo4j-graph-data-science` has node classification but no LP | Requires manual ETL; no provenance tracking; no validation |
| **Amazon Neptune** | Neptune ML uses DGL-KE for LP; tightly coupled to AWS SageMaker | Vendor lock-in; no SPARQL integration for predictions; black box |
| **Stardog** | No native LP; users export to external tools | Export/import friction; no metadata on predictions |
| **Virtuoso** | No native LP | Same export/import friction |
| **Blazegraph** | No native LP; discontinued | — |
| **pgvector** | Stores embeddings; no KGE training or LP | Only the vector part; no graph reasoning |
| **Weaviate / Qdrant** | Vector search only; no relational reasoning | Cannot model structured relations |

### 5.2 pg_ripple's Differentiation

**No existing system combines all of:**

1. **Native RDF + SPARQL** — standard query language for knowledge graphs
2. **Integer-encoded VP storage** — optimal for bulk export to KGE frameworks
3. **RDF-star provenance** — attach confidence, model name, training date to each prediction
4. **SHACL validation** — reject malformed predictions before they enter the graph
5. **Datalog pre-enrichment** — improve embedding quality with logical inference
6. **HTAP architecture** — write predictions without disrupting read queries
7. **Named graph isolation** — separate observed, inferred, and predicted triples
8. **SPARQL views** — live dashboards over prediction results
9. **JSON-LD framing** — shape prediction results for LLM context windows
10. **PostgreSQL ecosystem** — pgvector for embedding storage, PostGIS for spatial, pg_cron for scheduled retraining

This is a **unique market position**: the only system where a user can store, query, enrich, predict, validate, and consume knowledge graph facts in a single ACID-compliant database with standard interfaces.

---

## 6. Positioning Strategy

### 6.1 Market Segments

#### Segment A: Biomedical / Life Sciences

- **Pain point**: Researchers use separate tools for ontology management (Protégé), graph storage (Neo4j/Neptune), embedding training (PyKEEN), and result analysis (Jupyter)
- **pg_ripple pitch**: "One database for your entire biomedical knowledge pipeline — from ontology to prediction to validation"
- **Key features**: Load MeSH/GO/SNOMED as RDF; train KGE to predict drug–gene interactions; validate with SHACL shapes; query with SPARQL; export to GraphRAG for LLM-powered literature search
- **Competitive advantage**: Zero-ETL between storage and training; SHACL prevents nonsensical predictions (e.g., "aspirin treats happiness")

#### Segment B: Enterprise Knowledge Management

- **Pain point**: GraphRAG pipelines extract knowledge from documents, but the resulting graph is static and incomplete
- **pg_ripple pitch**: "Turn GraphRAG's static graph into a living, self-improving knowledge base"
- **Key features**: GraphRAG BYOG export (v0.26.0); link prediction to fill gaps; Datalog enrichment; continuous indexing via HTAP
- **Competitive advantage**: The only system that combines GraphRAG integration + link prediction + Datalog reasoning + live SPARQL views

#### Segment C: Financial Services / Compliance

- **Pain point**: Detecting hidden relationships between entities for anti-money laundering (AML) and know-your-customer (KYC)
- **pg_ripple pitch**: "Predict hidden financial connections with auditable provenance"
- **Key features**: Named graphs for audit trails; RDF-star for confidence and model provenance; SHACL for compliance rules; federation for external data sources (sanctions lists, company registries)
- **Competitive advantage**: Full audit trail on every predicted link; PostgreSQL's enterprise certifications (SOC 2, HIPAA, PCI-DSS by deployment)

#### Segment D: Academic / Research

- **Pain point**: Researchers need reproducible KGE experiments with queryable results
- **pg_ripple pitch**: "The reproducible KGE experiment database — store training data, model metadata, and predictions in one place"
- **Key features**: Export → train → import loop with full provenance; SPARQL queries over prediction results; comparison across models/runs via named graphs
- **Competitive advantage**: Every prediction is traceable to its training data, model, and hyperparameters

### 6.2 Messaging

**Tagline**: *"From facts to predictions — and back. pg_ripple closes the knowledge graph loop."*

**Key messages**:

1. **For data scientists**: "Export your knowledge graph to PyKEEN in one SQL call. Load predictions back with confidence scores and full provenance. No ETL scripts, no data wrangling."

2. **For enterprise architects**: "Link prediction with audit trails. Every predicted relationship has a traceable confidence score, model version, and training date — stored as RDF-star metadata in your PostgreSQL database."

3. **For GraphRAG users**: "Make your GraphRAG graph smarter. Link prediction fills the gaps that LLM extraction misses. Datalog reasoning derives logical consequences. SHACL validation catches errors. All in one database."

4. **For researchers**: "40 KGE models (via PyKEEN), 37 built-in datasets, and a SPARQL-queryable experiment database. Reproduce any result, compare models across runs, and publish with provenance."

### 6.3 Concrete Deliverables for pg_ripple

| Deliverable | Description | Effort |
|---|---|---|
| `export_kge_triples()` | SQL function: VP scan → TSV export in PyKEEN/DGL-KE/AmpliGraph format | 1 pw |
| `export_kge_triples_numeric()` | Numeric-ID variant with entity2id/relation2id mapping files | 0.5 pw |
| `load_kge_predictions()` | SQL function: load TSV predictions into named graph with RDF-star metadata | 1.5 pw |
| `lp:` ontology | Link prediction provenance vocabulary (Turtle file) | 0.5 pw |
| SHACL shapes | Validation shapes for prediction metadata and domain constraints | 0.5 pw |
| `scripts/train_kge.py` | Python CLI wrapper: `--pg-url --graph-iri --model --epochs --output` | 1 pw |
| `scripts/predict_links.py` | Python CLI: train, predict, export TSV for pg_ripple import | 1 pw |
| End-to-end example | `examples/link_prediction.sql` + Jupyter notebook | 1 pw |
| pg_regress tests | 3 tests: export, import, provenance queries | 1 pw |
| Documentation | User guide, reference, tutorial | 1 pw |
| **Total** | | **~9 pw** |

---

## 7. Risk Analysis

### 7.1 Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| KGE model quality depends heavily on hyperparameters | High | Medium | Provide sensible defaults in `train_kge.py`; document PyKEEN's HPO integration |
| Large KGs (>10M triples) may exhaust GPU memory during training | Medium | High | Support numeric export format; document DGL-KE for distributed training |
| Predicted triples may introduce noise if confidence threshold is too low | Medium | High | SHACL validation gate; default minimum confidence 0.75 |
| Dependency on external Python ecosystem (PyKEEN, PyTorch) | Low | Medium | Keep integration at the data level (TSV/Parquet); no tight coupling |

### 7.2 Market Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Foundation models (ULTRA) may reduce need for per-graph KGE training | Medium | Medium | Support ULTRA integration path alongside traditional KGE |
| LLM-based LP (e.g., GPT-4 for relationship prediction) may eclipse embedding-based LP | Low | High | pg_ripple's value is as the *storage and validation layer*, not the ML layer; agnostic to prediction method |
| Users may not adopt the Python CLI bridge | Medium | Low | Provide pure-SQL functions for export/import; Python is optional |

---

## 8. Timeline and Dependencies

### 8.1 Prerequisites

- **v0.4.0** (RDF-star): Required for provenance metadata ✅ (shipped)
- **v0.7.0** (SHACL Core): Required for prediction validation ✅ (shipped)
- **v0.10.0** (Datalog): Required for pre-enrichment ✅ (shipped)
- **v0.26.0** (GraphRAG): Recommended for full GraphRAG + LP pipeline (planned)

### 8.2 Suggested Roadmap Placement

Link prediction integration would be well-positioned as **v0.27.0**, following the GraphRAG integration (v0.26.0):

- **v0.26.0**: GraphRAG BYOG export, Datalog enrichment, SHACL quality enforcement
- **v0.27.0**: Link prediction export/import, PyKEEN integration, provenance metadata
- Together: a complete **knowledge graph intelligence pipeline** — extract (GraphRAG) → store (pg_ripple) → enrich (Datalog) → predict (KGE) → validate (SHACL) → query (SPARQL) → serve (JSON-LD / HTTP)

### 8.3 Phased Delivery

| Phase | Scope | Effort |
|---|---|---|
| **Phase 1: Export/Import** | `export_kge_triples()`, `load_kge_predictions()`, lp: ontology, SHACL shapes, basic example | 4 pw |
| **Phase 2: CLI & Automation** | `train_kge.py`, `predict_links.py`, Jupyter notebook, pg_regress tests | 3 pw |
| **Phase 3: Documentation & Polish** | User guide, reference docs, tutorial, performance benchmarks | 2 pw |
| **Total** | | **~9 pw** |

---

## 9. References

### Research Papers

1. Bordes, A. et al. (2013). "Translating Embeddings for Modeling Multi-relational Data." NeurIPS. *(TransE)*
2. Yang, B. et al. (2014). "Embedding Entities and Relations for Learning and Inference in Knowledge Bases." ICLR. *(DistMult)*
3. Trouillon, T. et al. (2016). "Complex Embeddings for Simple Link Prediction." ICML. *(ComplEx)*
4. Sun, Z. et al. (2019). "RotatE: Knowledge Graph Embedding by Relational Rotation in Complex Space." ICLR. *(RotatE)*
5. Balažević, I. et al. (2019). "TuckER: Tensor Factorization for Knowledge Graph Completion." EMNLP. *(TuckER)*
6. Ali, M. et al. (2021). "PyKEEN 1.0: A Python Library for Training and Evaluating Knowledge Graph Embeddings." JMLR.
7. Ali, M. et al. (2021). "Bringing Light Into the Dark: A Large-scale Evaluation of Knowledge Graph Embedding Models Under a Unified Framework." IEEE TPAMI.
8. Rossi, A. et al. (2021). "Knowledge Graph Embedding for Link Prediction: A Comparative Analysis." ACM TKDD.
9. Galkin, M. et al. (2024). "Towards Foundation Models for Knowledge Graph Reasoning." ICLR. *(ULTRA)*
10. Edge, D. et al. (2024). "From Local to Global: A Graph RAG Approach to Query-Focused Summarization." arXiv:2404.16130. *(GraphRAG)*
11. Zheng, D. et al. (2020). "DGL-KE: Training Knowledge Graph Embeddings at Scale." SIGIR.
12. Dong, X. et al. (2014). "Knowledge Vault: A Web-Scale Approach to Probabilistic Knowledge Fusion." KDD.

### Software

- **PyKEEN**: https://github.com/pykeen/pykeen (v1.11.1, MIT, 2k stars, 40 models, 37 datasets)
- **AmpliGraph**: https://github.com/Accenture/AmpliGraph (v2.1.0, Apache 2.0, 2.2k stars)
- **DGL-KE**: https://github.com/awslabs/dgl-ke (v0.1.1, Apache 2.0, 1.3k stars; maintenance mode)
- **LibKGE**: https://github.com/uma-pi1/kge (~550 stars)
- **ULTRA**: https://github.com/DeepGraphLearning/ULTRA (foundation model for KG reasoning)

---

## 10. Conclusion

Link prediction is a mature, high-impact ML technique with proven applications in drug discovery, enterprise knowledge management, fraud detection, and recommendation systems. pg_ripple's existing architecture — VP storage, dictionary encoding, RDF-star, SHACL, Datalog, HTAP, named graphs, SPARQL views, JSON-LD framing — provides the ideal substrate for a complete KGE workflow.

The integration strategy is deliberately **loosely coupled**: pg_ripple handles storage, validation, and querying; PyKEEN/DGL-KE handle ML; the interface is simple TSV/Parquet files. This avoids tight dependencies while providing seamless end-to-end workflows.

By delivering link prediction integration in v0.27.0 (following GraphRAG integration in v0.26.0), pg_ripple becomes the **only system** that offers a complete knowledge graph intelligence pipeline: extract → store → enrich → predict → validate → query → serve.
