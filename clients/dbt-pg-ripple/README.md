# dbt-pg-ripple

A [dbt](https://www.getdbt.com) adapter for [pg_ripple](https://github.com/pg-ripple/pg-ripple2),
the PostgreSQL RDF triple store with native SPARQL query execution.

## Overview

`dbt-pg-ripple` extends the standard `dbt-postgres` adapter with SPARQL-aware
macros that let data engineers mix SQL and SPARQL transformations in the same
dbt project.

## SPARQL macros

### `sparql_model`

Define a dbt model whose source is a SPARQL SELECT query:

```sql
-- models/persons.sql
{{ config(materialized='table') }}
{{ sparql_model(
    query="SELECT ?s ?name WHERE { ?s <https://schema.org/name> ?name }",
    columns=["s TEXT", "name TEXT"]
) }}
```

### `sparql_source`

Reference a pg_ripple SPARQL query as a source in another model:

```sql
-- models/enriched_persons.sql
WITH persons AS (
    {{ sparql_source(ref='persons') }}
)
SELECT * FROM persons WHERE name != ''
```

### `sparql_ref`

Reference a named graph as a dbt source:

```sql
{{ sparql_ref(graph='https://hr.example.org/employees') }}
```

## Installation

```bash
pip install dbt-pg-ripple
```

Or from source:

```bash
cd clients/dbt-pg-ripple
pip install -e .
```

## Configuration

In your `profiles.yml`:

```yaml
my_project:
  target: dev
  outputs:
    dev:
      type: pg_ripple
      host: localhost
      port: 5432
      user: postgres
      password: ""
      dbname: my_ripple_db
      schema: public
      threads: 4
```

## Requirements

- Python 3.8+
- dbt-core >= 1.5.0
- PostgreSQL 18 with pg_ripple >= 0.61.0

## License

Apache 2.0 — see [LICENSE](../../LICENSE).
