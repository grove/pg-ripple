#!/usr/bin/env python3
"""
graphrag_export.py — pg_ripple GraphRAG BYOG export CLI (v0.26.0)

Exports a named graph from pg_ripple as Parquet files suitable for use with
Microsoft GraphRAG's Bring Your Own Graph (BYOG) feature.

Prerequisites:
    pip install psycopg pyarrow

Usage:
    python graphrag_export.py \\
        --pg-url "postgresql://user:pass@localhost/mydb" \\
        --graph-iri "https://example.org/my-graph" \\
        --output-dir ./graphrag_output \\
        --enrich-with-datalog \\
        --validate

The output directory will contain:
    entities.parquet
    relationships.parquet
    text_units.parquet

Pass these to GraphRAG:
    graphrag index --config settings.yaml
    # settings.yaml: entity_table_path: ./graphrag_output/entities.parquet
"""

import argparse
import sys
import os
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Export a pg_ripple named graph to Parquet files for GraphRAG BYOG.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "--pg-url",
        required=True,
        help="PostgreSQL connection string (e.g. 'postgresql://user:pass@host/db').",
    )
    parser.add_argument(
        "--graph-iri",
        required=True,
        help="Named graph IRI to export (e.g. 'https://example.org/my-graph').",
    )
    parser.add_argument(
        "--output-dir",
        default="./graphrag_output",
        help="Directory for Parquet output files (default: ./graphrag_output).",
    )
    parser.add_argument(
        "--enrich-with-datalog",
        action="store_true",
        default=False,
        help=(
            "Run pg_ripple.infer('owl-rl') and pg_ripple.infer('graphrag_enrichment') "
            "before exporting to derive implicit relationships."
        ),
    )
    parser.add_argument(
        "--validate",
        action="store_true",
        default=False,
        help=(
            "Run pg_ripple.validate() before exporting. "
            "Prints violations and exits with code 1 if any violations are present."
        ),
    )
    parser.add_argument(
        "--format",
        choices=["parquet", "csv"],
        default="parquet",
        help="Output format: 'parquet' (default) or 'csv' (for debugging).",
    )
    return parser.parse_args()


def connect(pg_url: str):
    """Return an open psycopg connection."""
    try:
        import psycopg  # type: ignore
    except ImportError:
        print(
            "ERROR: psycopg (v3) is required. Install with: pip install psycopg",
            file=sys.stderr,
        )
        sys.exit(1)
    try:
        conn = psycopg.connect(pg_url, autocommit=True)
    except Exception as exc:
        print(f"ERROR: could not connect to PostgreSQL: {exc}", file=sys.stderr)
        sys.exit(1)
    return conn


def run_validation(conn) -> bool:
    """
    Run pg_ripple.validate() and print any violations.
    Returns True if the graph conforms (no violations), False otherwise.
    """
    with conn.cursor() as cur:
        cur.execute("SELECT pg_ripple.validate()")
        result = cur.fetchone()[0]  # JsonB dict

    conforms = result.get("conforms", True)
    violations = result.get("results", [])

    if not conforms:
        print(f"SHACL VIOLATIONS ({len(violations)} found):")
        for v in violations:
            focus = v.get("focusNode", "?")
            path = v.get("resultPath", "?")
            msg = v.get("resultMessage", "validation error")
            print(f"  [{focus}] {path}: {msg}")
    else:
        print("SHACL validation passed: graph conforms.")

    return bool(conforms)


def run_datalog_enrichment(conn) -> None:
    """Run OWL-RL and GraphRAG enrichment inference."""
    print("Running OWL-RL inference...")
    with conn.cursor() as cur:
        cur.execute("SELECT pg_ripple.load_rules_builtin('owl-rl')")
        cur.execute("SELECT pg_ripple.infer('owl-rl')")
        rows = cur.fetchone()[0]
    print(f"  OWL-RL: {rows} triples derived.")

    print("Running graphrag_enrichment inference...")
    with conn.cursor() as cur:
        cur.execute(
            "SELECT pg_ripple.infer('graphrag_enrichment')"
        )
        rows = cur.fetchone()[0]
    print(f"  graphrag_enrichment: {rows} triples derived.")


