#!/usr/bin/env python3
"""Compare normalized schema-fingerprint JSON documents."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def normalize(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: normalize(value[key]) for key in sorted(value)}
    if isinstance(value, list):
        normalized = [normalize(item) for item in value]
        return sorted(normalized, key=lambda item: json.dumps(item, sort_keys=True))
    return value


def load(path: Path) -> Any:
    with path.open(encoding="utf-8") as stream:
        return normalize(json.load(stream))


def diff(left: Any, right: Any, path: str = "") -> list[str]:
    if type(left) is not type(right):
        return [path or "$"]
    if isinstance(left, dict):
        differences: list[str] = []
        for key in sorted(set(left) | set(right)):
            child = f"{path}.{key}" if path else key
            if key not in left or key not in right:
                differences.append(child)
            else:
                differences.extend(diff(left[key], right[key], child))
        return differences
    if isinstance(left, list):
        if left == right:
            return []
        return [path or "$"]
    return [] if left == right else [path or "$"]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("expected", type=Path)
    parser.add_argument("actual", type=Path)
    parser.add_argument(
        "--allow",
        action="append",
        default=[],
        metavar="PATH",
        help="allow a changed JSON path (repeatable)",
    )
    args = parser.parse_args()

    differences = diff(load(args.expected), load(args.actual))
    allowed = tuple(args.allow)
    unexpected = [
        path
        for path in differences
        if not any(path == prefix or path.startswith(f"{prefix}.") for prefix in allowed)
    ]
    if unexpected:
        print("schema fingerprints differ:", file=sys.stderr)
        for path in unexpected:
            print(f"  {path}", file=sys.stderr)
        return 1
    print("schema fingerprints match")
    if differences:
        print(f"allowed differences: {len(differences)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
