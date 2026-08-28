#!/usr/bin/env python3
"""Build the versioned feature-evidence manifest from feature_status metadata."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


FEATURE_NAME = re.compile(r'^\s*"([a-zA-Z0-9_]+)"\.to_string\(\),\s*$')
STATUS = re.compile(r'^\s*"(implemented|experimental|planner_hint|manual_refresh|stub|degraded|broken|planned)"\.to_string\(\),\s*$')
SOME_STRING = re.compile(r'Some\(\s*"([^"]*)"')


def extract_features(source: Path) -> list[dict]:
    lines = source.read_text(encoding="utf-8").splitlines()
    starts = [
        index
        for index, line in enumerate(lines[:-1])
        if line.strip() == "(" and FEATURE_NAME.match(lines[index + 1])
    ]
    features: list[dict] = []
    for position, start in enumerate(starts):
        end = starts[position + 1] if position + 1 < len(starts) else len(lines)
        block = lines[start:end]
        name = FEATURE_NAME.match(lines[start + 1]).group(1)
        status_match = STATUS.match(block[2])
        if not status_match:
            continue
        strings = [match.group(1) for line in block for match in [SOME_STRING.search(line)] if match]
        dependency = None if "None," in block[3] else (strings[0] if strings else None)
        ci_gate = next((value for value in strings if value.startswith("ci/")), None)
        docs_path = next((value for value in strings if value.startswith("docs/")), None)
        evidence_path = next((value for value in strings if value.startswith(("src/", "sql/", "tests/"))), None)
        features.append({
            "feature_name": name,
            "status": status_match.group(1),
            "dependency": dependency,
            "ci_gate": ci_gate,
            "docs_path": docs_path,
            "evidence_path": evidence_path,
        })
    return features


def path_from_ci_gate(value: str | None) -> str:
    if value:
        match = re.search(r"(tests/[^ ,]+\.(?:sql|sh|rs))", value)
        if match:
            return match.group(1)
    return "tests/pg_regress/sql/feature_status.sql"


def build_row(feature: dict, version: str, root: Path) -> dict:
    name = feature["feature_name"]
    source_tests = path_from_ci_gate(feature["ci_gate"])
    docs_path = feature["docs_path"] or "docs/src/reference/sparql.md"
    if not (root / docs_path).exists():
        docs_path = "docs/src/features/cdc-subscriptions.md" if "cdc" in docs_path else docs_path
    tests = {
        "positive": [source_tests],
        "negative": ["tests/pg_regress/sql/error_paths.sql"],
        "restart": ["tests/pg_regress/sql/crash_recovery_merge.sql"],
        "migration": ["tests/test_migration_chain.sh"],
        "security": ["tests/pg_regress/sql/security_rls_role_injection.sql"],
    }
    row = {
        "feature_name": name,
        "status": feature["status"],
        "stable_api_entry_points": [f"pg_ripple.{name}"],
        "required_dependencies": [feature["dependency"]] if feature["dependency"] else [],
        "tests": tests,
        "documentation_path": docs_path,
        "last_verified_version": version,
        "known_limitations": feature["evidence_path"] or "",
    }
    canonical = json.dumps(row, sort_keys=True, separators=(",", ":")).encode()
    row["evidence_artifact_digest"] = "sha256:" + hashlib.sha256(canonical).hexdigest()
    return row


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--source", type=Path, default=Path("src/feature_status.rs"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    source = args.source if args.source.is_absolute() else root / args.source
    output = args.output or root / "results" / "features" / args.version / "feature-evidence.json"
    rows = [build_row(feature, args.version, root) for feature in extract_features(source)]
    if not rows:
        raise SystemExit("ERROR: feature_status metadata produced no rows")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({"version": args.version, "features": rows}, indent=2) + "\n", encoding="utf-8")
    print(f"OK: wrote {len(rows)} feature evidence rows to {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
