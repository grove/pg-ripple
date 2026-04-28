[← Back to Blog Index](README.md)

# Neuro-Symbolic Entity Resolution

## Combining ML candidates with logical constraints for safe, explainable entity merging

---

Entity resolution — deciding that two records refer to the same real-world entity — is one of the hardest problems in data integration. Pure ML approaches (embedding similarity, learned matchers) have high recall but produce false merges that corrupt your data. Pure rule-based approaches (exact match on email, name similarity thresholds) have high precision but miss non-obvious matches.

pg_ripple combines both: ML models propose candidates, SHACL constraints veto impossible merges, Datalog rules propagate transitivity, and the audit trail explains every decision. This is neuro-symbolic entity resolution — and it runs inside PostgreSQL.

---

## The Pipeline

```
   ML Model                    Logic Layer                   Storage
   ────────                    ───────────                   ───────

   Embedding         ──────▶  SHACL                ──────▶  owl:sameAs
   similarity                  veto check                    canonicalization
   (high recall)               (safety gate)                 (union-find)
                                    │
   String                          │
   similarity        ──────▶  Datalog              ──────▶  Derivation
   (fuzzy match)               transitivity                  audit log
                               propagation
```

### Stage 1: ML Candidate Generation

pg_ripple's KGE embeddings (TransE/RotatE) encode graph structure into vectors. `find_alignments()` computes cosine similarity between entities across two graphs:

```sql
SELECT * FROM pg_ripple.find_alignments(
  source_graph => 'http://example.org/source_a',
  target_graph => 'http://example.org/source_b',
  threshold    => 0.75,
  limit        => 1000
);
```

Returns:

| source_entity | target_entity | similarity |
|--------------|--------------|-----------|
| src:customer_42 | tgt:client_007 | 0.92 |
| src:customer_88 | tgt:client_123 | 0.87 |
| src:customer_15 | tgt:client_456 | 0.78 |

These are candidates — entities that look similar based on their graph neighborhood. But similarity isn't identity. `customer_42` and `client_007` might refer to different people who happen to have similar relationships.

### Stage 2: SHACL Veto

Before accepting a candidate merge, SHACL constraints check for logical impossibility:

```turtle
ex:MergeConstraint a sh:NodeShape ;
  sh:targetSubjectsOf owl:sameAs ;
  sh:sparql [
    sh:select """
      SELECT $this WHERE {
        $this owl:sameAs ?other .
        $this schema:birthDate ?d1 .
        ?other schema:birthDate ?d2 .
        FILTER(?d1 != ?d2)
      }
    """ ;
    sh:message "Cannot merge: different birth dates" ;
    sh:severity sh:Violation ;
  ] .
```

If `customer_42` was born on 1985-03-15 and `client_007` was born on 1990-07-22, they're not the same person — regardless of what the embedding similarity says. The SHACL constraint vetoes the merge.

Other veto rules:
- Different genders (if known)
- Different nationalities (for certain entity types)
- Mutual exclusion constraints from the ontology
- Business rules (e.g., "a customer can't be their own supplier")

The ML model optimizes for recall (find all possible matches). SHACL optimizes for precision (reject impossible matches). Together, they produce high-recall, high-precision alignment.

### Stage 3: Datalog Propagation

Accepted merges are asserted as `owl:sameAs`:

```sql
SELECT pg_ripple.sparql_update('
  INSERT DATA {
    src:customer_42 owl:sameAs tgt:client_007 .
    src:customer_88 owl:sameAs tgt:client_123 .
  }
');
```

Datalog's OWL RL rules propagate transitivity:

```
-- If A sameAs B and B sameAs C, then A sameAs C
owl_sameAs(X, Z) :- owl_sameAs(X, Y), owl_sameAs(Y, Z).
```

This means: if source A's `customer_42` matches target B's `client_007`, and target B's `client_007` matches source C's `kontakt_42`, all three are automatically linked.

### Stage 4: Canonicalization

pg_ripple's union-find canonicalization rewrites all merged entities to a single canonical ID. Subsequent SPARQL queries see one entity, not three aliases.

### Stage 5: Audit Trail

Every merge decision is logged with:
- The candidate pair
- The similarity score
- The SHACL validation result (pass/veto)
- The propagation chain (if transitivity was used)
- The timestamp

```sql
SELECT * FROM pg_ripple.sparql('
  SELECT ?entity1 ?entity2 ?score ?decision ?reason WHERE {
    GRAPH <pg_ripple:merge_audit> {
      ?merge ex:source ?entity1 ;
             ex:target ?entity2 ;
             ex:similarity ?score ;
             ex:decision ?decision ;
             ex:reason ?reason .
    }
  }
  ORDER BY DESC(?score)
');
```

The audit trail is queryable with SPARQL — you can answer "why were these entities merged?" or "which merges were vetoed and why?"

---

## The Confidence Cascade

The pipeline naturally produces confidence levels:

- **High confidence (> 0.9 similarity, SHACL pass):** Auto-merge. No human review needed.
- **Medium confidence (0.75–0.9, SHACL pass):** Queue for human review.
- **Low confidence (< 0.75):** Discard.
- **SHACL veto (any similarity):** Reject regardless of score.

```sql
-- Auto-merge high-confidence candidates
SELECT pg_ripple.auto_merge_candidates(
  min_similarity => 0.9,
  require_shacl_pass => true
);

-- Queue medium-confidence for review
SELECT source_entity, target_entity, similarity
FROM pg_ripple.find_alignments(...)
WHERE similarity BETWEEN 0.75 AND 0.9;
```

---

## Why Not Just ML?

Pure ML entity resolution (learned matchers, Magellan, DeepMatcher) achieves 85–95% F1 on benchmarks. But the 5–15% error rate produces false merges that corrupt downstream data:

- Two patients merged → wrong medication prescribed.
- Two companies merged → incorrect financial reporting.
- Two products merged → wrong inventory counts.

In production, a false merge is much worse than a missed match. A missed match means data stays siloed (inconvenient). A false merge means data is wrong (dangerous).

The SHACL veto layer catches the dangerous false merges. It can't catch all of them — some false merges are logically consistent but factually wrong. But it catches the obvious ones (different birth dates, different locations, violated constraints), which are the majority of false positives from ML models.

---

## Why Not Just Rules?

Pure rule-based entity resolution (exact email match, name + birthdate match) is safe but limited:

- Different name spellings (Robert vs. Bob) are missed.
- Records without overlapping attributes can't be matched.
- Threshold tuning is fragile — 0.85 Jaccard on names catches too many, 0.95 catches too few.

The ML model handles fuzzy matching that rules can't express. Entities that are structurally similar in the graph (similar neighbors, similar types, similar property distributions) are aligned even when no single attribute matches exactly.

The combination — ML for recall, logic for precision — is strictly better than either alone. pg_ripple provides both in the same database, using the same graph data, without external ML infrastructure.
