# Probabilistic and Fuzzy Features for pg_ripple

> **Status:** Research report, 2026-05-01  
> **Audience:** Maintainers, contributors, and evaluators planning the v0.84.0 uncertain knowledge engine.  
> **Authority:** [ROADMAP.md](../ROADMAP.md) is the authoritative schedule. This document provides the research basis for the v0.84.0 scope decision and post-1.0 directions.

---

## 1. Executive Summary

Classical knowledge graph systems reason in binary: a fact is true or false, a constraint passes or fails, a class member belongs or does not. Real-world knowledge is graded — extracted facts carry extraction confidence, sensor readings carry measurement uncertainty, regulatory rules have degrees of compliance, and ontologies often encode probabilistic rather than definitive relationships.

This report surveys eight families of probabilistic and fuzzy features that could extend pg_ripple's reasoning capabilities. Four are selected for v0.84.0 because they compose cleanly with the existing architecture. Four additional directions are documented for post-1.0 research work.

**v0.84.0 scope:**

| # | Feature | Why now |
|---|---|---|
| 1 | **Probabilistic Datalog** | GUC stub already exists; closes the gap between the published blog post and the actual implementation |
| 2 | **Fuzzy SPARQL filtering** | Pure query-layer addition; builds on `pg_trgm` already available in PostgreSQL |
| 3 | **Soft SHACL scoring** | Natural extension of the existing SHACL pipeline; unlocks data-quality dashboards |
| 4 | **Provenance-weighted confidence** | Zero-friction path to confidence for data that already has PROV-O metadata |

Features 1–4 share a single confidence side table (`_pg_ripple.confidence`) and compose cleanly without architectural changes to VP tables.

**Post-1.0 directions:**

| # | Feature | Why deferred |
|---|---|---|
| 5 | Temporal confidence decay | Depends on stable confidence side table from Feature 1 |
| 6 | Subjective Logic belief functions | Different uncertainty representation; design needs validation |
| 7 | Link prediction in SPARQL | Expensive all-entity scoring; needs HNSW approximation work |
| 8 | AMIE+ rule mining | Very large search space; needs time-bounded query engine support |

---

## 2. Architecture Fit

### 2.1 Storage model

VP tables store `(s, o, g, i, source)` as BIGINT columns. The `i` column is a globally-unique statement identifier (SID) from a shared sequence. The `source` SMALLINT distinguishes explicit (`0`) from inferred (`1`) triples.

For confidence scoring, two options exist:

**Option A — side table (recommended for v0.84.0):**

```sql
CREATE TABLE _pg_ripple.confidence (
    statement_id BIGINT NOT NULL,
    confidence   FLOAT8 NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    model        TEXT   NOT NULL DEFAULT 'datalog',
    asserted_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY  (statement_id, model)
);
CREATE INDEX ON _pg_ripple.confidence (statement_id);
```

Advantages: zero schema change to VP tables; the hot path is unaffected when probabilistic mode is off; multiple confidence values per triple from different models are supported.

**Option B — inline column:**

Add `confidence FLOAT8 DEFAULT 1.0` directly to each VP delta/main table. Simpler to join but requires a schema migration on every VP table and bloats the hot path.

**Decision:** Option A (side table) for v0.84.0. The SID column already provides the join key. Inline columns can be evaluated after the feature stabilises.

### 2.2 Query layer

SPARQL → SQL translation in `src/sparql/plan.rs` already dispatches custom functions via the `pg:*` prefix. Adding `pg:confidence(?s, ?p, ?o)`, `pg:fuzzy_match(?a, ?b)`, and `pg:confPath(pred, threshold)` follows the existing pattern.

### 2.3 Datalog layer

The Datalog rule parser in `src/datalog/parse.rs` already handles annotations. Adding `@weight(FLOAT)` is an additive grammar extension. The semi-naive evaluation loop in `src/datalog/seminaive.rs` generates SQL CTEs; confidence propagation inserts coefficient multiplication into the generated SQL.

### 2.4 SHACL layer

