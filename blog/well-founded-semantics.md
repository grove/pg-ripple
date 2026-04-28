[← Back to Blog Index](README.md)

# Well-Founded Semantics: When Your Ontology Has Cycles

## Three-valued logic for the real-world cases where true/false isn't enough

---

Most Datalog programs are well-behaved. Rules derive facts. Facts feed other rules. The process converges to a unique fixpoint where everything is either true or false.

Then someone writes rules with negation that form a cycle:

```
reliable(X) :- sensor(X), NOT faulty(X).
faulty(X) :- sensor(X), NOT reliable(X).
```

Is sensor X reliable or faulty? If it's not faulty, it's reliable. But if it's reliable, it's not faulty. Which makes it reliable. Which makes it not faulty. Which makes it reliable...

This is a classic circular dependency through negation. Standard Datalog has no answer. Stratified negation rejects the program entirely — you can't stratify a negation cycle. But the question is reasonable. A real ontology might have exactly this kind of mutual exclusion.

Well-founded semantics (WFS) gives an answer: **unknown**.

---

## The Three Values

WFS extends Datalog from two-valued logic (true/false) to three-valued logic:

- **True:** The fact is derivable and has a well-founded justification.
- **False:** The fact is not derivable.
- **Unknown:** The fact is involved in a cycle through negation and cannot be definitively classified.

For the sensor example:
- `reliable(sensor7)` → **unknown**
- `faulty(sensor7)` → **unknown**

This is the correct answer. Without additional information, neither classification is justified. The system doesn't guess — it says "I don't know."

---

## Why This Matters for Knowledge Graphs

Real ontologies contain negation cycles more often than you'd expect:

### Closed-World Assumptions
```
verified(X) :- reviewed(X), NOT flagged(X).
flagged(X) :- reported(X), NOT verified(X).
```
An entity is verified if it's been reviewed and not flagged. An entity is flagged if it's been reported and not verified. If an entity has been both reviewed and reported, WFS correctly marks both as unknown — pending human resolution.

### Default Reasoning
```
typical_bird(X) :- bird(X), NOT abnormal(X).
abnormal(X) :- bird(X), NOT typical_bird(X).
```
Without explicit evidence of abnormality, WFS gives unknown rather than arbitrarily choosing one. Additional facts (e.g., `penguin(X)` → `abnormal(X)`) break the cycle and produce definite answers.

### Competitive Classification
```
eligible_for_A(X) :- applicant(X), NOT eligible_for_B(X).
eligible_for_B(X) :- applicant(X), NOT eligible_for_A(X).
```
An applicant can't be in both programs. Without a tie-breaker, WFS correctly says both are unknown.

---

## How pg_ripple Implements WFS

pg_ripple's WFS implementation (since v0.32.0) uses the alternating fixpoint algorithm:

1. **Optimistic pass:** Assume all negated atoms are false (i.e., nothing is negated). Compute the fixpoint.
2. **Pessimistic pass:** Assume all atoms not derived in step 1 are false. Recompute.
3. **Iterate** until the optimistic and pessimistic fixpoints converge.

Facts that are true in both passes are **true**. Facts that are false in both passes are **false**. Facts that differ between passes are **unknown**.

The SQL implementation uses a pair of temporary tables (optimistic and pessimistic) per derived predicate, and iterates the fixpoint computation until both tables stabilize.

---

## Querying Unknown Facts

After inference with WFS, you can query each truth value:

```sql
-- Find all definitely true facts
SELECT * FROM pg_ripple.sparql('
  SELECT ?sensor WHERE {
    ?sensor ex:reliable true .
  }
');

-- Find all unknown facts (need human review)
SELECT * FROM pg_ripple.datalog_query_unknown('reliable(?sensor)');
```

The unknown facts are a natural work queue: they represent cases where the system's rules are insufficient and human judgment is needed.

---

## WFS vs. Stratification

| Aspect | Stratified Negation | Well-Founded Semantics |
|--------|--------------------|-----------------------|
| Handles negation cycles | No (rejects the program) | Yes (assigns unknown) |
| Performance | Faster (no iteration) | Slower (alternating fixpoint) |
| Expressiveness | Limited (must be stratifiable) | Full (any Datalog with negation) |
| Answer completeness | Incomplete (rejects valid programs) | Complete (answers every query) |

pg_ripple uses stratified negation by default (it's faster). When a rule set contains negation cycles that can't be stratified, it automatically falls back to WFS. You can also force WFS:

```sql
SET pg_ripple.datalog_semantics = 'well_founded';
SELECT pg_ripple.datalog_infer();
```

---

## The Performance Trade-Off

WFS is more expensive than stratified evaluation. The alternating fixpoint requires multiple passes over the data, and convergence can take 10–20 iterations for complex programs.

On a benchmark with 5 million triples and 50 rules (10 with negation cycles):

| Approach | Inference time | Correctness |
|----------|---------------|-------------|
| Stratified (rejects cycles) | 2.1 seconds | Incomplete (15 rules rejected) |
| Well-founded | 8.4 seconds | Complete (all rules evaluated) |

The 4× slowdown is the price of completeness. For ontologies where negation cycles are rare (most OWL RL programs), stratification handles 95% of rules and WFS handles the remaining 5%. The hybrid approach keeps the common case fast while providing correct answers for the hard cases.

---

## When to Use WFS

- **Default reasoning.** "Birds typically fly" with exceptions. WFS handles the exceptions correctly.
- **Mutual exclusion constraints.** "An entity can be A or B but not both." WFS marks undecidable cases as unknown.
- **Multi-source integration.** Different sources assert contradictory negations. WFS doesn't pick a winner — it flags the conflict.
- **Regulatory compliance.** When rules interact in complex ways, WFS ensures that only well-founded conclusions are reported as true. Unknown is better than wrong.

pg_ripple is one of very few systems that implements WFS inside a relational database. For most Datalog workloads, you'll never see it — stratification handles the common case. But when your ontology has the hard cases, WFS is there.
