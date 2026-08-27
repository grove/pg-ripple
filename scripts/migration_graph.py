#!/usr/bin/env python3
"""Validate pg_ripple migrations and emit an ordered migration graph."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

VERSION = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
BASE = re.compile(r"^pg_ripple--(\d+\.\d+\.\d+)\.sql$")
EDGE = re.compile(r"^pg_ripple--(\d+\.\d+\.\d+)--(\d+\.\d+\.\d+)\.sql$")


def version(value: str) -> tuple[int, int, int]:
    match = VERSION.fullmatch(value)
    if not match:
        raise ValueError(f"invalid semver: {value}")
    return tuple(map(int, match.groups()))


def control_version(control: Path) -> str:
    for line in control.read_text(encoding="utf-8").splitlines():
        if line.startswith("default_version"):
            value = line.split("=", 1)[1].strip().strip("'")
            version(value)
            return value
    raise ValueError(f"{control} has no default_version")


def build(sql_dir: Path, target: str) -> dict:
    bases: dict[str, str] = {}
    edges: list[dict[str, str]] = []
    seen_pairs: set[tuple[str, str]] = set()
    for path in sorted(sql_dir.glob("pg_ripple--*.sql")):
        base = BASE.fullmatch(path.name)
        edge = EDGE.fullmatch(path.name)
        if not base and not edge:
            raise ValueError(f"unparseable migration filename: {path.name}")
        if base:
            current = base.group(1)
            version(current)
            if current in bases:
                raise ValueError(f"duplicate base migration for {current}")
            bases[current] = path.name
            continue
        source, destination = edge.groups()
        version(source)
        version(destination)
        if (source, destination) in seen_pairs:
            raise ValueError(f"duplicate migration edge: {source} -> {destination}")
        seen_pairs.add((source, destination))
        edges.append({"from": source, "to": destination, "file": path.name})

    if len(bases) != 1:
        raise ValueError(f"expected exactly one base migration, found {len(bases)}")
    start = next(iter(bases))
    outgoing: dict[str, list[dict[str, str]]] = {}
    incoming: dict[str, list[dict[str, str]]] = {}
    for edge in edges:
        outgoing.setdefault(edge["from"], []).append(edge)
        incoming.setdefault(edge["to"], []).append(edge)

    def visit(node: str, active: set[str], done: set[str]) -> None:
        if node in active:
            raise ValueError(f"migration graph cycle at {node}")
        if node in done:
            return
        active.add(node)
        for edge in outgoing.get(node, []):
            visit(edge["to"], active, done)
        active.remove(node)
        done.add(node)

    done: set[str] = set()
    for node in {e["from"] for e in edges} | {e["to"] for e in edges}:
        visit(node, set(), done)
    for edge in edges:
        if version(edge["to"]) <= version(edge["from"]):
            raise ValueError(f"migration is not forward: {edge['file']}")

    ambiguous = [node for node, values in outgoing.items() if len(values) > 1]
    ambiguous += [node for node, values in incoming.items() if len(values) > 1]
    if ambiguous:
        names = ", ".join(sorted(set(ambiguous), key=version))
        raise ValueError(f"ambiguous migration graph at: {names}")

    def path_from(begin: str) -> list[dict[str, str]]:
        ordered: list[dict[str, str]] = []
        visited: set[str] = set()
        current = begin
        while current != target:
            if current in visited:
                raise ValueError(f"migration graph cycle at {current}")
            visited.add(current)
            choices = outgoing.get(current, [])
            if not choices:
                raise ValueError(f"migration gap: no migration from {current} to {target}")
            edge = choices[0]
            ordered.append(edge)
            current = edge["to"]
        return ordered

    ordered = path_from(start)
    visited = {start}
    visited.update(edge["to"] for edge in ordered)

    nodes = {start, target} | {e["from"] for e in edges} | {e["to"] for e in edges}
    unreachable = sorted(nodes - visited, key=version)
    if unreachable:
        raise ValueError(f"unreachable migration versions: {', '.join(unreachable)}")
    paths = {
        begin: path_from(begin)
        for begin in sorted(visited, key=version)
    }
    return {
        "base_version": start,
        "target_version": target,
        "base": bases[start],
        "migrations": ordered,
        "migration_count": len(ordered),
        "paths": paths,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sql-dir", type=Path, default=Path("sql"))
    parser.add_argument("--control", type=Path, default=Path("pg_ripple.control"))
    parser.add_argument("--output", type=Path, default=Path("target/migration-graph.json"))
    args = parser.parse_args()
    try:
        target = control_version(args.control)
        graph = build(args.sql_dir, target)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(graph, indent=2) + "\n", encoding="utf-8")
    except (OSError, ValueError) as error:
        print(f"migration graph: {error}", file=sys.stderr)
        return 1
    print(f"migration graph: {graph['base_version']} -> {target} ({graph['migration_count']} migrations)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
