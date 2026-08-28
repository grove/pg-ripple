#!/usr/bin/env bash
# v0.133.0 physical base-backup/restore and optional PITR smoke test.
#
# This script never touches a cluster unless the caller explicitly opts in.
# It restores into disposable data directories under a private temporary
# directory; the source cluster is only used as the pg_basebackup source.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

[[ "${PG_RIPPLE_RUN_PHYSICAL:-0}" == "1" ]] ||
    resilience_skip "set PG_RIPPLE_RUN_PHYSICAL=1 to run physical restore against the configured disposable source"
resilience_require_opt_in
for command_name in pg_basebackup psql cp find; do
    resilience_require_cmd "$command_name"
done
resilience_find_pgctl

SOURCE_DATA="${PG_RIPPLE_PRIMARY_DATA:-${PGDATA:-}}"
[[ -n "$SOURCE_DATA" && -d "$SOURCE_DATA" ]] ||
    resilience_die "PG_RIPPLE_PRIMARY_DATA or PGDATA must point to the running source data directory"
SOURCE_DB="${PGDATABASE:-postgres}"
RUN_ID="${PG_RIPPLE_RESILIENCE_RUN_ID:-v0133}"
resilience_require_safe_id "$RUN_ID" PG_RIPPLE_RESILIENCE_RUN_ID
MARKER="pg_ripple_resilience_marker_${RUN_ID}"
PITR_TARGET="pg_ripple_resilience_good_${RUN_ID}"

WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/pg-ripple-physical.XXXXXX")"
BASE_DIR="$WORKDIR/base"
RESTORE_DIR="$WORKDIR/restore"
PITR_DIR="$WORKDIR/pitr"
RESTORE_SOCKET="$WORKDIR/restore-socket"
PITR_SOCKET="$WORKDIR/pitr-socket"
RESTORE_PORT="${PG_RIPPLE_RESTORE_PORT:-55433}"
PITR_PORT="${PG_RIPPLE_PITR_PORT:-55434}"
SOURCE_READY=0

[[ "$RESTORE_PORT" =~ ^[0-9]+$ && "$PITR_PORT" =~ ^[0-9]+$ ]] ||
    resilience_die "PG_RIPPLE_RESTORE_PORT and PG_RIPPLE_PITR_PORT must be numeric"
[[ "${PG_RIPPLE_WAL_ARCHIVE_DIR:-}" != *"'"* ]] ||
    resilience_die "PG_RIPPLE_WAL_ARCHIVE_DIR must not contain a single quote"

PSQL=(psql -X -v ON_ERROR_STOP=1 -d "$SOURCE_DB")

cleanup_cluster() {
    local directory="$1"
    if [[ -d "$directory" ]] && "$PGCTL" status -D "$directory" >/dev/null 2>&1; then
        "$PGCTL" stop -D "$directory" -m fast -w -t 60 >/dev/null
    fi
}

cleanup_status=0
cleanup() {
    if [[ -d "$RESTORE_DIR" ]] && ! cleanup_cluster "$RESTORE_DIR"; then
        echo "CLEANUP: failed to stop restored cluster at ${RESTORE_DIR}" >&2
        cleanup_status=1
    fi
    if [[ -d "$PITR_DIR" ]] && ! cleanup_cluster "$PITR_DIR"; then
        echo "CLEANUP: failed to stop PITR cluster at ${PITR_DIR}" >&2
        cleanup_status=1
    fi
    if (( SOURCE_READY )); then
        if ! "${PSQL[@]}" -c "DROP TABLE IF EXISTS public.${MARKER};" >/dev/null; then
            echo "CLEANUP: failed to remove ${MARKER} from ${SOURCE_DB}" >&2
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
    if (( status == 0 && cleanup_status != 0 )); then
        status=1
    fi
    exit "$status"
}
trap finish EXIT

echo "=== physical base-backup/restore ==="
resilience_wait_for_sql 30 "${PSQL[@]}" -Atqc "SELECT 1"
SOURCE_READY=1
source_has_extension="$("${PSQL[@]}" -Atqc "SELECT to_regclass('_pg_ripple.dictionary') IS NOT NULL")"
[[ "$source_has_extension" == "t" ]] ||
    resilience_die "${SOURCE_DB} does not contain the pg_ripple internal catalog"

"${PSQL[@]}" -c "DROP TABLE IF EXISTS public.${MARKER}; CREATE TABLE public.${MARKER}(value text NOT NULL); INSERT INTO public.${MARKER} VALUES ('base');" >/dev/null
"${PSQL[@]}" -c "CHECKPOINT;" >/dev/null

echo "--- taking pg_basebackup ---"
BASEBACKUP=(pg_basebackup -D "$BASE_DIR" -Fp -X stream -c fast -P)
[[ -n "${PGHOST:-}" ]] && BASEBACKUP+=( -h "$PGHOST" )
[[ -n "${PGPORT:-}" ]] && BASEBACKUP+=( -p "$PGPORT" )
[[ -n "${PGUSER:-}" ]] && BASEBACKUP+=( -U "$PGUSER" )
"${BASEBACKUP[@]}"

