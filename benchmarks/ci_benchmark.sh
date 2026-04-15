#!/usr/bin/env bash
# ci_benchmark.sh — CI performance regression baseline for pg_ripple.
#
# Runs a simplified insert-throughput + point-query benchmark and outputs
# machine-readable results. Optionally compares against a stored baseline
# and exits non-zero if throughput regresses by more than the threshold.
#
# Usage:
#   bash benchmarks/ci_benchmark.sh              # run benchmark, print results
#   BASELINE_FILE=path/to/baseline.json \
#     REGRESS_THRESHOLD=10 \
#     bash benchmarks/ci_benchmark.sh             # compare against baseline
#
# Environment variables:
#   PGDATABASE          database to connect to (default: postgres)
#   BASELINE_FILE       path to JSON baseline file (optional)
#   REGRESS_THRESHOLD   max allowed regression percentage (default: 10)
#   RESULT_FILE         path to write results JSON (default: benchmark_results.json)

set -euo pipefail

DB="${PGDATABASE:-postgres}"
THRESHOLD="${REGRESS_THRESHOLD:-10}"
RESULT_FILE="${RESULT_FILE:-benchmark_results.json}"
BASELINE="${BASELINE_FILE:-}"

echo "=== pg_ripple CI Performance Benchmark ==="
echo "Database: $DB"
echo "Regression threshold: ${THRESHOLD}%"
echo ""

# ── Reset extension ───────────────────────────────────────────────────────────
psql -d "$DB" -q -c "DROP EXTENSION IF EXISTS pg_ripple CASCADE;"
psql -d "$DB" -q -c "CREATE EXTENSION pg_ripple;"

# ── Insert throughput: 100K triples ───────────────────────────────────────────
# Uses 100K triples (not 1M) to keep CI runtime reasonable (~30s max).
echo "Phase 1: Insert throughput (100,000 triples)..."

INSERT_RESULT=$(psql -d "$DB" -t -A -q <<'SQL'
DO $$
DECLARE
    pred      INT;
    batch     INT;
    nt        TEXT;
    i         INT;
    batch_sz  INT := 5000;
    total     BIGINT := 0;
    t_start   TIMESTAMPTZ;
    t_end     TIMESTAMPTZ;
    elapsed   NUMERIC;
    throughput NUMERIC;
BEGIN
    t_start := clock_timestamp();

    -- 10 predicates × 10,000 subjects = 100,000 triples
    FOR pred IN 1..10 LOOP
        FOR batch IN 0..1 LOOP
            nt := '';
            FOR i IN (batch * batch_sz + 1)..(batch * batch_sz + batch_sz) LOOP
                nt := nt ||
                    '<http://ci-bench.test/S' || i || '> ' ||
                    '<http://ci-bench.test/P' || pred || '> ' ||
                    '<http://ci-bench.test/O' || pred || '_' || i || '> .' || E'\n';
            END LOOP;
            PERFORM pg_ripple.load_ntriples(nt);
            total := total + batch_sz;
        END LOOP;
    END LOOP;

    t_end := clock_timestamp();
    elapsed := EXTRACT(EPOCH FROM (t_end - t_start));
    throughput := total / elapsed;

    -- Output as tab-separated: total elapsed throughput
    RAISE NOTICE 'BENCH_INSERT|%|%|%', total, round(elapsed, 3), round(throughput, 0);
END $$;
SQL
)

# Parse insert results from NOTICE output
INSERT_LINE=$(echo "$INSERT_RESULT" | grep 'BENCH_INSERT' | head -1 | sed 's/.*BENCH_INSERT|//')
INSERT_TOTAL=$(echo "$INSERT_LINE" | cut -d'|' -f1)
INSERT_ELAPSED=$(echo "$INSERT_LINE" | cut -d'|' -f2)
INSERT_THROUGHPUT=$(echo "$INSERT_LINE" | cut -d'|' -f3)

echo "  Loaded: ${INSERT_TOTAL} triples"
echo "  Elapsed: ${INSERT_ELAPSED}s"
echo "  Throughput: ${INSERT_THROUGHPUT} triples/sec"
echo ""

# ── Point-query latency ──────────────────────────────────────────────────────
echo "Phase 2: Point-query latency..."

