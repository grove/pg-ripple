#!/usr/bin/env python3
"""
scripts/check_roadmap_evidence.py

v0.67.0 GATE-01: Portable Python replacement for check_roadmap_evidence.sh.

Verifies that CHANGELOG.md contains a section for the given version, and that
completion claims (lines with "implemented", "delivered", "added", "complete",
"released") have at least one evidence marker in the same bullet point or adjacent
context (test file, docs path, SQL function, pg_regress name).

Usage:
    python3 scripts/check_roadmap_evidence.py --version X.Y.Z

Exit code 0 = OK; non-zero = evidence gaps or extraction failure.
"""

import argparse
import re
import sys
from pathlib import Path


# Lines that use strong completion language and therefore need evidence.
COMPLETION_WORDS_RE = re.compile(
    r"\b(implemented|delivered|added|completed?|released?)\b",
    re.IGNORECASE,
)

# Evidence markers — any of these in a bullet line counts as evidence.
EVIDENCE_MARKERS_RE = re.compile(
    r"(ci/regress:|ci/test:|docs/src/|pg_ripple\.|feature_status|"
    r"roadmap/|plans/|\.sql|\.md|#\[pg_test\]|pg_regress|SPARQL|GUC)",
    re.IGNORECASE,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Check CHANGELOG.md for evidence markers on completion claims."
    )
    parser.add_argument(
        "--version",
        required=True,
        metavar="X.Y.Z",
        help="Release version to check (required).",
    )
    parser.add_argument(
        "--changelog",
        default="CHANGELOG.md",
        metavar="FILE",
        help="Path to CHANGELOG.md (default: CHANGELOG.md).",
    )
    return parser.parse_args()


def extract_version_section(changelog_text: str, version: str) -> str | None:
    """Extract the section for the given version from CHANGELOG.md."""
    # Match a header like: ## [0.67.0] or ## v0.67.0 or ## 0.67.0
    version_escaped = re.escape(version)
    section_re = re.compile(
        rf"^##\s+(\[?v?{version_escaped}\]?)\b.*$",
        re.MULTILINE,
    )
    m = section_re.search(changelog_text)
    if not m:
        return None

    start = m.start()
    # Find the next ## header (or end of file).
    next_section_re = re.compile(r"^##\s+", re.MULTILINE)
    next_m = next_section_re.search(changelog_text, start + 1)
    end = next_m.start() if next_m else len(changelog_text)

    return changelog_text[start:end]


def main() -> int:
    args = parse_args()
    root = Path(__file__).parent.parent

    # Validate version argument — prevents stale / check-nothing invocations.
    version_re = re.compile(r"^\d+\.\d+\.\d+$")
    if not version_re.match(args.version):
        print(
            f"ERROR: --version must be X.Y.Z, got: {args.version}",
            file=sys.stderr,
        )
        return 1

    changelog_path = root / args.changelog
    if not changelog_path.exists():
        print(f"ERROR: changelog not found: {changelog_path}", file=sys.stderr)
        return 1

    changelog_text = changelog_path.read_text(encoding="utf-8", errors="replace")

    section = extract_version_section(changelog_text, args.version)
    if section is None:
        print(
            f"ERROR: version {args.version} section not found in {changelog_path}.\n"
            f"Add a '## [{args.version}]' or '## v{args.version}' header.",
            file=sys.stderr,
        )
        return 1

    print(
        f"check_roadmap_evidence v{args.version}: "
        f"found section ({len(section.splitlines())} lines)"
    )

    # Check bullet lines with completion language.
    failures: list[str] = []
    for line in section.splitlines():
        stripped = line.strip()
        if not stripped.startswith(("-", "*", "+")):
            continue
        if not COMPLETION_WORDS_RE.search(stripped):
            continue
        if EVIDENCE_MARKERS_RE.search(stripped):
            continue
        failures.append(stripped[:120])

    if failures:
        print(
            f"\nFAIL: {len(failures)} completion claim(s) lack evidence markers:",
            file=sys.stderr,
        )
        for line in failures:
            print(f"  {line}", file=sys.stderr)
        print(
            "\nAdd an evidence marker (test file, docs path, SQL function, etc.).",
            file=sys.stderr,
        )
        return 1

    print(
        "OK: all completion claims in version section have evidence markers "
        "(or section contains no unevidenced claims)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
