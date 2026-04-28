[← Back to Blog Index](README.md)

# Probabilistic Datalog: Soft Rules for Uncertain Knowledge

## When 95% confidence is good enough — fuzzy reasoning inside PostgreSQL

---

Classical Datalog deals in absolutes. A fact is true or it's not. A rule fires or it doesn't. There's no "probably" or "with 95% confidence."

Real-world knowledge is messier. An NLP extraction says "Company A acquired Company B" with 87% confidence. A sensor reading is "anomalous" with 72% probability. A medical guideline says a drug interaction is "likely" but not certain.

pg_ripple's probabilistic Datalog mode lets you attach confidence weights to rules and propagate them through inference chains.

---

## Weighted Rules

```sql
SET pg_ripple.probabilistic_datalog = on;

-- If someone is a Manager, they're probably (95%) also a DecisionMaker
SELECT pg_ripple.datalog_add_rule(
  'decision_maker(X) :- manager(X). @weight(0.95)'
);

-- If someone is a DecisionMaker and in Finance, they're probably (90%) a BudgetApprover
SELECT pg_ripple.datalog_add_rule(
  'budget_approver(X) :- decision_maker(X), department(X, finance). @weight(0.90)'
);

SELECT pg_ripple.datalog_infer();
```

After inference:
- Alice is a Manager (explicit, confidence = 1.0)
- Alice is a DecisionMaker (inferred, confidence = 0.95)
- Alice is in Finance (explicit, confidence = 1.0)
- Alice is a BudgetApprover (inferred, confidence = 0.95 × 0.90 = 0.855)

The confidence propagates multiplicatively through the derivation chain. A chain of three 90% rules produces a 72.9% conclusion — which is still useful but properly discounted.

---

## How Confidence Propagates

The propagation model follows Markov Logic Network principles:

- **Conjunction (multiple body atoms):** Multiply the confidences. `a(X) :- b(X), c(X). @weight(0.9)` with `b(X)` at 0.95 and `c(X)` at 0.8 produces `a(X)` at 0.9 × 0.95 × 0.8 = 0.684.

- **Multiple derivations:** If the same fact can be derived through multiple rule paths, the confidences are combined using noisy-OR: $p = 1 - \prod(1 - p_i)$. Two independent derivations at 0.7 produce a combined confidence of 1 - (0.3 × 0.3) = 0.91.

- **Base facts:** Explicit triples have confidence 1.0 by default. You can assign lower confidence to facts from unreliable sources:

```sql
SELECT pg_ripple.load_ntriples_with_confidence(
  '<ex:sensor7> <ex:reports> <ex:anomaly42> .',
  confidence => 0.72
);
```

---

## Querying with Confidence

```sql
SELECT * FROM pg_ripple.sparql('
  SELECT ?person ?confidence WHERE {
    ?person ex:budget_approver true .
    BIND(pg:confidence(?person, ex:budget_approver) AS ?confidence)
  }
  ORDER BY DESC(?confidence)
');
```

Returns:

| person | confidence |
|--------|-----------|
| ex:alice | 0.855 |
| ex:bob | 0.760 |
| ex:carol | 0.513 |

You can filter on confidence:

```sql
SELECT * FROM pg_ripple.sparql('
  SELECT ?person WHERE {
    ?person ex:budget_approver true .
    FILTER(pg:confidence(?person, ex:budget_approver) > 0.8)
  }
');
```

This returns only high-confidence inferences — useful when the downstream system needs to make decisions and can't tolerate uncertainty below a threshold.

---

## Use Cases

### NLP-Extracted Knowledge

An NLP pipeline extracts relationships from documents with varying confidence:

```turtle
<< ex:companyA ex:acquired ex:companyB >> ex:confidence 0.87 ;
                                          ex:source ex:article_42 .
<< ex:companyA ex:acquired ex:companyC >> ex:confidence 0.62 ;
                                          ex:source ex:article_17 .
```

Datalog rules that derive facts from these extractions inherit the confidence:

```
competitor(X, Y) :- acquired(X, Z), acquired(Y, Z), X != Y. @weight(0.85)
```

If Company A and Company D both acquired Company B, they're probably competitors — but only if the acquisition facts are themselves confident. The chain: 0.87 × 0.85 = 0.74 for one acquisition, factored against the other's confidence.

### Sensor Fusion

Multiple sensors report on the same phenomenon:

```
high_temperature(Zone) :- sensor_reading(S, Zone, Temp), Temp > 40. @weight(0.90)
fire_risk(Zone) :- high_temperature(Zone), dry_conditions(Zone). @weight(0.80)
```

A single sensor reporting 41°C at 90% confidence gives a fire risk of 0.90 × 0.80 = 0.72. Two independent sensors both reporting high temperature produce a combined high_temperature confidence of 0.99 (noisy-OR), giving fire risk of 0.99 × 0.80 = 0.79.

### Medical Decision Support

```
drug_interaction(D1, D2) :- same_cyp_enzyme(D1, E), same_cyp_enzyme(D2, E). @weight(0.75)
contraindicated(D1, D2) :- drug_interaction(D1, D2), severity(D1, D2, major). @weight(0.95)
```

The confidence chain ensures that low-confidence interactions don't produce high-confidence contraindications. A physician sees the confidence score and uses clinical judgment.

---

## Mixing Crisp and Probabilistic Rules

Not all rules need weights. OWL RL rules (subclass inference, transitivity) are crisp — they're logically certain. You can mix crisp and probabilistic rules in the same program:

```sql
-- Crisp: RDFS subclass (certain)
SELECT pg_ripple.datalog_load_ruleset('rdfs');

-- Probabilistic: domain-specific (uncertain)
SELECT pg_ripple.datalog_add_rule(
  'expert_in(X, Topic) :- authored(X, Paper), about(Paper, Topic). @weight(0.70)'
);
```

Crisp rules propagate with confidence 1.0. Probabilistic rules propagate with their specified weight. The engine handles both in the same fixpoint computation.

---

## Limitations

Probabilistic Datalog in pg_ripple is not a full probabilistic programming language. Specifically:

- **No learning.** Weights are specified by the user, not learned from data. If you need weight learning, train externally (e.g., with Markov Logic Network tools) and load the learned weights.
- **Independence assumption.** The noisy-OR combination assumes derivation paths are independent. Correlated evidence is double-counted.
- **No continuous distributions.** Confidence is a single float, not a distribution. If you need Bayesian reasoning with prior/posterior updates, pg_ripple isn't the right tool.

For the common case — attaching confidence to derived facts and filtering by confidence threshold — these limitations are acceptable. The alternative is treating all inferences as equally certain, which is strictly worse.
