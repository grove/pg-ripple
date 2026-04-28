"""Regression gate: fail if any benchmark metric drops >10% vs rolling 4-week avg.

Used by .github/workflows/performance_trend.yml.
"""
import sys
import pathlib
import statistics

REGRESS_THRESHOLD = 0.10  # 10% drop triggers failure
WARMUP_WEEKS = 4          # minimum history weeks before gate is active
failed = False


def check_regression(name: str, csv_path: str, value_col: int) -> None:
    global failed
    hist = pathlib.Path(csv_path)
    if not hist.exists():
        print(f"SKIP {name}: no history file yet")
        return
    rows = []
    for line in hist.read_text().splitlines()[1:]:  # skip header
        parts = line.strip().split(",")
        if len(parts) > value_col:
            try:
                rows.append(float(parts[value_col]))
            except ValueError:
                pass
    if len(rows) < WARMUP_WEEKS + 1:
        print(f"SKIP {name}: only {len(rows)} data points (need ≥{WARMUP_WEEKS+1})")
        return
    recent = rows[-1]
    baseline = statistics.mean(rows[-WARMUP_WEEKS - 1:-1])
    ratio = recent / baseline if baseline > 0 else 1.0
    drop = 1.0 - ratio
    if drop > REGRESS_THRESHOLD:
        print(f"FAIL {name}: {recent:.1f} is {drop*100:.1f}% below 4-week avg {baseline:.1f}")
        failed = True
    else:
        print(f"PASS {name}: {recent:.1f} vs 4-week avg {baseline:.1f} ({ratio*100:.1f}%)")


check_regression("insert_throughput", "benchmarks/insert_throughput_history.csv", 2)
check_regression("merge_throughput(workers=1)", "benchmarks/merge_throughput_history.csv", 3)
check_regression("hybrid_search", "benchmarks/hybrid_search_history.csv", 2)

sys.exit(1 if failed else 0)
