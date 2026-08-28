#!/usr/bin/env python3
"""Validate versioned conformance reports and required-suite gate semantics."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


FIELDS = {
    "pg_ripple_version",
    "git_sha",
    "artifact_digest",
    "postgres_version",
    "suite",
    "suite_commit",
    "started_at",
    "duration_seconds",
    "expected_total",
    "executed_total",
    "passed",
    "failed",
    "skipped",
    "xfail",
    "xpass",
    "unexpected_failures",
}
COUNT_FIELDS = ("passed", "failed", "skipped", "timeout", "xfail", "xpass")


def fail(message: str) -> None:
    raise SystemExit(f"ERROR: {message}")


def bounds(value: object, label: str) -> tuple[int, int]:
    if not isinstance(value, dict) or not isinstance(value.get("min"), int) or not isinstance(value.get("max"), int):
        fail(f"{label} must contain integer min and max")
    minimum, maximum = value["min"], value["max"]
    if minimum < 0 or minimum > maximum:
        fail(f"{label} has invalid bounds")
    return minimum, maximum


def validate_report(path: Path, suite: str, version: str, source: dict) -> None:
    try:
        report = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"{suite}: cannot read report: {exc}")
    missing = FIELDS - report.keys()
    if missing:
        fail(f"{suite}: report is missing fields: {', '.join(sorted(missing))}")
    if report["pg_ripple_version"] != version:
        fail(f"{suite}: report version is {report['pg_ripple_version']!r}, expected {version!r}")
    if report["suite"] != suite:
        fail(f"{suite}: report suite is {report['suite']!r}")
    for field in ("git_sha", "artifact_digest", "postgres_version", "suite_commit", "started_at"):
        if not isinstance(report[field], str) or report[field] in {"", "unknown", "uncomputed"}:
            fail(f"{suite}: report metadata field {field} is missing")
    if not isinstance(report["unexpected_failures"], list):
        fail(f"{suite}: unexpected_failures must be an array")
    values = {field: report[field] for field in COUNT_FIELDS + ("executed_total", "expected_total")}
    if any(not isinstance(value, int) or value < 0 for value in values.values()):
        fail(f"{suite}: report counts must be non-negative integers")
    if report["executed_total"] != sum(report[field] for field in COUNT_FIELDS):
        fail(f"{suite}: executed_total does not equal outcome counts")
    if len(report["unexpected_failures"]) != report["failed"] + report["timeout"] + report["xpass"]:
        fail(f"{suite}: unexpected_failures does not match failed + timeout + xpass")
    minimum, maximum = bounds(source["expected_test_count"], f"{suite}.expected_test_count")
    if not minimum <= report["executed_total"] <= maximum:
        fail(f"{suite}: executed_total {report['executed_total']} is outside {minimum}..{maximum}")
    if source["policy"] == "required":
        if report["executed_total"] == 0:
            fail(f"{suite}: required suite executed zero tests")
        if report["skipped"] or report["failed"] or report["xpass"]:
            fail(f"{suite}: required suite contains skipped, failed, or XPASS tests")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--results-dir", type=Path)
    parser.add_argument("--lockfile", type=Path, default=Path("tests/conformance/sources.lock"))
    args = parser.parse_args()
    lock = json.loads(args.lockfile.read_text(encoding="utf-8"))
    directory = args.results_dir or Path("results/conformance") / args.version
    suites = lock["suites"]
    checked = 0
    for suite, source in suites.items():
        report_path = directory / f"{suite}.json"
        if not report_path.exists():
            if source["policy"] == "required":
                fail(f"{suite}: required report is missing: {report_path}")
            continue
        validate_report(report_path, suite, args.version, source)
        checked += 1
    print(f"OK: validated {checked} conformance reports for v{args.version}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, json.JSONDecodeError, KeyError) as exc:
        print(f"ERROR: invalid conformance report configuration: {exc}", file=sys.stderr)
        raise SystemExit(1)
