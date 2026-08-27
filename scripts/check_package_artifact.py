#!/usr/bin/env python3
"""Validate the files needed to install one platform package artifact."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
from migration_graph import build, control_version  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("package", type=Path)
    parser.add_argument("--platform", choices=("linux", "macos", "windows"), required=True)
    args = parser.parse_args()

    package = args.package.resolve()
    control = package / "extension" / "pg_ripple.control"
    if not control.is_file():
        raise SystemExit(f"package artifact: missing {control}")

    expected_version = control_version(ROOT / "pg_ripple.control")
    actual_version = re.search(r"^default_version\s*=\s*'([^']+)'", control.read_text(), re.M)
    if not actual_version or actual_version.group(1) != expected_version:
        raise SystemExit(
            f"package artifact: control version is "
            f"{actual_version.group(1) if actual_version else 'missing'}, "
            f"expected {expected_version}"
        )

    graph = build(ROOT / "sql", expected_version)
    sql_files = {path.name for path in (package / "extension").glob("pg_ripple--*.sql")}
    required_sql = {graph["base"]} | {edge["file"] for edge in graph["migrations"]}
    missing = sorted(required_sql - sql_files)
    if missing:
        raise SystemExit(f"package artifact: missing migration files: {', '.join(missing)}")

    suffixes = {
        "linux": (".so",),
        "macos": (".dylib", ".so"),
        "windows": (".dll",),
    }[args.platform]
    libraries = [
        path
        for path in (package / "lib").rglob("*")
        if path.is_file() and path.suffix.lower() in suffixes
    ]
    if not libraries:
        raise SystemExit(f"package artifact: no {args.platform} shared library found")

    print(
        f"package artifact passed: {args.platform}, {expected_version}, "
        f"{len(required_sql)} migration files, {len(libraries)} shared library file(s)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
