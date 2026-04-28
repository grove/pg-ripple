"""Vector index recall baseline gate (v0.67.0 BENCH-01).

Reads vector_bench_raw.txt and fails if recall < 0.90.
"""
import re
import sys
import pathlib

raw = pathlib.Path("vector_bench_raw.txt").read_text()
# expect a line like: recall=0.95 build_sec=12.3
recall_m = re.search(r"recall=([\d.]+)", raw)
if not recall_m:
    print("ERROR: no vector recall result found in benchmark output", file=sys.stderr)
    sys.exit(1)
recall = float(recall_m.group(1))
if recall < 0.90:
    print(f"FAIL vector recall {recall:.3f} < 0.90 floor")
    sys.exit(1)
print(f"PASS vector recall {recall:.3f} >= 0.90 floor")
sys.exit(0)