SHACL constraints produce `sh:ValidationResult` triples. Soft SHACL adds a numeric `sh:resultSeverityScore FLOAT8` alongside the existing categorical `sh:resultSeverity`. The composite `pg_ripple.shacl_score()` function aggregates validation results into a [0, 1] quality score.

---

## 3. Feature 1: Probabilistic Datalog

**Milestone: v0.84.0 | Complexity: Large**

### Background

ProbLog (De Raedt et al., 2007) showed that logic programs can be annotated with probabilities and that inference can compute exact or approximate probabilities of derived facts. The model was later generalised to Markov Logic Networks (Richardson & Domingos, 2006), which use log-odds weights, and to weighted Datalog systems used in production knowledge graphs such as NELL (Carlson et al., 2010).

pg_ripple already has a `pg_ripple.probabilistic_datalog` GUC stub (added v0.57.0) and a `blog/probabilistic-datalog.md` blog post that describes the intended feature. The v0.84.0 work makes both real.

### Core model

Each rule carries an `@weight(FLOAT)` annotation (0 ≤ w ≤ 1), interpreted as the conditional probability that the rule fires when its body is satisfied:

```
decision_maker(X) :- manager(X), dept(X, finance). @weight(0.90)
budget_approver(X) :- decision_maker(X), risk_score(X, low). @weight(0.85)
```

**Conjunction:** confidences multiply. A rule at weight 0.9 applied to body atoms at confidence 0.95 and 0.80 produces:

$$\text{conf}(\text{head}) = 0.9 \times 0.95 \times 0.80 = 0.684$$

**Multiple derivation paths (noisy-OR):** when a fact is derived via $k$ independent rule firings, the combined confidence is:

$$\text{conf}(\text{head}) = 1 - \prod_{i=1}^{k} (1 - \text{conf}_i)$$

Two independent paths at 0.7 yield $1 - 0.3 \times 0.3 = 0.91$.

**Explicit facts:** triples inserted without a weight have confidence 1.0 by default. `load_triples_with_confidence()` assigns lower confidence to uncertain source data.

This model is provably sound for acyclic rule sets. For cyclic rule sets (which require loopy belief propagation or Monte Carlo sampling), a GUC guards access until the semantics are stabilised.

### SQL surface

```sql
-- Enable the mode
SET pg_ripple.probabilistic_datalog = on;

-- Add a weighted rule
SELECT pg_ripple.add_rule(
  'budget_approver(X) :- decision_maker(X), dept(X, finance). @weight(0.85)'
);

-- Bulk-load base facts with explicit confidence
SELECT pg_ripple.load_triples_with_confidence(
  '...n-triples data...',
  confidence => 0.72
);

-- Query derived confidence via SPARQL
SELECT * FROM pg_ripple.sparql('
  SELECT ?approver ?conf WHERE {
    ?approver ex:isBudgetApprover true .
    BIND(pg:confidence(?approver, ex:isBudgetApprover) AS ?conf)
    FILTER(?conf > 0.5)
  }
  ORDER BY DESC(?conf)
');
```

### Use cases

| Use case | How probabilistic Datalog helps |
|---|---|
| NLP / IE pipelines | Facts extracted from text carry extraction confidence; downstream rules inherit and propagate it automatically |
| Sensor / IoT data | Uncertainty in readings flows into derived alerts without losing the uncertainty signal |
| Knowledge base completion | Combine Datalog rules with pgvector similarity scores to rank plausible but unconfirmed facts |
| Soft ontology alignment | `owl:sameAs` suggestions from embedding-based entity alignment carry a threshold-able confidence |
| Risk scoring | Chain soft "risk indicator" rules to produce composite risk scores without bespoke SQL |
| Explainability | `explain_inference()` exposes per-step confidence for auditors |

### Implementation plan

