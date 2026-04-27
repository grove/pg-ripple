"""
dbt-pg-ripple: dbt adapter for pg_ripple RDF triple store.

Provides SPARQL-aware macros (sparql_model, sparql_source, sparql_ref)
that let data engineers mix SQL and SPARQL in the same dbt project.
"""

from dbt_pg_ripple.adapter import PgRippleAdapter

__version__ = "0.61.0"
__all__ = ["PgRippleAdapter"]
