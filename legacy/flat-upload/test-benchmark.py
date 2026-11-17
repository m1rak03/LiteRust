"""Synthetic aggregation regression only; not browser performance data."""
import contextlib
import csv
import importlib.util
import io
from pathlib import Path
import tempfile

spec = importlib.util.spec_from_file_location("summary", Path(__file__).with_name("summarize-benchmark.py"))
summary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(summary)
with tempfile.TemporaryDirectory() as root:
    path = Path(root) / "synthetic.csv"
    with path.open("w", newline="", encoding="utf-8") as target:
        writer = csv.DictWriter(target, fieldnames=["Scenario", "Browser", "Repeat", "PrivateMiB", "SumWorkingSetMiB", "CpuPercent"])
        writer.writeheader()
        for repeat, values in enumerate(([1, 3, 100], [10, 20, 90], [25, 30, 35]), 1):
            for value in values:
                writer.writerow(dict(Scenario="synthetic", Browser="LiteRust", Repeat=repeat,
                                     PrivateMiB=value, SumWorkingSetMiB=value * 2, CpuPercent=""))
    output = io.StringIO()
    with contextlib.redirect_stdout(output):
        summary.summarize([path])
    assert "synthetic | LiteRust | 3 | 20.00 | 40.00 | N/A | >=3 independent repeats" in output.getvalue()
print("PASS: median of independent-run medians; missing CPU remains N/A. Synthetic data only.")
