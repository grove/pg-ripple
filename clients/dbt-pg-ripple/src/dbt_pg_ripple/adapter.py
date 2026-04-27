"""
PgRippleAdapter: dbt-postgres subclass with SPARQL-aware extensions.
"""
from __future__ import annotations

from typing import Any

try:
    from dbt.adapters.postgres import PostgresAdapter
    from dbt.adapters.postgres.relation import PostgresRelation
except ImportError as exc:  # pragma: no cover
    raise ImportError("dbt-postgres must be installed to use dbt-pg-ripple") from exc


class PgRippleAdapter(PostgresAdapter):
    """dbt adapter for pg_ripple.

    Inherits all PostgreSQL adapter functionality and adds SPARQL-aware macro
    helpers so that dbt models can call pg_ripple.sparql() directly.
    """

    ConnectionManager = PostgresAdapter.ConnectionManager  # type: ignore[misc]

    @classmethod
    def date_function(cls) -> str:
        return "now()"

    def sparql_model(
        self,
        query: str,
        columns: list[str] | None = None,
        graph: str | None = None,
    ) -> str:
        """Return a SQL expression wrapping a SPARQL SELECT as a table function.

        Args:
            query:   SPARQL SELECT query string.
            columns: Optional list of column definitions (e.g. ["s TEXT", "name TEXT"]).
            graph:   Optional named graph IRI to scope the query.

        Returns:
            SQL string: ``SELECT * FROM pg_ripple.sparql($$…$$) AS t(…)``
        """
        if graph:
            scoped = f"SELECT * WHERE {{ GRAPH <{graph}> {{ {query} }} }}"
        else:
            scoped = query

        if columns:
            col_def = ", ".join(columns)
            return f"SELECT * FROM pg_ripple.sparql($${scoped}$$) AS t({col_def})"
        return f"SELECT * FROM pg_ripple.sparql($${scoped}$$)"

    def sparql_source(self, ref: str) -> str:
        """Reference a previously defined sparql_model by name."""
        return f"SELECT * FROM {{{{ ref('{ref}') }}}}"

    def sparql_ref(self, graph: str) -> str:
        """Reference all triples in a named graph as a SQL relation."""
        return (
            f"SELECT * FROM pg_ripple.sparql("
            f"'SELECT ?s ?p ?o WHERE {{ GRAPH <{graph}> {{ ?s ?p ?o }} }}')"
        )