1. **Grammar**: add `@weight(FLOAT)` to the Datalog rule parser in `src/datalog/parse.rs`.
2. **Schema**: create `_pg_ripple.confidence` side table in the v0.84.0 migration script.
3. **Semi-naive loop**: in `src/datalog/seminaive.rs`, when the GUC is on, generate SQL that joins against the confidence side table and emits `confidence = rule_weight × conf_1 × conf_2 × …`.
4. **Noisy-OR merge**: after all stratum evaluations, a SQL pass computes `1 - PRODUCT(1 - confidence)` for duplicate `(s, p, o)` derivations.
5. **SPARQL function**: add `pg:confidence(?s, ?p, ?o)` to the function dispatch table in `src/sparql/plan.rs`, compiling to a JOIN against `_pg_ripple.confidence`.
6. **Bulk-loader**: add `confidence FLOAT8 DEFAULT 1.0` parameter to `load_ntriples()` / new `load_triples_with_confidence()` function.
7. **`explain_inference()`**: include per-step confidence in the derivation tree JSON output.
8. **Test corpus**: adapt the ProbLog CLUTRR and Nations benchmarks for CI regression.

---

## 4. Feature 2: Fuzzy SPARQL Filtering

**Milestone: v0.84.0 | Complexity: Medium**

### Background

Approximate string matching is a prerequisite for any real-world integration scenario. Entities in different source systems have slightly different names: "John Smith" vs "J. Smith" vs "J Smith Jr." SPARQL's `FILTER` is exact; fuzzy SPARQL extends it with similarity predicates.

This is not entirely novel: FuzzySPARQL (Straccia & Ragone, 2010) proposed similar extensions for fuzzy description logics. What is distinctive about the pg_ripple approach is that it compiles directly to PostgreSQL's battle-tested `pg_trgm` extension rather than implementing a custom similarity engine.

### Sub-feature A: approximate string similarity

```sparql
SELECT ?person WHERE {
  ?person foaf:name ?name .
  FILTER(pg:fuzzy_match(?name, "John Smith") > 0.8)
}
```

`pg:fuzzy_match(a, b)` compiles to PostgreSQL `similarity(a, b)` from `pg_trgm` (trigram overlap). For queries where match order is irrelevant:

```sparql
FILTER(pg:token_set_ratio(?name, "Microsoft Corporation") > 0.9)
```

compiles to `word_similarity(a, b)` which scores "Corporation Microsoft" at 1.0.

When a GIN trigram index exists on the dictionary `value` column, PostgreSQL's planner can push the similarity threshold down to an index scan — large vocabularies do not require a full scan.

### Sub-feature B: confidence-threshold property paths

```sparql
SELECT ?x ?y WHERE {
  ?x pg:confPath(ex:relatedTo, 0.7)+ ?y
}
```

