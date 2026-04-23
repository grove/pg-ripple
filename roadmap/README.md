# pg_ripple — Roadmap Feature Descriptions

This directory contains plain-language descriptions of pg_ripple's upcoming releases. The goal is to give a clear picture of what each version delivers, why it matters, and how much work is involved — without requiring deep technical knowledge.

---

## What is pg_ripple?

pg_ripple is a database extension for PostgreSQL that lets you store and query knowledge graphs — structured networks of facts expressed as subject → predicate → object triples. It supports SPARQL (the standard query language for knowledge graphs), SHACL (data validation rules), and Datalog (inference rules). It also integrates with AI/LLM tooling for question-answering over graph data.

---

## Roadmap Overview

| Version | Theme | Key Deliverables | Estimated Effort |
|---|---|---|---|
| **[v0.51.0](v0.51.0.md)** | Security Hardening & Production Readiness | Non-root container, SPARQL DoS limits, HTTP streaming, OTLP tracing, pg_upgrade docs, OWL 2 RL completion | 8–10 pw |
| **[v0.52.0](v0.52.0.md)** | DX, Extended Standards & Architecture | SHACL-SPARQL, `COPY rdf FROM`, RAG hardening, OpenAPI spec, CDC lifecycle events, code quality splits | 6–9 pw |
| **[v0.53.0](v0.53.0.md)** | High Availability & Logical Replication | RDF logical replication, Helm chart, vector index benchmarks | 5–7 pw |
| **v1.0.0** | Production Release | Final conformance, stress test, security audit, API stability guarantee | 6–8 pw |

**Total estimated effort to v1.0.0 from the current state (v0.50.0): 25–34 person-weeks**
