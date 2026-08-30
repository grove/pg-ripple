#!/usr/bin/env python3
"""Check router, access classes, OpenAPI, and HTTP documentation stay aligned."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path


METHODS = {"get", "post", "put", "delete", "patch", "head", "options"}
METHOD_RE = re.compile(r"\b(" + "|".join(sorted(METHODS)) + r")\s*\(")
ROUTE_START_RE = re.compile(r"\.route\(")
ROUTE_PATH_RE = re.compile(r'\.route\(\s*"([^"]+)"')
DOC_ROW_RE = re.compile(r'^\|\s*`?([^|]+?)`?\s*\|\s*`?([^|]+?)`?\s*\|\s*([^|]+?)\s*\|')
OPENAPI_PATH_RE = re.compile(r"^  (/[^:]+):\s*$")
OPENAPI_METHOD_RE = re.compile(r"^    (get|post|put|delete|patch|head|options):\s*$")


@dataclass(frozen=True, order=True)
class Route:
    path: str
    method: str
    access: str
    source_line: int = 0


def normalise_path(path: str) -> str:
    path = re.sub(r":([a-zA-Z0-9_]+)", "{}", path)
    path = re.sub(r"\{[^}]+\}", "{}", path)
    return path.rstrip("/") or "/"


def classify(method: str, path: str) -> str | None:
    if path in {"/health", "/ready", "/health/ready"}:
        return "PublicHealth"
    if path in {"/metrics", "/metrics/extension"}:
        return "Metrics"
    if path.startswith("/admin/") or path == "/explorer":
        return "Admin"
    if path.startswith("/datalog/stats/") or path.startswith("/datalog/views/"):
        return "Admin"
    if path in {"/datalog/lattices", "/datalog/views"}:
        return "Read" if method == "GET" else "Admin"
    if path.startswith("/datalog/rules") or path.startswith("/datalog/infer/"):
        return "Read" if method == "GET" else "Write"
    if path.startswith("/datalog/query/") or path in {"/datalog/constraints", "/datalog/constraints/"} or path.startswith("/datalog/constraints/"):
        return "Read"
    if path == "/sparql/bindings":
        return "Read"
    if path == "/sparql":
        return "Read" if method == "GET" else "Write"
    if path in {"/sparql/stream", "/rag", "/explain", "/hypothetical", "/void", "/service", "/openapi.yaml", "/flight/do_get"} or path.startswith("/subscribe/") or path.startswith("/rules/") or path in {"/rules/draft", "/rules/validate"}:
        return "Read"
    if path == "/rule-libraries" or path.startswith("/rule-libraries/"):
        return "Write" if path.endswith("/subscribe") else "Read"
    if path.startswith("/confidence/"):
        return "Read" if method == "GET" and not path.endswith("/load") else "Write"
    if path.startswith("/pagerank/") or path.startswith("/centrality/") or path.startswith("/temporal/") or path.startswith("/pprl/") or path.startswith("/dp/") or path.startswith("/entity-resolution/"):
        return "Read" if method == "GET" else "Write"
    if path.startswith("/proof-tree/"):
        return "Read"
    if path == "/tenants" or path.startswith("/tenants/"):
        return "Read" if method == "GET" else "Write"
    if path.startswith("/federation/"):
        return "Write"
    if path.startswith("/json-mapping/"):
        return "Read" if method == "GET" else "Write"
    if path.startswith("/rule-conflicts/"):
        return "Read"
    return None


def extract_routes(router_path: Path) -> list[Route]:
    lines = router_path.read_text(encoding="utf-8").splitlines()
    routes: list[Route] = []
    for index, line in enumerate(lines):
        if not ROUTE_START_RE.search(line):
            continue
        window_lines: list[str] = []
        depth = 0
        for current, candidate in enumerate(lines[index:], index):
            window_lines.append(candidate)
            depth += candidate.count("(") - candidate.count(")")
            if depth <= 0:
                break
        window = " ".join(window_lines)
        match = ROUTE_PATH_RE.search(window)
        if not match:
            continue
        methods = sorted({method.upper() for method in METHOD_RE.findall(window)})
        path = normalise_path(match.group(1))
        for method in methods or ["GET"]:
            routes.append(Route(path, method, classify(method, path) or "UNCLASSIFIED", index + 1))
    return sorted(set(routes))


def expand_methods(value: str) -> list[str]:
    return [part.strip().upper() for part in value.replace("`", "").split("/")]


def expand_auth(value: str, count: int) -> list[str]:
    values = [part.strip().replace("`", "") for part in value.split("/")]
    return values if len(values) == count else values[:1] * count


def load_docs(path: Path) -> dict[tuple[str, str], str]:
    result: dict[tuple[str, str], str] = {}
    lines = path.read_text(encoding="utf-8").splitlines() if path.exists() else []
    in_endpoint_table = False
    for line in lines:
        if line.startswith("| Method | Path | Auth |"):
            in_endpoint_table = True
            continue
        if in_endpoint_table and not line.startswith("|"):
            break
        if not in_endpoint_table:
            continue
        match = DOC_ROW_RE.match(line)
        if match and match.group(1).strip() != "---":
            methods = expand_methods(match.group(1))
            auth = expand_auth(match.group(3), len(methods))
            route_path = normalise_path(match.group(2).strip())
            for method, access in zip(methods, auth):
                result[(route_path, method)] = access
    return result


def load_openapi(path: Path) -> set[tuple[str, str]]:
    result: set[tuple[str, str]] = set()
    current: str | None = None
    lines = path.read_text(encoding="utf-8").splitlines() if path.exists() else []
    for line in lines:
        path_match = OPENAPI_PATH_RE.match(line)
        if path_match:
            current = normalise_path(path_match.group(1))
            continue
        method_match = OPENAPI_METHOD_RE.match(line)
        if current and method_match:
            result.add((current, method_match.group(1).upper()))
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=None, metavar="DIR")
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--inventory-out", type=Path)
    args = parser.parse_args()
    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parent.parent
    router_path = root / "pg_ripple_http" / "src" / "routing" / "mod.rs"
    docs_path = root / "docs" / "src" / "reference" / "http-api.md"
    openapi_path = root / "pg_ripple_http" / "openapi.yaml"
    routes = extract_routes(router_path)
    router_set = {(route.path, route.method) for route in routes}
    docs_set = load_docs(docs_path)
    openapi_set = load_openapi(openapi_path)
    errors: list[str] = []
    errors.extend(f"undocumented route: {method} {path}" for path, method in sorted(router_set - docs_set.keys()))
    errors.extend(f"documented nonexistent route: {method} {path}" for path, method in sorted(docs_set.keys() - router_set))
    errors.extend(f"OpenAPI missing route: {method} {path}" for path, method in sorted(router_set - openapi_set))
    errors.extend(f"OpenAPI documents nonexistent route: {method} {path}" for path, method in sorted(openapi_set - router_set))
    errors.extend(f"missing access classification: {route.method} {route.path}" for route in routes if route.access == "UNCLASSIFIED")
    access_aliases = {"PublicHealth": {"PublicHealth", "None"}, "Metrics": {"Metrics", "None or metrics token"}}
    for route in routes:
        documented = docs_set.get((route.path, route.method))
        if documented is not None and documented not in access_aliases.get(route.access, {route.access}):
            errors.append(f"access mismatch: {route.method} {route.path} is {route.access}, docs say {documented}")
    inventory = {
        "source": "pg_ripple_http/src/routing/mod.rs",
        "routes": [{"method": route.method, "path": route.path, "access": route.access, "source_line": route.source_line} for route in routes],
    }
    if args.inventory_out:
        output = args.inventory_out if args.inventory_out.is_absolute() else root / args.inventory_out
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")
    if errors and args.strict:
        print("HTTP route truth check failed:", file=sys.stderr)
        print("\n".join(f"  {error}" for error in errors), file=sys.stderr)
        return 1
    if errors:
        print("HTTP route truth warnings:")
        print("\n".join(f"  {error}" for error in errors))
    else:
        print(f"OK: checked {len(routes)} route/method registrations")
    return 0


if __name__ == "__main__":
    sys.exit(main())
