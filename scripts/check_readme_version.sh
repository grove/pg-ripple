#!/usr/bin/env bash
# check_readme_version.sh — verify README "What works today (vX.Y.Z)" matches Cargo.toml version.
# v0.70.0 README-02
set -euo pipefail

CARGO_VER=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
README_VER=$(grep -o 'What works today (v[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*)' README.md 2>/dev/null \
    | sed 's/.*What works today (v\([^)]*\)).*/\1/' || echo "NOT_FOUND")

if [[ "$CARGO_VER" != "$README_VER" ]]; then
  echo "ERROR: README version ($README_VER) does not match Cargo.toml ($CARGO_VER)"
  echo "       Update README.md 'What works today (v$CARGO_VER)' heading."
  exit 1
fi

echo "README version check passed: v$README_VER"
