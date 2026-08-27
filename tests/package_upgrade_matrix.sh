#!/usr/bin/env bash
# Upgrade databases created from recent release archives to the candidate.
set -euo pipefail

CANDIDATE_ARCHIVE="${1:?usage: package_upgrade_matrix.sh CANDIDATE_ARCHIVE RECENT_ARCHIVES_DIR}"
RECENT_DIR="${2:?usage: package_upgrade_matrix.sh CANDIDATE_ARCHIVE RECENT_ARCHIVES_DIR}"
PGHOST="${PGHOST:-${PGRX_HOST:-${HOME}/.pgrx}}"
PGPORT="${PGPORT:-${PGRX_PORT:-28818}}"
PGUSER="${PGUSER:-${PGRX_USER:-$(whoami)}}"
PSQL=(psql -X -v ON_ERROR_STOP=1 -h "$PGHOST" -p "$PGPORT" -U "$PGUSER")
PG_CONFIG="${PG_CONFIG:-$(cargo pgrx info path pg18)/bin/pg_config}"
PG_SHARE="$($PG_CONFIG --sharedir)"
PG_LIB="$($PG_CONFIG --pkglibdir)"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/pg-ripple-upgrades.XXXXXX")"
TARGET_VERSION="$(sed -n "s/^default_version = '\([^']*\)'/\1/p" pg_ripple.control)"

cleanup() {
    cargo pgrx stop pg18 >/dev/null 2>&1 || true
    rm -rf "$WORKDIR"
}
trap cleanup EXIT

extract() {
    local archive="$1" destination="$2"
    python3 - "$archive" "$destination" <<'PY'
import sys
import tarfile
from pathlib import Path

archive = Path(sys.argv[1])
destination = Path(sys.argv[2])
with tarfile.open(archive, "r:gz") as bundle:
    bundle.extractall(destination, filter="data")
PY
    find "$destination" -type d -name extension -print -quit | while IFS= read -r extension; do
        dirname "$extension"
        break
    done
}

install_package() {
    local package_root="$1"
    find "$PG_SHARE/extension" -maxdepth 1 -type f \
        \( -name 'pg_ripple.control' -o -name 'pg_ripple--*.sql' \) -delete
    find "$PG_LIB" -maxdepth 1 -type f -name 'pg_ripple.*' -delete
    cp "$package_root/extension/pg_ripple.control" "$PG_SHARE/extension/"
    find "$package_root/extension" -maxdepth 1 -name '*.sql' -exec cp {} "$PG_SHARE/extension/" \;
    find "$package_root/lib" -maxdepth 1 -type f -name '*.so' -exec cp {} "$PG_LIB/" \;
}

shopt -s nullglob
archives=("$RECENT_DIR"/*/*.tar.gz)
[[ "${#archives[@]}" -ge 6 ]] || {
    echo "FAIL: expected six recent release archives in $RECENT_DIR, found ${#archives[@]}" >&2
    exit 1
}

for archive in "${archives[@]}"; do
    version="$(basename "$(dirname "$archive")")"
    old_root="$WORKDIR/old-$version"
    candidate_root="$WORKDIR/candidate-$version"
    mkdir -p "$old_root" "$candidate_root"
    old_package="$(extract "$archive" "$old_root")"
    candidate_package="$(extract "$CANDIDATE_ARCHIVE" "$candidate_root")"
    old_version="$(sed -n "s/^default_version = '\([^']*\)'/\1/p" "$old_package/extension/pg_ripple.control")"
    [[ "$old_version" == "$version" ]] || {
        echo "FAIL: expected $version archive, found $old_version" >&2
        exit 1
    }

    install_package "$old_package"
    cargo pgrx start pg18 --postgresql-conf "allow_system_table_mods=on" >/dev/null
    db="pg_ripple_artifact_upgrade_${version//./_}_$$"
    "${PSQL[@]}" -d postgres -c "CREATE DATABASE \"$db\";" >/dev/null
    "${PSQL[@]}" -d "$db" -c "CREATE EXTENSION pg_ripple; SELECT pg_ripple.insert_triple('<https://upgrade.example/s>', '<https://upgrade.example/p>', '\"$version\"');" >/dev/null
    before="$(${PSQL[@]} -d "$db" -tAc 'SELECT pg_ripple.triple_count();' | tr -d '[:space:]')"
    [[ "$before" == 1 ]] || { echo "FAIL: $version archive smoke insert returned $before rows" >&2; exit 1; }

    cargo pgrx stop pg18 >/dev/null
    install_package "$candidate_package"
    cargo pgrx start pg18 --postgresql-conf "allow_system_table_mods=on" >/dev/null
    "${PSQL[@]}" -d "$db" -c "ALTER EXTENSION pg_ripple UPDATE TO '$TARGET_VERSION';" >/dev/null
    after="$(${PSQL[@]} -d "$db" -tAc 'SELECT pg_ripple.triple_count();' | tr -d '[:space:]')"
    installed="$(${PSQL[@]} -d "$db" -tAc "SELECT extversion FROM pg_extension WHERE extname='pg_ripple';" | tr -d '[:space:]')"
    [[ "$installed" == "$TARGET_VERSION" && "$after" == "$before" ]] || {
        echo "FAIL: $version -> $TARGET_VERSION upgrade lost data or version ($installed, $after)" >&2
        exit 1
    }
    "${PSQL[@]}" -d postgres -c "DROP DATABASE \"$db\";" >/dev/null
    cargo pgrx stop pg18 >/dev/null
    echo "PASS: $version artifact upgraded to $TARGET_VERSION with $after triple(s) preserved"
done