def export_table(
    conn,
    fn_name: str,
    graph_iri: str,
    output_path: str,
) -> int:
    """
    Call a pg_ripple.export_graphrag_*() function and return the row count.
    The function writes the Parquet file directly from Rust.
    """
    with conn.cursor() as cur:
        cur.execute(
            f"SELECT pg_ripple.{fn_name}(%s, %s)",
            (graph_iri, output_path),
        )
        return cur.fetchone()[0]


def export_to_csv(conn, graph_iri: str, output_dir: Path) -> dict:
    """
    Fallback: export as CSV using psycopg + SPARQL queries.
    Returns a dict of {filename: row_count}.
    """
    import csv

    counts = {}

    entity_query = (
        "SELECT r.result->>'entity' AS id, "
        "r.result->>'title' AS title, "
        "r.result->>'type' AS type, "
        "r.result->>'description' AS description, "
        "r.result->>'frequency' AS frequency, "
        "r.result->>'degree' AS degree "
        "FROM pg_ripple.sparql("
        f"'SELECT ?entity ?title ?type ?description ?frequency ?degree "
        f"WHERE {{ GRAPH <{graph_iri}> {{"
        f" ?entity <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://graphrag.org/ns/Entity> ."
        f" OPTIONAL {{ ?entity <https://graphrag.org/ns/title> ?title }}"
        f" OPTIONAL {{ ?entity <https://graphrag.org/ns/type> ?type }}"
        f" OPTIONAL {{ ?entity <https://graphrag.org/ns/description> ?description }}"
        f" OPTIONAL {{ ?entity <https://graphrag.org/ns/frequency> ?frequency }}"
        f" OPTIONAL {{ ?entity <https://graphrag.org/ns/degree> ?degree }}"
        f"}}}}') r(result jsonb)"
    )

    with conn.cursor() as cur:
        cur.execute(entity_query)
        rows = cur.fetchall()

    entity_path = output_dir / "entities.csv"
    with open(entity_path, "w", newline="", encoding="utf-8") as f:
        writer = csv.writer(f)
        writer.writerow(["id", "title", "type", "description", "frequency", "degree"])
        writer.writerows(rows)
    counts["entities.csv"] = len(rows)

    return counts


def main() -> None:
    args = parse_args()

    output_dir = Path(args.output_dir).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    print(f"Connecting to PostgreSQL...")
    conn = connect(args.pg_url)

    # Validate first if requested.
    if args.validate:
        conforms = run_validation(conn)
        if not conforms:
            print(
                "Aborting export: SHACL violations found. Fix them and re-run.",
                file=sys.stderr,
            )
            conn.close()
            sys.exit(1)

    # Run Datalog enrichment if requested.
    if args.enrich_with_datalog:
        run_datalog_enrichment(conn)

    graph_iri = args.graph_iri
    # Strip angle brackets if the user provided them.
    if graph_iri.startswith("<") and graph_iri.endswith(">"):
        graph_iri = graph_iri[1:-1]

    if args.format == "csv":
        print("Exporting as CSV (debug mode)...")
        counts = export_to_csv(conn, graph_iri, output_dir)
        for fname, n in counts.items():
            print(f"  {fname}: {n} rows -> {output_dir / fname}")
    else:
        # Parquet export via Rust functions.
        tables = [
            ("export_graphrag_entities", "entities.parquet"),
            ("export_graphrag_relationships", "relationships.parquet"),
            ("export_graphrag_text_units", "text_units.parquet"),
        ]
        for fn_name, filename in tables:
            dest = str(output_dir / filename)
            try:
                n = export_table(conn, fn_name, graph_iri, dest)
                print(f"  {filename}: {n} rows -> {dest}")
            except Exception as exc:
                print(f"  ERROR exporting {filename}: {exc}", file=sys.stderr)
                conn.close()
                sys.exit(1)

    conn.close()
    print("Export complete.")


if __name__ == "__main__":
    main()