POINT_RESULT=$(psql -d "$DB" -t -A -q <<'SQL'
DO $$
DECLARE
    t_start   TIMESTAMPTZ;
    t_end     TIMESTAMPTZ;
    elapsed   NUMERIC;
    dummy     BIGINT;
    i         INT;
    n_iter    INT := 100;
BEGIN
    -- Warm up
    FOR i IN 1..10 LOOP
        SELECT count(*) INTO dummy FROM pg_ripple.find_triples(
            '<http://ci-bench.test/S' || i || '>', '<http://ci-bench.test/P1>', NULL
        );
    END LOOP;

    -- Measure: 100 point queries
    t_start := clock_timestamp();
    FOR i IN 1..n_iter LOOP
        SELECT count(*) INTO dummy FROM pg_ripple.find_triples(
            '<http://ci-bench.test/S' || ((i * 7) % 10000 + 1) || '>',
            '<http://ci-bench.test/P' || ((i % 10) + 1) || '>',
            NULL
        );
    END LOOP;
    t_end := clock_timestamp();
    elapsed := EXTRACT(EPOCH FROM (t_end - t_start)) * 1000.0 / n_iter;

    RAISE NOTICE 'BENCH_POINT|%', round(elapsed, 3);
END $$;
SQL
)

POINT_LATENCY=$(echo "$POINT_RESULT" | grep 'BENCH_POINT' | head -1 | sed 's/.*BENCH_POINT|//')
echo "  Avg point-query latency: ${POINT_LATENCY}ms"
echo ""

# ── SPARQL query latency ─────────────────────────────────────────────────────
echo "Phase 3: SPARQL BGP query latency..."

SPARQL_RESULT=$(psql -d "$DB" -t -A -q <<'SQL'
DO $$
DECLARE
    t_start   TIMESTAMPTZ;
    t_end     TIMESTAMPTZ;
    elapsed   NUMERIC;
    dummy     BIGINT;
    i         INT;
    n_iter    INT := 50;
BEGIN
    -- Warm up
    FOR i IN 1..5 LOOP
        SELECT count(*) INTO dummy FROM pg_ripple.sparql(
            'SELECT ?o WHERE { <http://ci-bench.test/S' || i || '> <http://ci-bench.test/P1> ?o . }'
        );
    END LOOP;

    -- Measure: 50 SPARQL BGP queries
    t_start := clock_timestamp();
    FOR i IN 1..n_iter LOOP
        SELECT count(*) INTO dummy FROM pg_ripple.sparql(
            'SELECT ?o WHERE { <http://ci-bench.test/S' || ((i * 13) % 10000 + 1) || '> <http://ci-bench.test/P' || ((i % 10) + 1) || '> ?o . }'
        );
    END LOOP;
    t_end := clock_timestamp();
    elapsed := EXTRACT(EPOCH FROM (t_end - t_start)) * 1000.0 / n_iter;

    RAISE NOTICE 'BENCH_SPARQL|%', round(elapsed, 3);
END $$;
SQL
)

SPARQL_LATENCY=$(echo "$SPARQL_RESULT" | grep 'BENCH_SPARQL' | head -1 | sed 's/.*BENCH_SPARQL|//')
echo "  Avg SPARQL BGP latency: ${SPARQL_LATENCY}ms"
echo ""

# ── Write results JSON ───────────────────────────────────────────────────────
cat > "$RESULT_FILE" <<EOF
{
  "version": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "insert_throughput_triples_per_sec": ${INSERT_THROUGHPUT},
  "insert_total_triples": ${INSERT_TOTAL},
  "insert_elapsed_sec": ${INSERT_ELAPSED},
  "point_query_latency_ms": ${POINT_LATENCY},
  "sparql_bgp_latency_ms": ${SPARQL_LATENCY}
}
EOF

echo "Results written to: $RESULT_FILE"
cat "$RESULT_FILE"
echo ""

# ── Regression check ─────────────────────────────────────────────────────────
if [[ -n "$BASELINE" && -f "$BASELINE" ]]; then
    echo "=== Regression Check (threshold: ${THRESHOLD}%) ==="

    BASELINE_THROUGHPUT=$(python3 -c "
import json, sys
with open('$BASELINE') as f:
    b = json.load(f)
print(b.get('insert_throughput_triples_per_sec', 0))
")

    if [[ "$BASELINE_THROUGHPUT" != "0" ]]; then
        REGRESSED=$(python3 -c "
baseline = float($BASELINE_THROUGHPUT)
current = float($INSERT_THROUGHPUT)
threshold = float($THRESHOLD)
pct_change = ((baseline - current) / baseline) * 100
print(f'Change: {pct_change:+.1f}%')
if pct_change > threshold:
    print(f'FAIL: throughput regressed by {pct_change:.1f}% (threshold: {threshold}%)')
    exit(1)
else:
    print(f'PASS: within {threshold}% threshold')
    exit(0)
")
        RC=$?
        echo "$REGRESSED"
        if [[ $RC -ne 0 ]]; then
            echo ""
            echo "PERFORMANCE REGRESSION DETECTED"
            exit 1
        fi
    else
        echo "Baseline throughput is 0; skipping regression check."
    fi
else
    echo "No baseline file provided; skipping regression check."
    echo "To enable: set BASELINE_FILE=path/to/baseline.json"
fi

echo ""
echo "=== CI Benchmark complete ==="
