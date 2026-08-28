#!/usr/bin/env python3
"""Validate temporary conformance exceptions before they can affect a gate."""

from __future__ import annotations

import argparse
import datetime as dt
import re
import sys
from pathlib import Path


FIELD_RE = re.compile(
    r"\b(issue|owner|rationale|expires)=([^\s]+(?:\s+(?!issue=|owner=|rationale=|expires=)[^\s]+)*)"
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("path", nargs="?", default="tests/conformance/known_failures.txt")
    args = parser.parse_args()
    today = dt.date.today()
    errors: list[str] = []
    count = 0
    seen: set[tuple[str, str]] = set()

    for number, raw in enumerate(Path(args.path).read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        count += 1
        token, _, fields = line.partition(" ")
        if ":" not in token:
            errors.append(f"L{number}: entry must start with suite:key")
            continue
        suite, key = token.split(":", 1)
        if not suite or not key or (suite, key) in seen:
            errors.append(f"L{number}: duplicate or empty suite/key")
        seen.add((suite, key))
        values = {name: value.strip() for name, value in FIELD_RE.findall(fields)}
        for field in ("issue", "owner", "rationale", "expires"):
            if not values.get(field):
                errors.append(f"L{number}: missing {field} metadata")
        if values.get("issue", "").startswith("http") is False:
            errors.append(f"L{number}: issue must be an HTTP(S) URL")
        try:
            expiry = dt.date.fromisoformat(values["expires"])
            if expiry < today:
                errors.append(f"L{number}: exception expired on {expiry.isoformat()}")
        except (KeyError, ValueError):
            errors.append(f"L{number}: expires must be YYYY-MM-DD")

    if errors:
        print("known-failure metadata validation failed:", file=sys.stderr)
        print("\n".join(f"  {error}" for error in errors), file=sys.stderr)
        return 1
    print(f"OK: validated {count} temporary conformance exception(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
