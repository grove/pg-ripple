#!/usr/bin/env bash
# v0.133.0 primary/standby promotion and read-replica write-safety checks.
#
# The failover mode is intentionally opt-in and requires local data-directory
# paths so the script can fence the old primary before promotion. The
# read-replica mode is read-only and only checks the configured replica.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

mode="${1:---read-replica}"
case "$mode" in
    --failover|--read-replica) ;;
    *) echo "usage: ha_safety.sh [--failover|--read-replica]" >&2; exit 2 ;;
esac

[[ "${PG_RIPPLE_RUN_HA:-0}" == "1" ]] ||
    resilience_skip "set PG_RIPPLE_RUN_HA=1 to run against the configured disposable topology"
resilience_require_cmd psql
resilience_find_pgctl
RUN_ID="${PG_RIPPLE_RESILIENCE_RUN_ID:-v0133}"
resilience_require_safe_id "$RUN_ID" PG_RIPPLE_RESILIENCE_RUN_ID

if [[ "$mode" == "--read-replica" ]]; then
    REPLICA_URL="${PG_RIPPLE_READ_REPLICA_URL:-}"
    [[ -n "$REPLICA_URL" ]] ||
        resilience_die "read-replica mode requires PG_RIPPLE_READ_REPLICA_URL"
    REPLICA_PSQL=(psql -X -v ON_ERROR_STOP=1 "$REPLICA_URL")
    is_recovery="$("${REPLICA_PSQL[@]}" -Atqc "SELECT pg_is_in_recovery()")"
    [[ "$is_recovery" == "t" ]] ||
        resilience_die "configured read replica is writable; pg_is_in_recovery() returned ${is_recovery}"
    "${REPLICA_PSQL[@]}" -Atqc "SELECT pg_ripple.health() IS NOT NULL" | grep -Fxq t ||
        resilience_die "read replica could not execute pg_ripple.health()"

    output_file="$(mktemp "${TMPDIR:-/tmp}/pg-ripple-replica-write.XXXXXX")"
    cleanup_status=0
    cleanup() {
        if ! rm -f -- "$output_file"; then
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

    if "${REPLICA_PSQL[@]}" -c "SELECT pg_ripple.insert_triple('<https://resilience.test/${RUN_ID}>','<https://resilience.test/write>','\"must-fail\"');" >"$output_file" 2>&1; then
        resilience_die "write unexpectedly succeeded on a read replica"
    fi
    if ! grep -Eqi 'read-only|recovery|cannot execute' "$output_file"; then
        cat "$output_file" >&2
        resilience_die "replica write failed for an unexpected reason; connection/auth failures are not a safety pass"
    fi
    echo "PASS: read replica accepted reads and rejected writes"
    exit 0
fi

resilience_require_topology_opt_in
PRIMARY_URL="${PG_RIPPLE_PRIMARY_URL:-}"
STANDBY_URL="${PG_RIPPLE_STANDBY_URL:-}"
PRIMARY_DATA="${PG_RIPPLE_PRIMARY_DATA:-}"
STANDBY_DATA="${PG_RIPPLE_STANDBY_DATA:-}"
[[ -n "$PRIMARY_URL" && -n "$STANDBY_URL" ]] ||
    resilience_die "failover mode requires PG_RIPPLE_PRIMARY_URL and PG_RIPPLE_STANDBY_URL"
[[ -d "$PRIMARY_DATA" && -d "$STANDBY_DATA" ]] ||
    resilience_die "failover mode requires local PG_RIPPLE_PRIMARY_DATA and PG_RIPPLE_STANDBY_DATA for fencing"

PRIMARY_PSQL=(psql -X -v ON_ERROR_STOP=1 "$PRIMARY_URL")
STANDBY_PSQL=(psql -X -v ON_ERROR_STOP=1 "$STANDBY_URL")
"${PRIMARY_PSQL[@]}" -Atqc "SELECT pg_is_in_recovery()" | grep -Fxq f ||
    resilience_die "configured primary is not writable"
"${STANDBY_PSQL[@]}" -Atqc "SELECT pg_is_in_recovery()" | grep -Fxq t ||
    resilience_die "configured standby is not in recovery"

MARKER="https://resilience.test/${RUN_ID}"
"${PRIMARY_PSQL[@]}" -c "SELECT pg_ripple.insert_triple('<${MARKER}>','<https://resilience.test/state>','\"before-promotion\"');" >/dev/null
for ((attempt = 1; attempt <= 60; attempt++)); do
    if [[ "$("${STANDBY_PSQL[@]}" -Atqc "SELECT count(*) FROM pg_ripple.sparql('SELECT ?s WHERE { ?s <https://resilience.test/state> ?o }') WHERE true")" =~ ^[1-9][0-9]*$ ]]; then
        break
    fi
    [[ "$attempt" -lt 60 ]] || resilience_die "standby did not replay the promotion marker within 60 seconds"
    sleep 1
done

"$PGCTL" stop -D "$PRIMARY_DATA" -m fast -w -t 60 >/dev/null
"$PGCTL" promote -D "$STANDBY_DATA" -w -t 60 >/dev/null
for ((attempt = 1; attempt <= 60; attempt++)); do
    if [[ "$("${STANDBY_PSQL[@]}" -Atqc "SELECT pg_is_in_recovery()")" == "f" ]]; then break; fi
    [[ "$attempt" -lt 60 ]] || resilience_die "standby did not leave recovery after promotion"
    sleep 1
done
"${STANDBY_PSQL[@]}" -c "SELECT pg_ripple.insert_triple('<${MARKER}>','<https://resilience.test/state>','\"after-promotion\"');" >/dev/null
"${STANDBY_PSQL[@]}" -Atqc "SELECT pg_ripple.health() IS NOT NULL AND pg_ripple.triple_count() >= 1" | grep -Fxq t ||
    resilience_die "promoted standby failed pg_ripple health/triple-count verification"
echo "PASS: fenced primary, promoted standby, and wrote successfully on the new primary"
