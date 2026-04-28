"""Merge throughput baseline gate (v0.67.0 BENCH-01).

Reads merge_throughput_raw.txt and benchmarks/merge_throughput_baselines.json.
Fails if any worker-count throughput falls below the p95 floor.
"""
import json
import sys
import re
import pathlib

baselines = json.loads(pathlib.Path("benchmarks/merge_throughput_baselines.json").read_text())
raw = pathlib.Path("merge_throughput_raw.txt").read_text()
# parse lines like: workers=1 throughput=500000
results = {}
for line in raw.splitlines():
    m = re.search(r"workers=(\d+)\s+throughput=(\d+)", line)
    if m:
        results[m.group(1)] = int(m.group(2))
if not results:
    print("ERROR: no merge throughput results found — benchmark output could not be parsed", file=sys.stderr)
    sys.exit(1)
failed = False
for workers, measured in results.items():
    b = baselines.get("merge_workers", {}).get(workers)
    if not b:
        continue
    floor = int(b.get("p95", 0))
    if measured < floor:
        print(f"FAIL workers={workers}: {measured} < p95 floor {floor} triples/s")
        failed = True
    else:
        print(f"PASS workers={workers}: {measured} >= p95 floor {floor} triples/s")
sys.exit(1 if failed else 0)
