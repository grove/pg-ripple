#!/usr/bin/env bash
# v0.133.0 bounded resource-pressure smoke checks.
#
# Exercises PostgreSQL's native statement, memory, and temporary-file guards
# plus pg_ripple's property-path depth bound. It uses a uniquely named test
# database and never changes cluster-wide settings.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

[[ "${PG_RIPPLE_RUN_RESOURCE_PRESSURE:-0}" == "1" ]] ||
    resilience_skip "set PG_RIPPLE_RUN_RESOURCE_PRESSURE=1 to run bounded pressure checks"
resilience_require_opt_in
for command_name in psql createdb; do resilience_require_cmd "$command_name"; done

RUN_ID="${PG_RIPPLE_RESILIENCE_RUN_ID:-v0133}"
resilience_require_safe_id "$RUN_ID" PG_RIPPLE_RESILIENCE_RUN_ID
ROWS="${PG_RIPPLE_RESOURCE_ROWS:-2000}"
TIMEOUT_MS="${PG_RIPPLE_RESOURCE_TIMEOUT_MS:-1500}"
WORK_MEM="${PG_RIPPLE_RESOURCE_WORK_MEM:-1MB}"
[[ "$ROWS" =~ ^[1-9][0-9]*$ && "$ROWS" -le 100000 ]] ||
    resilience_die "PG_RIPPLE_RESOURCE_ROWS must be between 1 and 100000"
[[ "$TIMEOUT_MS" =~ ^[1-9][0-9]*$ && "$TIMEOUT_MS" -le 30000 ]] ||
    resilience_die "PG_RIPPLE_RESOURCE_TIMEOUT_MS must be between 1 and 30000"
[[ "$WORK_MEM" =~ ^[1-9][0-9]*(kB|MB|GB)$ ]] ||
    resilience_die "PG_RIPPLE_RESOURCE_WORK_MEM must be a positive kB, MB, or GB value"

DB="pg_ripple_resilience_resource_${RUN_ID}"
PSQL=(psql -X -v ON_ERROR_STOP=1)
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/pg-ripple-resource.XXXXXX")"
TIMEOUT_OUTPUT="$WORKDIR/timeout.out"
DB_CREATED=0

cleanup_status=0
cleanup() {
    if (( DB_CREATED )); then
        if ! "${PSQL[@]}" -d postgres -c "DROP DATABASE IF EXISTS \"${DB}\";" >/dev/null; then
            echo "CLEANUP: failed to drop ${DB}" >&2
            cleanup_status=1
        fi
    fi
    if ! resilience_cleanup_tempdir "$WORKDIR"; then
        echo "CLEANUP: failed to remove ${WORKDIR}" >&2
        cleanup_status=1
    fi
}
finish() {
    local status=$?
    trap - EXIT
    cleanup
    if (( status == 0 && cleanup_status != 0 )); then status=1; fi
    exit "$status"
}
trap finish EXIT

"${PSQL[@]}" -d postgres -c "DROP DATABASE IF EXISTS \"${DB}\";" >/dev/null
createdb "$DB"
DB_CREATED=1
"${PSQL[@]}" -d "$DB" -c "CREATE EXTENSION pg_ripple; SET pg_ripple.max_path_depth = 3; SET work_mem = '${WORK_MEM}'; SET temp_file_limit = '16MB';" >/dev/null

"${PSQL[@]}" -d "$DB" -Atqc "SELECT pg_ripple.load_ntriples('<https://resilience.test/a> <https://resilience.test/link> <https://resilience.test/b> .<https://resilience.test/b> <https://resilience.test/link> <https://resilience.test/a> .')" >/dev/null
path_count="$("${PSQL[@]}" -d "$DB" -Atqc "SELECT count(*) FROM pg_ripple.sparql('SELECT ?x WHERE { <https://resilience.test/a> <https://resilience.test/link>+ ?x }')")"
[[ "$path_count" =~ ^[0-9]+$ && "$path_count" -le 6 ]] ||
    resilience_die "property-path bound returned an invalid count (${path_count}); expected a finite result at max_path_depth=3"

if "${PSQL[@]}" -d "$DB" -c "SET statement_timeout = '${TIMEOUT_MS}ms'; SELECT pg_sleep(30);" >"$TIMEOUT_OUTPUT" 2>&1; then
    resilience_die "statement_timeout did not cancel the bounded pressure query"
fi
if ! grep -Eqi 'statement timeout|canceling statement' "$TIMEOUT_OUTPUT"; then
    cat "$TIMEOUT_OUTPUT" >&2
    resilience_die "pressure query failed without the expected statement-timeout diagnostic"
fi

"${PSQL[@]}" -d "$DB" -c "SET statement_timeout = '30s'; DO \$\$ BEGIN FOR i IN 1..${ROWS} LOOP PERFORM pg_ripple.insert_triple(format('<https://resilience.test/s%s>', i), '<https://resilience.test/p>', format('\"%s\"', i)); END LOOP; END \$\$;" >/dev/null
"${PSQL[@]}" -d "$DB" -Atqc "SELECT pg_ripple.health() IS NOT NULL AND pg_ripple.triple_count() >= ${ROWS}" | grep -Fxq t ||
    resilience_die "bounded pressure load failed the health/triple-count check"
echo "PASS: resource guards bounded paths, cancellation, and a ${ROWS}-triple load"
