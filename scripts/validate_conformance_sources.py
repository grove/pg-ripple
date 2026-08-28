#!/usr/bin/env python3
"""Validate the immutable conformance source lock and fetched corpus shape."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path


REQUIRED_KEYS = {
    "kind",
    "policy",
    "repository",
    "commit",
    "archive_url",
    "archive_sha256",
    "expected_manifest_count",
    "expected_test_count",
    "license",
}


def fail(message: str) -> None:
    raise SystemExit(f"ERROR: {message}")


def load_lock(path: Path) -> dict:
    try:
        lock = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read source lock {path}: {exc}")
    if lock.get("format") != 1 or not isinstance(lock.get("suites"), dict):
        fail("source lock must have format 1 and a suites object")
    for suite, source in lock["suites"].items():
        missing = REQUIRED_KEYS - source.keys()
        if missing:
            fail(f"{suite}: missing lock fields: {', '.join(sorted(missing))}")
        if source["policy"] not in {"required", "informational"}:
            fail(f"{suite}: policy must be required or informational")
        commit = source["commit"]
        is_w3c_archive = source["repository"].startswith("https://www.w3.org/")
        if not re.fullmatch(r"[0-9a-f]{40}", commit) and not (
            is_w3c_archive and re.fullmatch(r"w3c-archive-\d{4}-\d{2}-\d{2}", commit)
        ):
            fail(f"{suite}: commit must be a 40-character SHA or a dated W3C archive revision")
        archive_sha = source["archive_sha256"]
        if not source["archive_url"].startswith("local:") and not re.fullmatch(r"[0-9a-f]{64}", archive_sha):
            fail(f"{suite}: remote source must have a concrete 64-character archive SHA-256")
        for field in ("expected_manifest_count", "expected_test_count"):
            bounds = source[field]
            if not isinstance(bounds, dict) or not isinstance(bounds.get("min"), int) or not isinstance(bounds.get("max"), int):
                fail(f"{suite}: {field} must contain integer min and max")
            if bounds["min"] < 0 or bounds["min"] > bounds["max"]:
                fail(f"{suite}: invalid {field} bounds")
        if not source["license"]:
            fail(f"{suite}: license is required")
    return lock


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def check_bounds(suite: str, label: str, value: int, bounds: dict) -> None:
    if not bounds["min"] <= value <= bounds["max"]:
        fail(
            f"{suite}: unexpected {label} count {value}; "
            f"expected {bounds['min']}..{bounds['max']}"
        )


def validate_shape(suite: str, source: dict, directory: Path) -> None:
    if not directory.is_dir():
        fail(f"{suite}: corpus directory does not exist: {directory}")

    manifests = (
        list(directory.rglob("manifest.ttl"))
        + list(directory.rglob("manifest.rdf"))
        + list(directory.rglob("manifest.json"))
    )
    check_bounds(suite, "manifest", len(manifests), source["expected_manifest_count"])

    suffixes = {".rq", ".ru", ".sparql", ".arq"}
    tests = [path for path in directory.rglob("*") if path.is_file() and path.suffix in suffixes]
    descriptor_count = 0
    for manifest in directory.rglob("manifest.json"):
        try:
            document = json.loads(manifest.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            fail(f"{suite}: cannot read {manifest}: {exc}")
        if isinstance(document.get("tests"), list):
            descriptor_count += len(document["tests"])
    check_bounds(suite, "test", max(len(tests), descriptor_count), source["expected_test_count"])


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lockfile", default="tests/conformance/sources.lock")
    parser.add_argument("--suite", help="validate one suite instead of lock metadata only")
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--directory", type=Path)
    args = parser.parse_args()

    lock = load_lock(Path(args.lockfile))
    if not args.suite:
        print(f"OK: validated metadata for {len(lock['suites'])} conformance sources")
        return 0
    source = lock["suites"].get(args.suite)
    if source is None:
        fail(f"suite is not present in source lock: {args.suite}")
    if args.archive:
        actual = sha256(args.archive)
        expected = source["archive_sha256"]
        if expected.startswith("sha256:"):
            expected = expected[7:]
        if len(expected) == 64 and actual != expected:
            fail(f"{args.suite}: archive SHA-256 mismatch: expected {expected}, got {actual}")
        if len(expected) != 64 and not source["archive_url"].startswith("local:"):
            fail(f"{args.suite}: remote source has no concrete archive SHA-256")
        print(f"OK: {args.suite} archive SHA-256 {actual}")
    if args.directory:
        validate_shape(args.suite, source, args.directory)
        print(f"OK: {args.suite} corpus shape is within the locked bounds")
    if not args.archive and not args.directory:
        fail("--suite requires --archive, --directory, or both")
    return 0


if __name__ == "__main__":
    sys.exit(main())
