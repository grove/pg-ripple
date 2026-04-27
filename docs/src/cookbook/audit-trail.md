# Cookbook: Audit Trail with PROV-O and Temporal Queries

**Goal.** Build an evidence chain that lets you answer regulator questions like *"on 21 March, did your system tell user X that fact Y was true? On what evidence?"*.

**Why pg_ripple.** Three composable features — `point_in_time`, `prov_enabled`, `audit_log_enabled` — combine into the kind of audit trail that pure-ML pipelines cannot produce. Plus RDF-star for per-fact provenance.

**Time to first result.** ~10 minutes.

---

## Step 1 — Turn on every layer

```sql
ALTER SYSTEM SET pg_ripple.prov_enabled       = on;   -- per-load PROV-O
ALTER SYSTEM SET pg_ripple.audit_log_enabled  = on;   -- per-UPDATE log
SELECT pg_reload_conf();
```

The third layer — RDF-star quoted triples for per-fact confidence/source — is loaded with the data itself.

## Step 2 — Load data with per-fact provenance

```sql
SELECT pg_ripple.load_turtle($TTL$
@prefix ex:   <https://example.org/> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .

ex:drugA ex:interactsWith ex:drugB .

# Annotate the fact itself.
<< ex:drugA ex:interactsWith ex:drugB >>
    ex:source     <https://pubmed.example/article/12345> ;
    ex:confidence "0.92"^^xsd:decimal ;
    ex:assertedAt "2026-03-15T09:00:00Z"^^xsd:dateTime ;
    prov:wasAttributedTo ex:loader/medkb-v3 .
$TTL$);
```

## Step 3 — Time passes and data changes

```sql
-- Several updates happen over the next month.
SELECT pg_ripple.sparql_update('
    INSERT DATA {
        <https://example.org/drugA> <https://example.org/manufacturer>
            <https://example.org/acme>
    }
');

-- A different role makes a correction.
SET ROLE editor_alice;
SELECT pg_ripple.sparql_update('
    DELETE DATA { <https://example.org/drugA> <https://example.org/manufacturer>
                  <https://example.org/acme> }
');
SELECT pg_ripple.sparql_update('
    INSERT DATA { <https://example.org/drugA> <https://example.org/manufacturer>
                  <https://example.org/acmecorp> }
');
RESET ROLE;
```

## Step 4 — Answer the regulator

The question is: *"On 21 March, did your system tell users that drug A interacts with drug B? On what evidence?"*

```sql
-- 1. Replay the graph as of 21 March 12:00.
SELECT pg_ripple.point_in_time('2026-03-21 12:00:00+00');

-- 2. Re-ask the question.
SELECT * FROM pg_ripple.sparql($$
    ASK { <https://example.org/drugA>
          <https://example.org/interactsWith>
          <https://example.org/drugB> }
$$);
-- → true

-- 3. Pull the per-fact evidence (RDF-star).
SELECT * FROM pg_ripple.sparql($$
    SELECT ?source ?confidence ?assertedAt WHERE {
        << <https://example.org/drugA>
           <https://example.org/interactsWith>
           <https://example.org/drugB> >>
            <https://example.org/source>     ?source ;
            <https://example.org/confidence> ?confidence ;
            <https://example.org/assertedAt> ?assertedAt .
    }
$$);

-- 4. Pull the loader activity (PROV-O).
SELECT * FROM pg_ripple.prov_stats()
WHERE  source_file LIKE '%medkb%';

-- 5. Pull every UPDATE since the load (audit log).
SELECT ts, role, operation, query
FROM   _pg_ripple.audit_log
WHERE  ts >= '2026-03-15'
   AND query ILIKE '%drugA%'
ORDER  BY ts;

-- 6. Reset point-in-time to "now".
SELECT pg_ripple.point_in_time(NULL);
```

That sequence is the kind of evidence chain a regulator looks for: *truth value at a point in time*, *evidence per fact*, *attribution per UPDATE*.

---

## Building a one-shot audit report

Wrap the queries above in a SQL function so the compliance team can run a single command:

```sql
CREATE FUNCTION audit_report(
    fact_subject  TEXT,
    fact_predicate TEXT,
    fact_object   TEXT,
    as_of         TIMESTAMPTZ
)
RETURNS TABLE (kind TEXT, payload JSONB) AS $$
BEGIN
    PERFORM pg_ripple.point_in_time(as_of);

    RETURN QUERY
        SELECT 'truth_value'::TEXT,
               jsonb_build_object('asof', as_of, 'value', (
                   SELECT bindings FROM pg_ripple.sparql(
                       format('ASK { %s %s %s }', fact_subject, fact_predicate, fact_object)
                   )
               ));

    RETURN QUERY
        SELECT 'rdf_star_evidence'::TEXT, to_jsonb(s)
        FROM   pg_ripple.sparql(format($q$
            SELECT ?p ?o WHERE {
                << %s %s %s >> ?p ?o
            }
        $q$, fact_subject, fact_predicate, fact_object)) s;

    RETURN QUERY
        SELECT 'audit_log'::TEXT, to_jsonb(a)
        FROM   _pg_ripple.audit_log a
        WHERE  ts <= as_of AND query ILIKE '%' || fact_subject || '%'
        ORDER  BY ts;

    PERFORM pg_ripple.point_in_time(NULL);
END;
$$ LANGUAGE plpgsql;
```

```sql
SELECT * FROM audit_report(
    '<https://example.org/drugA>',
    '<https://example.org/interactsWith>',
    '<https://example.org/drugB>',
    '2026-03-21 12:00:00+00'
);
```

---

## See also

- [Temporal & Provenance](../features/temporal-and-provenance.md)
- [Audit Log](../reference/audit-log.md)
- [Storing Knowledge — RDF-star](../features/storing-knowledge.md)
