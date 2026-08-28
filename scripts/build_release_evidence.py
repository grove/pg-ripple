#!/usr/bin/env python3
"""Build the immutable, versioned release-evidence bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


def run(*command: str) -> None:
    subprocess.run(command, cwd=ROOT, check=True)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def copy_tree(source: Path, destination: Path) -> None:
    for path in source.rglob("*"):
        if path.is_file():
            target = destination / path.relative_to(source)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, target)


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def package_version() -> str:
    for line in (ROOT / "Cargo.toml").read_text(encoding="utf-8").splitlines():
        if line.startswith("version = "):
            return line.split('"')[1]
    raise SystemExit("ERROR: Cargo.toml has no package version")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
    parser.add_argument("--output", type=Path, default=Path("target/release-evidence"))
    args = parser.parse_args()
    version = package_version()
    if version != args.version:
        raise SystemExit(f"ERROR: requested {args.version}, Cargo.toml is {version}")
    control = (ROOT / "pg_ripple.control").read_text(encoding="utf-8")
    if f"default_version = '{version}'" not in control:
        raise SystemExit("ERROR: pg_ripple.control version does not match Cargo.toml")

    bundle = (args.output if args.output.is_absolute() else ROOT / args.output) / version
    for name in ("schema-fingerprints", "migrations", "conformance", "security"):
        (bundle / name).mkdir(parents=True, exist_ok=True)

    feature = ROOT / "results/features" / version / "feature-evidence.json"
    if not feature.exists():
        run(sys.executable, "scripts/generate_feature_evidence.py", version)
    run(sys.executable, "scripts/validate_feature_evidence.py", str(feature), "--version", version)
    copy2 = bundle / "feature-evidence.json"
    shutil.copy2(feature, copy2)

    run(sys.executable, "scripts/check_http_routes.py", "--strict", "--inventory-out", str(bundle / "route-inventory.json"))
    conformance = ROOT / "results/conformance" / version
    run(sys.executable, "scripts/validate_conformance_reports.py", "--version", version, "--results-dir", str(conformance))
    copy_tree(conformance, bundle / "conformance")

    run(sys.executable, "scripts/migration_graph.py", "--output", str(bundle / "migrations/graph.json"))
    for path in sorted((ROOT / "sql").glob("pg_ripple--*.sql")):
        shutil.copy2(path, bundle / "migrations" / path.name)
    shutil.copy2(ROOT / "scripts/schema_fingerprint.sql", bundle / "schema-fingerprints/schema_fingerprint.sql")

    checks = {
        "migration_headers": ("bash", "scripts/check_migration_headers.sh"),
        "security_definer": ("bash", "scripts/check_no_security_definer.sh"),
        "github_actions_pinning": ("bash", "scripts/check_github_actions_pinned.sh"),
        "api_drift": (sys.executable, "scripts/check_api_drift.py", "--version", version),
        "roadmap_evidence": (sys.executable, "scripts/check_roadmap_evidence.py", "--version", version),
    }
    for name, command in checks.items():
        try:
            run(*command)
        except subprocess.CalledProcessError as error:
            raise SystemExit(f"ERROR: release evidence check failed: {name} ({error.returncode})") from error
    write_json(bundle / "security/checks.json", {"status": "passed", "checks": sorted(checks)})
    for source in (ROOT / "audit.toml", ROOT / "deny.toml"):
        if source.exists():
            shutil.copy2(source, bundle / "security" / source.name)

    benchmarks = sorted(path for path in (ROOT / "benchmarks").rglob("*") if path.is_file())
    write_json(bundle / "benchmark-summary.json", {
        "status": "source-inventory",
        "files": [str(path.relative_to(ROOT)) for path in benchmarks],
    })
    write_json(bundle / "test-counts.json", {
        "rust_integration_tests": len(list((ROOT / "tests").glob("*.rs"))),
        "pg_regress_sql": len(list((ROOT / "tests/pg_regress/sql").glob("*.sql"))),
        "migration_scripts": len(list((ROOT / "sql").glob("pg_ripple--*.sql"))),
    })
    sbom = ROOT / "sbom.json"
    if not sbom.exists():
        raise SystemExit("ERROR: sbom.json is missing")
    shutil.copy2(sbom, bundle / "sbom.json")

    git_sha = subprocess.check_output(("git", "rev-parse", "HEAD"), cwd=ROOT, text=True).strip()
    write_json(bundle / "build-provenance.json", {
        "version": version,
        "git_sha": git_sha,
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "rustc": subprocess.check_output(("rustc", "--version"), text=True).strip(),
        "postgres_version": subprocess.run(("pg_config", "--version"), text=True, capture_output=True, check=False).stdout.strip() or "unknown",
    })

    files = []
    for path in sorted(bundle.rglob("*")):
        if path.is_file() and path.name not in {"manifest.json", "checksums.txt"}:
            files.append({"path": str(path.relative_to(bundle)), "sha256": sha256(path), "bytes": path.stat().st_size})
    artifact_digest = hashlib.sha256("\n".join(item["sha256"] for item in files).encode()).hexdigest()
    write_json(bundle / "manifest.json", {
        "format": 1,
        "version": version,
        "artifact_digest": f"sha256:{artifact_digest}",
        "files": files,
    })
    checksum_lines = [f"{sha256(path)}  {path.relative_to(bundle)}" for path in sorted(bundle.rglob("*")) if path.is_file() and path.name != "checksums.txt"]
    (bundle / "checksums.txt").write_text("\n".join(checksum_lines) + "\n", encoding="utf-8")
    print(f"OK: release evidence bundle written to {bundle}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"ERROR: could not build release evidence: {error}", file=sys.stderr)
        raise SystemExit(1)