`pg:confPath(predicate, min_confidence)` is a path operator that only traverses edges where `pg:confidence(s, p, o) >= min_confidence`. This compiles to a `WITH RECURSIVE … CYCLE` CTE (using PG18's hash-based cycle detection) that joins against `_pg_ripple.confidence` in the edge condition. It enables "follow only confident connections" graph traversal, which is directly useful in entity resolution, supply-chain tracing, and social network analysis.

### Implementation plan

1. `pg_trgm` is a PostgreSQL built-in extension requiring only `CREATE EXTENSION pg_trgm`. No new Rust code needed for the similarity functions — they map to SQL calls.
2. Add `pg:fuzzy_match` and `pg:token_set_ratio` to the SPARQL function dispatch table in `src/sparql/plan.rs`.
3. Add `pg:confPath(pred, threshold)` as a new path operator to the SPARQL path grammar and SPARQL → SQL translator. The generated SQL is a `WITH RECURSIVE` CTE with a confidence JOIN in the recursive step.
4. Add `pg_ripple.default_fuzzy_threshold` GUC (FLOAT8, default 0.7) for path queries that omit an explicit threshold.
5. The migration script creates a GIN trigram index on `_pg_ripple.dictionary (value gin_trgm_ops)` if `pg_trgm` is available.

---

## 5. Feature 3: Soft SHACL Scoring

**Milestone: v0.84.0 | Complexity: Medium**

### Background

SHACL today produces binary results. For many production use cases, data quality is not binary — a dataset that fails 2% of its constraints is qualitatively different from one that fails 80%, and treating both as "invalid" is neither useful nor actionable.

Soft SHACL extends the existing validation pipeline without replacing it. Binary results continue to work as before; the new scoring layer is additive.

### Model

Each SHACL shape or property shape carries an optional `sh:severityWeight xsd:decimal` predicate (default 1.0). The composite quality score is:

$$\text{score} = 1 - \frac{\sum_i w_i \cdot \text{violations}_i}{\sum_i w_i \cdot \text{applicable}_i}$$

Where $w_i$ is the severity weight for shape $i$, $\text{violations}_i$ is the number of focus nodes violating shape $i$, and $\text{applicable}_i$ is the total number of applicable focus nodes.

This produces 1.0 for perfect data and meaningful intermediate values for partial quality. Shapes with `sh:severity sh:Violation` can carry a higher default weight than `sh:Warning` shapes.

### New SQL surface

```sql
-- Assign severity weights in the shape graph
SELECT pg_ripple.sparql_update('
  INSERT DATA {
    GRAPH ex:shapes {
      ex:PersonShape sh:severityWeight 0.9 .
      ex:AddressShape sh:severityWeight 0.6 .
    }
  }
');

-- Composite quality score for a graph
SELECT pg_ripple.shacl_score('http://example.org/mydata');
-- Returns: 0.94

-- Individual shape results with scores
SELECT * FROM pg_ripple.shacl_report_scored('http://example.org/mydata');
-- Returns: (focus_node, shape_iri, result_severity, result_severity_score, message)
```

### Use cases

- **Data quality dashboards**: time-series quality score; alert when score drops below threshold.
- **ETL pipelines**: accept data at score ≥ 0.9 (strict), score ≥ 0.7 (lenient).
- **Compliance reporting**: a composite score is more useful to regulators than a boolean.
- **Multi-source integration**: each source scores independently; low-scoring sources trigger remediation workflows.
- **Weighted validation**: security-related shapes can carry weight 1.0 while cosmetic shapes carry 0.1.

### Implementation plan

1. Recognise `sh:severityWeight` as a catalogued predicate in the SHACL shape store.
2. In `src/shacl/`, add a `shacl_score()` function that runs the existing validation pipeline and aggregates with the weighted formula.
3. Add `sh:resultSeverityScore FLOAT8` to the `shacl_validate()` result set.
4. Expose `pg_ripple.shacl_score(graph_iri TEXT) RETURNS FLOAT8` and `pg_ripple.shacl_report_scored(graph_iri TEXT) RETURNS TABLE (...)` as `pg_extern` functions.
5. Optional: `_pg_ripple.shacl_score_log (graph_iri TEXT, score FLOAT8, measured_at TIMESTAMPTZ)` for continuous monitoring.

---

## 6. Feature 4: Provenance-Weighted Confidence

**Milestone: v0.84.0 | Complexity: Small (given Feature 1)**

### Background

pg_ripple already supports PROV-O provenance (v0.57.0): triples record `prov:wasDerivedFrom` and `prov:wasAttributedTo` back to their source. Provenance-weighted confidence automatically converts those relationships into triple confidence scores without requiring the user to annotate each triple individually.

### How it works

1. The operator declares source trust scores in the provenance graph:

   ```sparql
   INSERT DATA {
     GRAPH ex:provenance {
       ex:WikidataExtractor  pg:sourceTrust 0.95 .
       ex:NLPPipeline        pg:sourceTrust 0.70 .
       ex:ManualEntry        pg:sourceTrust 0.99 .
     }
   }
   ```

2. A system-provided Datalog rule propagates trust from sources to triples:

   ```
   triple_confidence(S, P, O, Trust) :-
     quad(S, P, O, G),
     named_graph_source(G, Source),
     source_trust(Source, Trust).
   ```

3. The Datalog engine materialises confidence values into `_pg_ripple.confidence` automatically during `run_inference()`.

4. When a triple appears in multiple named graphs from different sources, the noisy-OR formula (Feature 1) combines the independent attestations: a triple supported by both a 0.70-confidence source and a 0.95-confidence source gets $1 - 0.3 \times 0.05 = 0.985$.

5. The `pg:confidence()` SPARQL function works transparently over these derived values.

### What makes this novel

Most probabilistic triple stores require confidence annotations at insert time. The provenance-weighted approach derives confidence *lazily* from data that is already in the graph (PROV-O triples), making the feature zero-effort for operators who already use provenance tracking. The combination with noisy-OR multi-source attestation is not widely implemented in production triple stores.

### Implementation plan

1. Register `pg:sourceTrust` as a special predicate in the VP catalog (`_pg_ripple.predicates`).
2. Provide a built-in Datalog rule template (auto-registered when `pg_ripple.prov_confidence = on`) that propagates source trust to the confidence side table.
3. Add `pg_ripple.prov_confidence` GUC (bool, default off).
4. When the GUC is enabled, the rule is auto-added on first `run_inference()` call.

---

## 7. Feature 5: Temporal Confidence Decay

**Milestone: Post-v0.84.0 | Complexity: Medium**

### What it is

Knowledge becomes stale. A fact asserted with 90% confidence six months ago may be only 60% confident today if the domain is fast-moving (personnel records, market data, regulatory status). Temporal decay models this via:

$$\text{confidence}(t) = c_0 \times e^{-\lambda \cdot \Delta t}$$

Where $c_0$ is the confidence at assertion time, $\lambda$ is a per-predicate decay rate (in units of 1/day), and $\Delta t$ is the age of the fact in days.

**New SPARQL functions:**

- `pg:current_confidence(?s, ?p, ?o)` — applies the decay formula at query time.
- `pg:confidence_at(?s, ?p, ?o, xsd:dateTime)` — confidence at a specific point in time.

**New management function:**

```sql
SELECT pg_ripple.set_decay_rate(
  predicate_iri => 'http://example.org/salary',
  lambda => 0.01  -- half-life ≈ 69 days
);
```

### Why it matters

CDC-ingested knowledge graphs continuously receive new data; old data should lose weight automatically rather than requiring explicit deletion or temporal queries. This is complementary to the existing `point_in_time()` feature.

### Architecture fit

The confidence side table stores `asserted_at TIMESTAMPTZ`. The `pg:current_confidence()` function applies the decay formula at query time. No background worker is needed for most use cases (decay is lazy).

Per-predicate decay rates are stored in a new `_pg_ripple.decay_rates (predicate_id BIGINT, lambda FLOAT8)` table.

### Complexity

Medium. Primarily the SPARQL function expansion and the per-predicate decay rate storage. No changes to the inference engine.

---

## 8. Feature 6: Subjective Logic Belief Functions

**Milestone: Post-v0.84.0 | Complexity: Large**

### What it is

Jøsang's Subjective Logic (2016) represents uncertainty as a **beta distribution opinion** $(b, d, u, a)$:

- $b$ = belief (probability mass supporting the fact)
- $d$ = disbelief (probability mass against the fact)
- $u$ = uncertainty (remaining probability mass — lack of evidence)
- $a$ = base rate (prior probability if no evidence)

Constraint: $b + d + u = 1$, all $\in [0, 1]$.

A point probability estimate (classical confidence) is a special case where $u = 0$. The value of Subjective Logic is that it distinguishes:

- "I'm 70% confident because evidence supports it" (high $b$, low $u$)
- "I'm 70% confident because I have no evidence" (low $b$, low $d$, high $u$, 70% base rate)

These are epistemically different and should be treated differently in decision-making.

**Operations:**

- **Consensus fusion**: combine opinions from two independent sources.
- **Discounting**: adjust an opinion by the trust in the source that provided it.
- **Deduction**: propagate opinions through a conditional relationship.

### Use cases

- Multi-agent knowledge graphs where different agents contribute beliefs with different certainty levels.
- Trust propagation networks (social trust graphs, supply chain integrity).
- Epistemic uncertainty quantification in medical or regulatory knowledge graphs where "lack of evidence" must be distinguished from "evidence of absence."

### Architecture fit

Opinions are stored as four FLOAT4 columns in an extended confidence side table: `conf_b`, `conf_d`, `conf_u`, `conf_a`. The fusion and discounting operations compile to SQL scalar functions. Existing `pg:confidence()` can return the projected probability $E[x] = b + a \cdot u$ for backwards compatibility.

### Complexity

Large. Requires the confidence side table (Feature 1) and new SQL functions for the three fundamental operations. The grammar for expressing opinions in rule annotations needs design work.

**Reference:** Audun Jøsang, *Subjective Logic: A Formalism for Reasoning Under Uncertainty*, Springer, 2016.

---

## 9. Feature 7: Link Prediction as a SPARQL Query Pattern

**Milestone: Post-v0.84.0 | Complexity: Large**

### What it is

pg_ripple already stores KGE embeddings (TransE/RotatE since v0.57.0) and provides `suggest_sameas()` for entity alignment. The missing piece is exposing link prediction as a first-class SPARQL pattern:

```sparql
SELECT ?target ?probability WHERE {
  BIND(ex:DrugCompoundX AS ?drug)
  BIND(ex:treats AS ?relation)
  pg:linkPredict(?drug, ?relation, ?target, ?probability)
  FILTER(?probability > 0.7)
}
ORDER BY DESC(?probability)
LIMIT 20
```

`pg:linkPredict(subject, predicate, ?object, ?probability)` is a magic predicate that:

1. Looks up the embedding vector for `subject` and `predicate`.
2. Scores all plausible `?object` entities using the KGE scoring function (RotatE: $\|\mathbf{h} \circ \mathbf{r} - \mathbf{t}\|$ in complex space).
3. Returns the top-k candidates above the probability threshold.

**Hybrid queries:**

```sparql
SELECT ?collab ?score WHERE {
  ?collab foaf:name ?name .
  FILTER(pg:fuzzy_match(?name, "CERN") > 0.7)
  pg:linkPredict(ex:MyOrg, ex:collaboratesWith, ?collab, ?score)
}
```

Fuzzy entity discovery + link prediction in a single query.

### Why it matters

- Makes link prediction a first-class citizen rather than a separate ML pipeline.
- Directly actionable in RAG workflows: "find likely relationships not yet in the graph."
- Combines naturally with Feature 2 (fuzzy paths), Feature 1 (confidence propagation), and Feature 4 (provenance weighting).

**Implementation note:** Scoring all entities is O(|E|) per query and must be approximated via HNSW ANN (already available in pgvector). The ULTRA foundation model (Galkin et al., ICLR 2024) provides zero-shot transfer to unseen relation types.

---

## 10. Feature 8: AMIE+ Rule Mining

**Milestone: Post-v0.84.0 | Complexity: Very Large**

### What it is

AMIE+ (Galárraga et al., WWW 2013) automatically learns association rules from a knowledge graph. Given a graph, it discovers rules such as:

```
?X ex:bornIn ?Y, ?Y ex:country ?Z  →  ?X ex:nationality ?Z
  support = 543, confidence = 0.81, PCA confidence = 0.89
```

These rules can then be reviewed by a domain expert and promoted to the Datalog rule set via `add_rule()`. The loop closes naturally with `explain_inference()`.

### New SQL surface

```sql
SELECT * FROM pg_ripple.mine_rules(
  graph_iri      => 'http://example.org/mykg',
  min_support    => 100,
  min_confidence => 0.7,
  max_body_atoms => 3
);
-- Returns: (rule TEXT, support BIGINT, confidence FLOAT8, pca_confidence FLOAT8)
```

### Why it matters

- Dramatically lowers the ontology engineering barrier: users discover rules from data rather than writing them from scratch.
- Complements the existing Datalog reasoner: mined rules can be passed directly to `add_rule()`.
- Closes the loop with Feature 1: mined rules can carry their mined confidence as `@weight()` annotations.

### Architecture fit

AMIE+'s inner loop is essentially "count triples matching a pattern" — equivalent to `SELECT COUNT(*) WHERE { … }` in SPARQL. pg_ripple's own SPARQL engine can execute the search procedure iteratively. The key challenge is bounding the exponential search space via GUC-controlled iteration limits and time budgets.

### Complexity

Very Large. The AMIE+ search is exponential in the maximum rule length and requires careful pruning to remain tractable on large graphs. A staged delivery — first single-atom rules (association rules), then two-atom rules, then three-atom — is recommended.

**Reference:** L. Galárraga et al., "AMIE: Association Rule Mining under Incomplete Evidence in Ontological Knowledge Bases," WWW 2013.

---

## 11. Recommendation Matrix

| Feature | Milestone | Strategic value | Engineering cost | Architecture fit |
|---|---|---|---|---|
| Probabilistic Datalog | **v0.84.0** | High | Large | Excellent |
| Fuzzy SPARQL filtering | **v0.84.0** | High | Medium | Excellent |
| Soft SHACL scoring | **v0.84.0** | Medium | Medium | Excellent |
| Provenance-weighted confidence | **v0.84.0** | High | Small (given #1) | Excellent |
| Temporal confidence decay | Post-1.0 | Medium | Medium | Good |
| Subjective Logic | Post-1.0 | Medium | Large | Good |
| Link prediction in SPARQL | Post-1.0 | High | Large | Good |
| AMIE+ rule mining | Post-1.0 | High | Very Large | Good |

### Rationale for v0.84.0 scope

Features 1–4 compose into a coherent "uncertain knowledge engine":

- **Feature 1** provides the confidence propagation engine and storage substrate.
- **Feature 2** makes the confidence system useful at query time (filter by similarity, follow confident edges).
- **Feature 3** applies confidence thinking to data quality validation.
- **Feature 4** provides a zero-friction on-ramp for data that already has PROV-O metadata.

All four build on existing subsystems (Datalog, SPARQL, SHACL, PROV-O) and require no architectural changes to VP table storage. The confidence side table is additive and gated behind GUCs.

Post-1.0 features are deferred because they require either: new algorithms (AMIE+, link prediction scoring), a different uncertainty representation that needs design validation (Subjective Logic), or close integration with the confidence propagation engine that should stabilise first (temporal decay).

---

## 12. Prior Art and Comparisons

| System | Relevant feature | Notes |
|---|---|---|
| **ProbLog 2** (KU Leuven) | Probabilistic logic programming | Gold standard for exact probabilistic inference; source of the `@weight` / noisy-OR model |
| **Markov Logic Networks** (Richardson & Domingos, 2006) | Weighted first-order logic | Uses log-odds weights; pg_ripple uses direct probability |
| **PARIS** (Suchanek et al., VLDB 2012) | Probabilistic alignment | Confidence propagation for `owl:sameAs` alignment — direct precedent for Feature 4 |
| **NELL** (Carlson et al., 2010) | Large-scale KB with confidences | Closest real-world predecessor at production scale |
| **FuzzySPARQL** (Straccia & Ragone, 2010) | Fuzzy SPARQL | Academic prototype; proposed fuzzy FILTER extensions similar to `pg:fuzzy_match` |
| **Subjective Logic** (Jøsang, 2016) | Belief functions | Foundation for Feature 6 |
| **AMIE+** (Galárraga et al., 2013) | Rule mining | State-of-the-art KG rule miner; foundation for Feature 8 |
| **ULTRA** (Galkin et al., ICLR 2024) | Foundation model KGE | Zero-shot link prediction; relevant to Feature 7 |
| **pg_trgm** | Trigram string similarity | PostgreSQL built-in; foundation for Feature 2 |
| **Stardog** | Weighted integrity constraints | Commercial precedent for soft SHACL; pg_ripple's model is more expressive |
| **GraphDB** | Similarity searches | Commercial precedent for fuzzy graph queries |
