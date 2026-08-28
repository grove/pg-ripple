#!/usr/bin/env bash
# Shared helpers for the v0.133.0 resilience qualification scripts.

set -euo pipefail

resilience_die() {
    echo "FAIL: $*" >&2
    exit 1
}

resilience_skip() {
    echo "SKIP: $*"
    exit 0
}

resilience_require_cmd() {
    local command_name="$1"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        resilience_die "required command '${command_name}' is not available"
    fi
}

resilience_require_opt_in() {
    [[ "${PG_RIPPLE_RESILIENCE_ALLOW_DESTRUCTIVE:-0}" == "1" ]] ||
        resilience_die "refusing live resilience test; set PG_RIPPLE_RESILIENCE_ALLOW_DESTRUCTIVE=1 on a disposable cluster"
}

resilience_require_topology_opt_in() {
    resilience_require_opt_in
    [[ "${PG_RIPPLE_RESILIENCE_ALLOW_TOPOLOGY_CHANGE:-0}" == "1" ]] ||
        resilience_die "refusing promotion test; set PG_RIPPLE_RESILIENCE_ALLOW_TOPOLOGY_CHANGE=1 after fencing the old primary"
}

resilience_require_safe_id() {
    local value="$1"
    local label="$2"
    [[ "$value" =~ ^[a-zA-Z0-9_]+$ ]] ||
        resilience_die "${label} must contain only letters, digits, and underscores"
}

resilience_find_pgctl() {
    if [[ -n "${PGCTL:-}" ]]; then
        [[ -x "$PGCTL" ]] || resilience_die "PGCTL is not executable: ${PGCTL}"
        return
    fi
    if ! PGCTL="$(command -v pg_ctl)"; then
        resilience_die "pg_ctl is required for restart verification"
    fi
    export PGCTL
}

resilience_find_timeout() {
    if command -v timeout >/dev/null 2>&1; then
        TIMEOUT_BIN="$(command -v timeout)"
    elif command -v gtimeout >/dev/null 2>&1; then
        TIMEOUT_BIN="$(command -v gtimeout)"
    else
        resilience_die "GNU timeout (timeout or gtimeout) is required to bound a resilience scenario"
    fi
    export TIMEOUT_BIN
}

resilience_wait_for_sql() {
    local attempts="$1"
    shift
    local attempt
    for ((attempt = 1; attempt <= attempts; attempt++)); do
        if "$@" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    resilience_die "PostgreSQL did not become ready after ${attempts} seconds"
}

resilience_cleanup_tempdir() {
    local directory="$1"
    [[ -n "$directory" && "$directory" != "/" && -d "$directory" ]] || return 0
    rm -rf -- "$directory"
}
