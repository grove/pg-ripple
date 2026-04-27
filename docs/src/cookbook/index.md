# Use Case Cookbook

The chapters under **Feature Deep Dives** explain *what each pg_ripple feature does*. The recipes in this cookbook explain *what you can do with the features chained together*. Each recipe is a self-contained story: a real-world goal, the step-by-step SQL, and the trade-offs to be aware of.

If you are evaluating pg_ripple, start here — these are the patterns that decide whether the technology fits your problem.

| Recipe | What you build | Features used |
|---|---|---|
| [Knowledge graph from a relational catalogue](relational-to-rdf.md) | A queryable RDF graph generated from existing PostgreSQL tables, validated and kept in sync | R2RML, SHACL, named graphs |
| [Chatbot grounded in a knowledge graph](grounded-chatbot.md) | An LLM application that answers questions using your graph as authoritative context | RAG pipeline, NL→SPARQL, JSON-LD framing |
| [Deduplicate customer records across systems](dedupe-customers.md) | A safe, auditable record-linkage pipeline that merges customer rows from two or more sources | KGE, suggest_sameas, SHACL hard rules, owl:sameAs |
| [Audit trail with PROV-O and temporal queries](audit-trail.md) | A regulator-defensible chain showing what the system told a user, when, and why | PROV-O, audit log, point_in_time, RDF-star |
| [CDC → Kafka via JSON-LD outbox](cdc-to-kafka.md) | A stream of structured graph-change events ready to push into Kafka, NATS, or any event bus | CDC subscriptions, JSON-LD framing, transactional outbox |
| [Probabilistic rules for soft constraints](probabilistic-rules.md) | A scoring rule set that propagates confidence values, not just facts | Lattice Datalog, RDF-star confidence triples |
| [SPARQL repair workflow](sparql-repair.md) | An iterative loop that uses the LLM to fix queries that failed to parse or returned no results | sparql_from_nl, explain_sparql, error catalog |
| [Ontology mapping and alignment](ontology-mapping.md) | A pipeline that lifts external vocabularies into a local schema using KGE and SHACL | KGE, suggest_sameas, OWL profiles |
