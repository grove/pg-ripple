#!/usr/bin/env bash
# v0.133.0 deterministic resilience qualification driver.
#
# The live matrix deliberately reuses the established crash-recovery scripts.
# --validate is the CI-safe path: it performs no database or filesystem
# mutation outside the repository and needs no PostgreSQL cluster.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

CORE_SCENARIOS=(
    load-sigkill-restart
    merge-sigkill-restart
    promotion-sigkill-restart
    writeback-sigkill-restart
    inference-sigkill-restart
    upgrade-sigkill-restart
    logical-dump-restore
)

OPTIONAL_SCENARIOS=(
    physical-backup-restore-pitr
    primary-standby-promotion
    read-replica-write-safety
    resource-pressure
)

all_scenarios() {
    printf '%s\n' "${CORE_SCENARIOS[@]}" "${OPTIONAL_SCENARIOS[@]}"
}

scenario_path() {
    case "$1" in
        load-sigkill-restart) printf '%s\n' "$ROOT/tests/crash_recovery/dict_during_kill.sh" ;;
        merge-sigkill-restart) printf '%s\n' "$ROOT/tests/crash_recovery/merge_kill.sh" ;;
        promotion-sigkill-restart) printf '%s\n' "$ROOT/tests/crash_recovery/promote_sigkill.sh" ;;
        writeback-sigkill-restart) printf '%s\n' "$ROOT/tests/crash_recovery/test_construct_view_kill.sh" ;;
        inference-sigkill-restart) printf '%s\n' "$ROOT/tests/crash_recovery/test_inference_kill.sh" ;;
        upgrade-sigkill-restart) printf '%s\n' "$ROOT/tests/upgrade_recovery.sh" ;;
        logical-dump-restore) printf '%s\n' "$ROOT/tests/integration/dump_restore.sh" ;;
        physical-backup-restore-pitr) printf '%s\n' "$SCRIPT_DIR/physical_backup_restore.sh" ;;
        primary-standby-promotion|read-replica-write-safety) printf '%s\n' "$SCRIPT_DIR/ha_safety.sh" ;;
        resource-pressure) printf '%s\n' "$SCRIPT_DIR/resource_pressure.sh" ;;
        *) return 1 ;;
    esac
}

scenario_args() {
    case "$1" in
        primary-standby-promotion) printf '%s\n' --failover ;;
        read-replica-write-safety) printf '%s\n' --read-replica ;;
        *) return 0 ;;
    esac
}

validate() {
    local scenario path script
    while IFS= read -r scenario; do
        path="$(scenario_path "$scenario")" || resilience_die "scenario has no command mapping: ${scenario}"
        [[ -f "$path" ]] || resilience_die "mapped script is missing: ${path}"
        if [[ "$path" == "$SCRIPT_DIR"/* ]]; then
            [[ -x "$path" ]] || resilience_die "new resilience script is not executable: ${path}"
        else
            [[ -r "$path" ]] || resilience_die "mapped legacy script is not readable: ${path}"
        fi
    done < <(all_scenarios)

    for script in "$SCRIPT_DIR"/*.sh; do
        bash -n "$script"
    done

    for script in "$SCRIPT_DIR/lib.sh" "$SCRIPT_DIR/physical_backup_restore.sh" \
        "$SCRIPT_DIR/ha_safety.sh" "$SCRIPT_DIR/resource_pressure.sh"; do
        if grep -nE '\|\|[[:space:]]*true' "$script"; then
            resilience_die "new resilience scripts must not mask failures with '|| true'"
        fi
    done

    for scenario in "${CORE_SCENARIOS[@]}" "${OPTIONAL_SCENARIOS[@]}"; do
        grep -Fq "$scenario" "$ROOT/docs/src/operations/production-checklist.md" ||
            resilience_die "runbook is missing scenario ${scenario}"
    done
    grep -Fq 'tests/resilience/fault_matrix.sh --validate' \
        "$ROOT/docs/src/operations/production-checklist.md" ||
        resilience_die "runbook is missing the CI-safe validation command"

    local expected_order="${CORE_SCENARIOS[*]}"
    [[ "$expected_order" == "load-sigkill-restart merge-sigkill-restart promotion-sigkill-restart writeback-sigkill-restart inference-sigkill-restart upgrade-sigkill-restart logical-dump-restore" ]] ||
        resilience_die "core scenario order is not deterministic"
    echo "PASS: v0.133.0 resilience matrix mappings, shell syntax, and runbook references are valid"
}

usage() {
    cat <<'EOF'
usage: fault_matrix.sh [--validate|--list|--all|--optional|--scenario NAME]

  --validate          CI-safe mapping, syntax, and runbook validation
  --list              print scenarios without running them
  --all               run the required core matrix in deterministic order
  --optional          run environment-gated backup, HA, and pressure checks
  --scenario NAME     run one named scenario
EOF
}

run_scenario() {
    local scenario="$1"
    local path args=()
    path="$(scenario_path "$scenario")" || resilience_die "unknown scenario: ${scenario}"
    while IFS= read -r arg; do
        [[ -n "$arg" ]] && args+=("$arg")
    done < <(scenario_args "$scenario")

    resilience_require_opt_in
    resilience_require_cmd psql
    resilience_find_timeout
    export PG_RIPPLE_RESILIENCE_RUN_ID="${PG_RIPPLE_RESILIENCE_RUN_ID:-v0133}"
    export PG_RIPPLE_RESILIENCE_REQUIRE_SIGKILL=1

    echo "=== ${scenario} ==="
    if ! "$TIMEOUT_BIN" --foreground --kill-after=10 "${PG_RIPPLE_RESILIENCE_CASE_TIMEOUT:-900}" \
        bash "$path" "${args[@]}"; then
        resilience_die "scenario failed: ${scenario}"
    fi
    echo "PASS: ${scenario}"
}

main() {
    case "${1:---all}" in
        --validate)
            validate
            ;;
        --list)
            all_scenarios
            ;;
        --all)
            local scenario
            for scenario in "${CORE_SCENARIOS[@]}"; do
                run_scenario "$scenario"
            done
            ;;
        --optional)
            local scenario
            for scenario in "${OPTIONAL_SCENARIOS[@]}"; do
                run_scenario "$scenario"
            done
            ;;
        --scenario)
            [[ $# -eq 2 ]] || { usage >&2; exit 2; }
            run_scenario "$2"
            ;;
        --help|-h)
            usage
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
}

main "$@"
