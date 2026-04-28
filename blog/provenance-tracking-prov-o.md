[← Back to Blog Index](README.md)

# Automatic Provenance Tracking with PROV-O

## Every bulk load, every inference run, every source file — tracked as queryable RDF

---

Someone asks: "Where did this triple come from?" In most databases, the answer is "I don't know — it was loaded at some point by someone."

pg_ripple can do better. When provenance tracking is enabled, every data operation generates PROV-O triples that record who did what, when, and from which source. The provenance is itself stored as RDF — queryable with the same SPARQL you use for everything else.

---

## Enabling Provenance

```sql
SET pg_ripple.prov_enabled = on;
```

From this point, every bulk load, SPARQL Update, or Datalog inference run generates PROV-O triples in a dedicated provenance graph.

---

## What Gets Tracked

### Bulk Loads

```sql
SELECT pg_ripple.load_turtle_file('/data/employees.ttl');
```

Generates:

```turtle
# In the provenance graph
_:activity1 a prov:Activity ;
  prov:startedAtTime "2026-04-28T14:32:00Z"^^xsd:dateTime ;
  prov:endedAtTime "2026-04-28T14:32:03Z"^^xsd:dateTime ;
  prov:wasAssociatedWith <pg_ripple:session/42> ;
  prov:used <file:///data/employees.ttl> ;
  prov:generated _:entity1 .

_:entity1 a prov:Entity ;
  prov:wasGeneratedBy _:activity1 ;
  prov:atLocation "http://example.org/default_graph" ;
  ex:tripleCount 15000 .

<file:///data/employees.ttl> a prov:Entity ;
  prov:atLocation "/data/employees.ttl" ;
  ex:format "text/turtle" ;
  ex:sizeBytes 1048576 .
```

This records:
- **What happened:** A load activity processed a Turtle file.
- **When:** Start and end timestamps.
- **Who:** The PostgreSQL session that ran the command.
- **What was consumed:** The source file (path, format, size).
- **What was produced:** A collection of triples in the default graph.

### SPARQL Updates

```sql
SELECT pg_ripple.sparql_update('
  INSERT DATA { ex:alice ex:department ex:product . }
');
```

Generates:

```turtle
_:activity2 a prov:Activity ;
  prov:startedAtTime "2026-04-28T14:35:00Z"^^xsd:dateTime ;
  rdfs:comment "SPARQL INSERT DATA" ;
  prov:wasAssociatedWith <pg_ripple:session/42> .
```

### Datalog Inference

```sql
SELECT pg_ripple.datalog_infer();
```

Generates:

```turtle
_:activity3 a prov:Activity ;
  prov:startedAtTime "2026-04-28T14:36:00Z"^^xsd:dateTime ;
  prov:endedAtTime "2026-04-28T14:36:08Z"^^xsd:dateTime ;
  rdfs:comment "Datalog inference: owl2rl ruleset" ;
  ex:newFacts 2400 ;
  ex:iterations 12 ;
  ex:strata 3 .
```

---

## Querying Provenance

The provenance triples are standard RDF, queryable with SPARQL:

```sql
-- "What data was loaded from this file?"
SELECT * FROM pg_ripple.sparql('
  SELECT ?activity ?time ?triples WHERE {
    GRAPH <pg_ripple:provenance> {
      ?activity prov:used <file:///data/employees.ttl> ;
                prov:startedAtTime ?time ;
                prov:generated ?entity .
      ?entity ex:tripleCount ?triples .
    }
  }
  ORDER BY DESC(?time)
');

-- "What operations happened today?"
SELECT * FROM pg_ripple.sparql('
  SELECT ?activity ?comment ?time WHERE {
    GRAPH <pg_ripple:provenance> {
      ?activity a prov:Activity ;
                prov:startedAtTime ?time .
      OPTIONAL { ?activity rdfs:comment ?comment . }
      FILTER(?time >= "2026-04-28"^^xsd:date)
    }
  }
  ORDER BY ?time
');

-- "Which source contributed the most triples?"
SELECT * FROM pg_ripple.sparql('
  SELECT ?source (SUM(?count) AS ?total) WHERE {
    GRAPH <pg_ripple:provenance> {
      ?activity prov:used ?source ;
                prov:generated ?entity .
      ?entity ex:tripleCount ?count .
    }
  }
  GROUP BY ?source
  ORDER BY DESC(?total)
');
```

---

## Provenance for Compliance

In regulated industries (healthcare, finance, government), data lineage is not optional:

- **HIPAA:** Track who loaded patient data and from which system.
- **SOX:** Audit trail for financial data modifications.
- **GDPR:** Record which data sources contributed to a subject's profile (needed for right-to-access requests).

pg_ripple's PROV-O tracking provides this lineage automatically. No application-level logging needed. The provenance graph is the audit trail.

---

## Aggregate Statistics

For a quick overview without SPARQL:

```sql
SELECT * FROM pg_ripple.prov_stats();
```

Returns:

| metric | value |
|--------|-------|
| total_activities | 127 |
| total_sources | 15 |
| total_triples_loaded | 12,500,000 |
| total_inferred | 3,200,000 |
| earliest_activity | 2026-01-15T08:00:00Z |
| latest_activity | 2026-04-28T14:36:08Z |

---

## Storage Cost

Provenance triples are lightweight — typically 5–10 triples per operation. For a graph that's loaded from 100 files and runs inference 50 times, the provenance graph has ~1,000 triples. Negligible compared to the data graph.

For very high-frequency operations (e.g., CDC-driven updates that fire thousands of SPARQL Updates per hour), provenance can be configured to aggregate:

```sql
SET pg_ripple.prov_granularity = 'batch';  -- One activity per batch, not per statement
```

---

## Provenance + GDPR Erasure

When `erase_subject()` removes a person's data, the provenance records for that data are also removed — but a tombstone record is kept:

```turtle
_:erasure1 a prov:Activity ;
  rdfs:comment "GDPR erasure: http://example.org/alice" ;
  prov:startedAtTime "2026-04-28T15:00:00Z"^^xsd:dateTime ;
  ex:erasedTripleCount 47 .
```

The tombstone records the fact of erasure (when, how many triples) without retaining the erased data. This satisfies the GDPR requirement to prove erasure occurred while not keeping the data you're supposed to erase.
