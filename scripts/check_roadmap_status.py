#!/usr/bin/env python3
"""
scripts/check_roadmap_status.py

v0.75.0 ROADMAP-VALIDATE-01 (MF-K): Validate that ROADMAP.md marks the
current extension version as Released after a release event.

When the version in Cargo.toml is tagged and released, the corresponding
ROADMAP.md table row must show '✅ Released' (or 'Released ✅').
This script exits non-zero if:
  1. The current version does not appear in ROADMAP.md.
  2. The ROADMAP.md row for the current version does NOT contain a
     Released/✅ marker (i.e., the status column still says 'Planned').
  3. The version appears but the status column is 'Planned' or empty.

Usage:
    python3 scripts/check_roadmap_status.py --version X.Y.Z
    python3 scripts/check_roadmap_status.py  # reads version from Cargo.toml

Exit code 0 = OK; non-zero = status mismatch.
"""

import argparse
import re
import sys
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Validate that ROADMAP.md marks the current version as Released. "
            "ROADMAP-VALIDATE-01 (v0.75.0 MF-K)."
        )
    )
    parser.add_argument(
        "--version",
        metavar="X.Y.Z",
        help="Version to check (default: read from Cargo.toml).",
    )
    parser.add_argument(
        "--roadmap",
        default="ROADMAP.md",
        metavar="FILE",
        help="Path to ROADMAP.md (default: ROADMAP.md).",
    )
    parser.add_argument(
        "--cargo-toml",
        default="Cargo.toml",
        metavar="FILE",
        help="Path to Cargo.toml for version extraction (default: Cargo.toml).",
    )
    return parser.parse_args()


def read_cargo_version(cargo_toml_path: Path) -> str | None:
    """Extract version = '...' from the [package] section of Cargo.toml."""
    text = cargo_toml_path.read_text(encoding="utf-8", errors="replace")
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    return m.group(1) if m else None


def check_roadmap_status(roadmap_path: Path, version: str) -> tuple[bool, str]:
    """
    Check that ROADMAP.md has a table row for `version` with a Released status.

    Returns (ok: bool, message: str).
    """
    text = roadmap_path.read_text(encoding="utf-8", errors="replace")

    # Locate any line containing the version string in a table row.
    # ROADMAP.md table rows look like:
    #   | [v0.75.0](roadmap/v0.75.0.md) | ... | ✅ Released | ...
    # or:
    #   | [v0.74.0](roadmap/v0.74.0.md) | ... | Released ✅ | ...
    version_pattern = re.escape(version)
    # Match a markdown table row containing the version.
    row_re = re.compile(
        rf"^\|[^\|]*{version_pattern}[^\|]*\|.*$",
        re.MULTILINE,
    )

    matches = row_re.findall(text)
    if not matches:
        return (
            False,
            f"Version {version} not found in any ROADMAP.md table row.\n"
            f"Add a row for v{version} to ROADMAP.md.",
        )

    # Check whether at least one matching row has a Released marker.
    released_re = re.compile(
        r"(Released\s*✅|✅\s*Released|Released\s*:white_check_mark:|Released)",
        re.IGNORECASE,
    )
    planned_re = re.compile(r"\bPlanned\b", re.IGNORECASE)

    for row in matches:
        if released_re.search(row):
            return (True, f"Version {version} is correctly marked as Released in ROADMAP.md.")
        if planned_re.search(row):
            return (
                False,
                f"Version {version} is still marked 'Planned' in ROADMAP.md.\n"
                f"Row: {row.strip()}\n"
                f"Update ROADMAP.md to mark v{version} as 'Released ✅' after tagging.",
            )

    # Row exists but status is unclear.
    return (
        False,
        f"Version {version} row exists in ROADMAP.md but has no clear Released/Planned status.\n"
        f"Row(s): {matches[0].strip()}\n"
        f"Ensure the Status column contains 'Released ✅' or '✅ Released'.",
    )


def main() -> int:
    args = parse_args()
    root = Path(__file__).parent.parent

    # Resolve version.
    version = args.version
    if not version:
        cargo_path = root / args.cargo_toml
        if not cargo_path.exists():
            print(f"ERROR: Cargo.toml not found: {cargo_path}", file=sys.stderr)
            return 1
        version = read_cargo_version(cargo_path)
        if not version:
            print(
                f"ERROR: could not extract version from {cargo_path}", file=sys.stderr
            )
            return 1
        print(f"check_roadmap_status: using version {version} from Cargo.toml")

    # Validate version string format.
    if not re.match(r"^\d+\.\d+\.\d+$", version):
        print(
            f"ERROR: --version must be X.Y.Z, got: {version}", file=sys.stderr
        )
        return 1

    roadmap_path = root / args.roadmap
    if not roadmap_path.exists():
        print(f"ERROR: ROADMAP.md not found: {roadmap_path}", file=sys.stderr)
        return 1

    ok, message = check_roadmap_status(roadmap_path, version)
    if ok:
        print(f"OK: {message}")
        return 0
    else:
        print(f"FAIL: {message}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
