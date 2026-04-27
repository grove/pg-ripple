"""Tests for dbt-pg-ripple adapter."""
import pytest

from dbt_pg_ripple.adapter import PgRippleAdapter


class TestSparqlMacros:
    """Unit tests for SPARQL macro helpers (no database required)."""

    def setup_method(self) -> None:
        # We test macro string generation without a live DB connection.
        # PgRippleAdapter is not instantiated here; we test static methods.
        pass

    def test_sparql_model_simple(self) -> None:
        """sparql_model() wraps query in pg_ripple.sparql()."""

        class _A(PgRippleAdapter):  # type: ignore[misc]
            pass

        # Use the method as an unbound callable to avoid needing a real adapter.
        result = PgRippleAdapter.sparql_model(
            None,  # type: ignore[arg-type]
            "SELECT ?s ?name WHERE { ?s <https://schema.org/name> ?name }",
        )
        assert "pg_ripple.sparql" in result
        assert "schema.org/name" in result

    def test_sparql_model_with_columns(self) -> None:
        result = PgRippleAdapter.sparql_model(
            None,  # type: ignore[arg-type]
            "SELECT ?s WHERE { ?s a <ex:Person> }",
            columns=["s TEXT"],
        )
        assert "AS t(s TEXT)" in result

    def test_sparql_model_with_graph(self) -> None:
        result = PgRippleAdapter.sparql_model(
            None,  # type: ignore[arg-type]
            "SELECT ?s WHERE { ?s a <ex:Person> }",
            graph="https://hr.example.org/",
        )
        assert "GRAPH <https://hr.example.org/>" in result

    def test_sparql_ref(self) -> None:
        result = PgRippleAdapter.sparql_ref(
            None,  # type: ignore[arg-type]
            "https://hr.example.org/employees",
        )
        assert "pg_ripple.sparql" in result
        assert "hr.example.org/employees" in result

    def test_date_function(self) -> None:
        assert PgRippleAdapter.date_function() == "now()"
