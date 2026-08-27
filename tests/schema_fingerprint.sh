#!/usr/bin/env bash
# Compare a fresh install with the complete sequential upgrade path.
set -euo pipefail

PGHOST="${PGHOST:-${PGRX_HOST:-${HOME}/.pgrx}}"
PGPORT="${PGPORT:-${PGRX_PORT:-28818}}"
PGUSER="${PGUSER:-${PGRX_USER:-$(whoami)}}"
PSQL=(psql -X -v ON_ERROR_STOP=1 -h "$PGHOST" -p "$PGPORT" -U "$PGUSER")
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GRAPH="$ROOT/target/migration-graph.json"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/pg-ripple-fingerprint.XXXXXX")"
FRESH_DB="pg_ripple_fresh_fingerprint_$$"
UPGRADE_DB="pg_ripple_upgrade_fingerprint_$$"

cleanup() {
    "${PSQL[@]}" -d postgres -c "DROP DATABASE IF EXISTS \"${FRESH_DB}\";" >/dev/null 2>&1 || true
    "${PSQL[@]}" -d postgres -c "DROP DATABASE IF EXISTS \"${UPGRADE_DB}\";" >/dev/null 2>&1 || true
    rm -rf "$WORKDIR"
}
trap cleanup EXIT

python3 "$ROOT/scripts/migration_graph.py" --output "$GRAPH" >/dev/null
BASE_VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["base_version"])' "$GRAPH")"
TARGET_VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["target_version"])' "$GRAPH")"

PG_SHARE="$(pg_config --sharedir 2>/dev/null || true)"
if [[ ! -f "$PG_SHARE/extension/pg_ripple--${BASE_VERSION}.sql" ]]; then
    echo "SKIP: pg_ripple ${BASE_VERSION} installation script is not installed"
    exit 0
fi

"${PSQL[@]}" -d postgres -c "SELECT 1;" >/dev/null
available="$("${PSQL[@]}" -d postgres -tAc "SELECT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pg_ripple');")"
[[ "$available" == t ]] || { echo "FAIL: pg_ripple is not installed" >&2; exit 1; }
"${PSQL[@]}" -d postgres -c "CREATE DATABASE \"${FRESH_DB}\";" >/dev/null
"${PSQL[@]}" -d postgres -c "CREATE DATABASE \"${UPGRADE_DB}\";" >/dev/null

"${PSQL[@]}" -d "$FRESH_DB" -c "CREATE EXTENSION pg_ripple;" >/dev/null
"${PSQL[@]}" -d "$UPGRADE_DB" -c "CREATE EXTENSION pg_ripple VERSION '${BASE_VERSION}';" >/dev/null
while IFS= read -r version; do
    "${PSQL[@]}" -d "$UPGRADE_DB" -c "ALTER EXTENSION pg_ripple UPDATE TO '${version}';" >/dev/null
done < <(python3 -c 'import json,sys; print("\n".join(e["to"] for e in json.load(open(sys.argv[1]))["migrations"]))' "$GRAPH")

"${PSQL[@]}" -d "$FRESH_DB" -At -f "$ROOT/scripts/schema_fingerprint.sql" >"$WORKDIR/fresh.json"
"${PSQL[@]}" -d "$UPGRADE_DB" -At -f "$ROOT/scripts/schema_fingerprint.sql" >"$WORKDIR/upgrade.json"
python3 "$ROOT/scripts/compare_schema_fingerprints.py" "$WORKDIR/fresh.json" "$WORKDIR/upgrade.json"
echo "PASS: fresh ${TARGET_VERSION} and sequential upgrade fingerprints match"
