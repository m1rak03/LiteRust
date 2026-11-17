"""Aggregate independent Windows runs. Usage: python tools/summarize-benchmark.py runs/*.csv"""
import csv
import statistics
import sys
from collections import defaultdict


def summarize(paths):
    runs = defaultdict(lambda: defaultdict(list))
    for path in paths:
        with open(path, encoding="utf-8-sig", newline="") as source:
            for row in csv.DictReader(source):
                key = row["Scenario"], row["Browser"], row["Repeat"]
                for metric in ("PrivateMiB", "SumWorkingSetMiB", "CpuPercent"):
                    if row.get(metric):
                        runs[key][metric].append(float(row[metric]))
    scenarios = defaultdict(lambda: defaultdict(list))
    for (scenario, browser, _repeat), metrics in runs.items():
        for metric, values in metrics.items():
            scenarios[scenario, browser][metric].append(statistics.median(values))
    print("Scenario | Browser | Runs | median private MiB | median summed WS MiB | CPU % | Status")
    for (scenario, browser), metrics in sorted(scenarios.items()):
        count = len(metrics["PrivateMiB"])
        values = [f"{statistics.median(metrics[m]):.2f}" if metrics[m] else "N/A"
                  for m in ("PrivateMiB", "SumWorkingSetMiB", "CpuPercent")]
        print(" | ".join([scenario, browser, str(count), *values,
                          ">=3 independent repeats" if count >= 3 else "INSUFFICIENT REPEATS"]))
    print("Private bytes are committed memory; summed working sets double-count shared pages. Neither is unique RAM.")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit("Pass real benchmark CSV files; no measurements are bundled.")
    summarize(sys.argv[1:])
