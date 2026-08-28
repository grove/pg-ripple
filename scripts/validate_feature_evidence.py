#!/usr/bin/env python3
"""Validate feature evidence paths and stable-feature release claims."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


STATUSES = {"implemented", "experimental", "planner_hint", "manual_refresh", "stub", "degraded", "broken", "planned"}
TEST_KINDS = {"positive", "negative", "restart", "migration", "security"}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("path", type=Path)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    document = json.loads(args.path.read_text(encoding="utf-8"))
    errors: list[str] = []
    if document.get("version") != args.version:
        errors.append("manifest version does not match --version")
    features = document.get("features")
    if not isinstance(features, list) or not features:
        errors.append("features must be a non-empty array")
        features = []
    for row in features:
        name = row.get("feature_name", "<unnamed>")
        if row.get("status") not in STATUSES:
            errors.append(f"{name}: invalid status")
        for field in ("stable_api_entry_points", "required_dependencies", "documentation_path", "last_verified_version", "known_limitations", "evidence_artifact_digest"):
            if field not in row:
                errors.append(f"{name}: missing {field}")
        for kind in TEST_KINDS:
            paths = row.get("tests", {}).get(kind)
            if not isinstance(paths, list) or not paths:
                errors.append(f"{name}: missing {kind} test evidence")
            for path in paths or []:
                if not (root / path).exists():
                    errors.append(f"{name}: missing evidence path {path}")
        docs = row.get("documentation_path")
        if docs and not (root / docs).exists():
            errors.append(f"{name}: missing documentation path {docs}")
        if row.get("status") == "implemented" and not row.get("stable_api_entry_points"):
            errors.append(f"{name}: stable feature has no API entry point")
        if row.get("required_dependencies") and not row.get("tests", {}).get("negative"):
            errors.append(f"{name}: dependency-degraded behavior is not tested")
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", row.get("evidence_artifact_digest", "")):
            errors.append(f"{name}: invalid evidence artifact digest")
    if errors:
        print("feature evidence validation failed:", file=sys.stderr)
        print("\n".join(f"  {error}" for error in errors), file=sys.stderr)
        return 1
    print(f"OK: validated {len(features)} feature evidence rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
