#!/usr/bin/env bash
# Install one release archive into the runner's pgrx PostgreSQL and run smoke SQL.
set -euo pipefail

ARCHIVE="${1:?usage: package_install_smoke.sh ARCHIVE --platform PLATFORM}"
PLATFORM="${3:?usage: package_install_smoke.sh ARCHIVE --platform PLATFORM}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/pg-ripple-package.XXXXXX")"
PGHOST="${PGHOST:-${PGRX_HOST:-${HOME}/.pgrx}}"
PGPORT="${PGPORT:-${PGRX_PORT:-28818}}"
PGUSER="${PGUSER:-${PGRX_USER:-$(whoami)}}"
PSQL=(psql -X -v ON_ERROR_STOP=1 -h "$PGHOST" -p "$PGPORT" -U "$PGUSER")

cleanup() {
    cargo pgrx stop pg18 >/dev/null 2>&1 || true
    rm -rf "$WORKDIR"
}
trap cleanup EXIT

PYTHON=python3
command -v "$PYTHON" >/dev/null 2>&1 || PYTHON=python
"$PYTHON" - "$ARCHIVE" "$WORKDIR" <<'PY'
import sys
import tarfile
import zipfile
from pathlib import Path

archive = Path(sys.argv[1])
destination = Path(sys.argv[2])
if archive.suffix == ".zip":
    with zipfile.ZipFile(archive) as bundle:
        bundle.extractall(destination)
elif archive.name.endswith(".tar.gz"):
    with tarfile.open(archive, "r:gz") as bundle:
        bundle.extractall(destination, filter="data")
else:
    raise SystemExit(f"unsupported package archive: {archive}")
PY

PACKAGE_ROOT="$(find "$WORKDIR" -type d -name extension -print -quit)"
[[ -n "$PACKAGE_ROOT" ]] || { echo "FAIL: extracted archive has no extension directory" >&2; exit 1; }
PACKAGE_ROOT="$(dirname "$PACKAGE_ROOT")"
"$PYTHON" "$ROOT/scripts/check_package_artifact.py" "$PACKAGE_ROOT" --platform "$PLATFORM"

PG_CONFIG="${PG_CONFIG:-$(cargo pgrx info path pg18)/bin/pg_config}"
PG_SHARE="$("$PG_CONFIG" --sharedir)"
PG_LIB="$("$PG_CONFIG" --pkglibdir)"
cp "$PACKAGE_ROOT/extension/pg_ripple.control" "$PG_SHARE/extension/"
find "$PACKAGE_ROOT/extension" -maxdepth 1 -name '*.sql' -exec cp {} "$PG_SHARE/extension/" \;
find "$PACKAGE_ROOT/lib" -maxdepth 1 -type f \( -name '*.so' -o -name '*.dylib' -o -name '*.dll' \) -exec cp {} "$PG_LIB/" \;

cargo pgrx start pg18 --postgresql-conf "allow_system_table_mods=on"
"${PSQL[@]}" -d postgres -c "CREATE EXTENSION pg_ripple; SELECT pg_ripple.triple_count(); DROP EXTENSION pg_ripple;" >/dev/null
echo "PASS: ${PLATFORM} package installed, smoke-tested, and unloaded"
