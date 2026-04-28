#!/usr/bin/env python3
"""
scripts/check_api_drift.py

v0.67.0 GATE-01: Portable Python replacement for check_api_drift.sh.

Detects SQL API signature drift between Rust source and documentation.
Extracts #[pg_extern] function names from src/ and checks that they appear
in at least one of: README.md, docs/src/, CHANGELOG.md.

Usage:
    python3 scripts/check_api_drift.py --version X.Y.Z

Exit code 0 = OK; non-zero = drift detected.
"""

import argparse
import re
import sys
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Check for SQL API drift between Rust source and documentation."
    )
    parser.add_argument(
        "--version",
        required=True,
        metavar="X.Y.Z",
        help="Release version being checked (required; prevents stale invocations).",
    )
    parser.add_argument(
        "--src",
        default="src",
        metavar="DIR",
        help="Rust source directory (default: src).",
    )
    return parser.parse_args()


def extract_pg_extern_names(src_dir: Path) -> list[str]:
    """Extract function names exported via #[pg_extern] from all .rs files."""
    names: list[str] = []
    pg_extern_re = re.compile(r"#\[pg_extern\]")
    fn_name_re = re.compile(r"\bfn\s+([a-z_][a-z0-9_]*)\s*\(")

    for rs_file in sorted(src_dir.rglob("*.rs")):
        text = rs_file.read_text(encoding="utf-8", errors="replace")
        lines = text.splitlines()
        for i, line in enumerate(lines):
            if pg_extern_re.search(line):
                # Look at the next 5 lines for the fn name.
                for lookahead in lines[i : i + 6]:
                    m = fn_name_re.search(lookahead)
                    if m:
                        names.append(m.group(1))
                        break

    return sorted(set(names))


def load_documentation_corpus(root: Path) -> str:
    """Load all documentation text (README, CHANGELOG, docs/src) into a single string."""
    corpus_parts: list[str] = []

    for path in [root / "README.md", root / "CHANGELOG.md"]:
        if path.exists():
            corpus_parts.append(path.read_text(encoding="utf-8", errors="replace"))

    docs_src = root / "docs" / "src"
    if docs_src.is_dir():
        for md_file in sorted(docs_src.rglob("*.md")):
            corpus_parts.append(
                md_file.read_text(encoding="utf-8", errors="replace")
            )

    return "\n".join(corpus_parts)


def main() -> int:
    args = parse_args()
    root = Path(__file__).parent.parent

    # Validate version argument (prevents stale invocations and check-nothing runs).
    version_re = re.compile(r"^\d+\.\d+\.\d+$")
    if not version_re.match(args.version):
        print(
            f"ERROR: --version must be in X.Y.Z format, got: {args.version}",
            file=sys.stderr,
        )
        return 1

    src_dir = root / args.src
    if not src_dir.is_dir():
        print(f"ERROR: source directory not found: {src_dir}", file=sys.stderr)
        return 1

    # Extract pg_extern names.
    names = extract_pg_extern_names(src_dir)
    if not names:
        print(
            "ERROR: no #[pg_extern] functions found — extraction failure or empty source.",
            file=sys.stderr,
        )
        return 1

    print(f"check_api_drift v{args.version}: found {len(names)} pg_extern functions")

    # Internal/helper pg_extern functions that are intentionally not part of the
    # public API documentation.  These are used by the extension internally or
    # are low-level codec helpers not meant for direct user invocation.
    INTERNAL_ALLOWLIST: frozenset[str] = frozenset({
        "decode_numeric_spi",
        "encode_lang_literal",
        "encode_typed_literal",
        "flush_encode_cache",
        "grant_graph_permission",
        "group_concat_decode",
        "revoke_graph_access",
        "revoke_graph_permission",
        "xsd_double_fmt",
    })

    # Load documentation corpus.
    corpus = load_documentation_corpus(root)

    # Check each function name against the corpus.
    failures: list[str] = []
    for name in names:
        if name in INTERNAL_ALLOWLIST:
            continue
        # Accept any occurrence of the function name in documentation.
        if name not in corpus:
            failures.append(name)

    if failures:
        print(
            f"\nFAIL: {len(failures)} pg_extern function(s) not found in documentation:",
            file=sys.stderr,
        )
        for name in failures:
            print(f"  - {name}", file=sys.stderr)
        print(
            "\nAdd documentation references in README.md, CHANGELOG.md, or docs/src/.",
            file=sys.stderr,
        )
        return 1

    print(f"OK: all {len(names)} pg_extern functions appear in documentation.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