"${PSQL[@]}" -c "UPDATE public.${MARKER} SET value = 'after-backup';" >/dev/null
mkdir -p "$RESTORE_SOCKET"
"$PGCTL" start -D "$BASE_DIR" -o "-p ${RESTORE_PORT} -k ${RESTORE_SOCKET}" \
    -l "$WORKDIR/restore.log" -w -t 60 >/dev/null
RESTORE_DIR="$BASE_DIR"

RESTORE_PSQL=(psql -X -v ON_ERROR_STOP=1 -h "$RESTORE_SOCKET" -p "$RESTORE_PORT" -d "$SOURCE_DB")
resilience_wait_for_sql 60 "${RESTORE_PSQL[@]}" -Atqc "SELECT 1"
restored_marker="$("${RESTORE_PSQL[@]}" -Atqc "SELECT value FROM public.${MARKER}")"
[[ "$restored_marker" == "base" ]] ||
    resilience_die "base restore saw '${restored_marker}', expected the pre-backup marker"
"${RESTORE_PSQL[@]}" -Atqc "SELECT pg_ripple.health() IS NOT NULL AND pg_ripple.triple_count() >= 0" | grep -Fxq t ||
    resilience_die "restored cluster failed the pg_ripple health/triple-count check"
echo "PASS: physical restore preserved pg_ripple catalogs and the base marker"

if [[ "${PG_RIPPLE_RUN_PITR:-0}" == "1" ]]; then
    ARCHIVE_DIR="${PG_RIPPLE_WAL_ARCHIVE_DIR:-}"
    [[ -n "$ARCHIVE_DIR" && -d "$ARCHIVE_DIR" ]] ||
        resilience_die "PITR requires PG_RIPPLE_WAL_ARCHIVE_DIR pointing at the primary archive directory"
    archive_mode="$("${PSQL[@]}" -Atqc "SHOW archive_mode")"
    [[ "$archive_mode" == "on" ]] ||
        resilience_die "PITR requires archive_mode=on; enable WAL archiving before this smoke test"
    archive_command="$("${PSQL[@]}" -Atqc "SHOW archive_command")"
    [[ -n "$archive_command" && "$archive_command" != "(disabled)" ]] ||
        resilience_die "PITR requires a working archive_command; configure it to write ${ARCHIVE_DIR}"

    before_archive_count="$(find "$ARCHIVE_DIR" -type f -print | wc -l | tr -d '[:space:]')"
    "${PSQL[@]}" -c "UPDATE public.${MARKER} SET value = 'good'; SELECT pg_create_restore_point('${PITR_TARGET}'); UPDATE public.${MARKER} SET value = 'bad'; SELECT pg_switch_wal();" >/dev/null
    archive_ready=0
    for ((attempt = 1; attempt <= 90; attempt++)); do
        archive_count="$(find "$ARCHIVE_DIR" -type f -print | wc -l | tr -d '[:space:]')"
        if (( archive_count > before_archive_count )); then
            archive_ready=1
            break
        fi
        sleep 1
    done
    (( archive_ready )) || resilience_die "PITR WAL did not reach ${ARCHIVE_DIR} within 90 seconds"

    cleanup_cluster "$BASE_DIR"
    cp -a "$BASE_DIR" "$PITR_DIR"
    printf "restore_command = 'cp %s/%%f %%p'\nrecovery_target_name = '%s'\nrecovery_target_action = 'promote'\n" \
        "$ARCHIVE_DIR" "$PITR_TARGET" > "$PITR_DIR/postgresql.auto.conf"
    touch "$PITR_DIR/recovery.signal"
    mkdir -p "$PITR_SOCKET"
    "$PGCTL" start -D "$PITR_DIR" -o "-p ${PITR_PORT} -k ${PITR_SOCKET}" \
        -l "$WORKDIR/pitr.log" -w -t 120 >/dev/null
    PITR_PSQL=(psql -X -v ON_ERROR_STOP=1 -h "$PITR_SOCKET" -p "$PITR_PORT" -d "$SOURCE_DB")
    resilience_wait_for_sql 120 "${PITR_PSQL[@]}" -Atqc "SELECT 1"
    "${PITR_PSQL[@]}" -Atqc "SELECT pg_is_in_recovery()" | grep -Fxq f ||
        resilience_die "PITR cluster did not promote after reaching ${PITR_TARGET}"
    pitr_marker="$("${PITR_PSQL[@]}" -Atqc "SELECT value FROM public.${MARKER}")"
    [[ "$pitr_marker" == "good" ]] ||
        resilience_die "PITR restored '${pitr_marker}', expected the named restore point marker 'good'"
    "${PITR_PSQL[@]}" -Atqc "SELECT pg_ripple.health() IS NOT NULL AND pg_ripple.triple_count() >= 0" | grep -Fxq t ||
        resilience_die "PITR cluster failed the pg_ripple health/triple-count check"
    echo "PASS: PITR stopped at ${PITR_TARGET} and preserved pg_ripple state"
else
    echo "SKIP: PITR (set PG_RIPPLE_RUN_PITR=1 with archive_mode=on and PG_RIPPLE_WAL_ARCHIVE_DIR)"
fi
