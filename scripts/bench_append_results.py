"""Append benchmark results to CSV history files.

Used by .github/workflows/performance_trend.yml.
Reads DATE and WEEK from environment variables (set by the workflow).
"""
import re
import pathlib
import datetime
import os

date_str = os.environ.get("DATE", datetime.date.today().isoformat())
week_str = os.environ.get(
    "WEEK",
    f"{datetime.date.today().isocalendar()[0]}-W{datetime.date.today().isocalendar()[1]:02d}",
)

csv_dir = pathlib.Path("benchmarks")


def extract_tps(raw_path: str) -> float | None:
    """Extract tps from pgbench output."""
    try:
        text = pathlib.Path(raw_path).read_text()
    except FileNotFoundError:
        return None
    m = re.search(r"tps\s*=\s*([\d.]+)", text)
    return float(m.group(1)) if m else None


def extract_workers_throughput(raw_path: str) -> dict[str, int]:
    try:
        text = pathlib.Path(raw_path).read_text()
    except FileNotFoundError:
        return {}
    results = {}
    for m in re.finditer(r"workers=(\d+)\s+throughput=(\d+)", text):
        results[m.group(1)] = int(m.group(2))
    return results


# Insert throughput
insert_tps = extract_tps("insert_throughput_raw.txt")
if insert_tps is not None:
    hist = csv_dir / "insert_throughput_history.csv"
    if not hist.exists():
        hist.write_text("date,week,tps\n")
    with hist.open("a") as f:
        f.write(f"{date_str},{week_str},{insert_tps:.1f}\n")
    print(f"Insert TPS: {insert_tps:.1f}")

# Merge throughput
merge_results = extract_workers_throughput("merge_throughput_raw.txt")
for workers, tp in merge_results.items():
    hist = csv_dir / "merge_throughput_history.csv"
    if not hist.exists():
        hist.write_text("date,week,workers,throughput\n")
    with hist.open("a") as f:
        f.write(f"{date_str},{week_str},{workers},{tp}\n")
    print(f"Merge workers={workers} throughput={tp}")

# Hybrid search
hybrid_tps = extract_tps("hybrid_search_raw.txt")
if hybrid_tps is not None:
    hist = csv_dir / "hybrid_search_history.csv"
    if not hist.exists():
        hist.write_text("date,week,tps\n")
    with hist.open("a") as f:
        f.write(f"{date_str},{week_str},{hybrid_tps:.1f}\n")
    print(f"Hybrid search TPS: {hybrid_tps:.1f}")
