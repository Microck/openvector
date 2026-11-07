#!/usr/bin/env python3
"""Compare identical benchmark_noise examples built against two revisions.

Only the Rust noise-reduction call is timed. Runs alternate A/B and B/A order;
every sample must have identical inputs, output bytes, dimensions and amount.
No third-party Python packages are needed.
"""

import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import statistics
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--warmup", type=int, default=1)
    parser.add_argument("--cpu", type=int)
    parser.add_argument("--case", default="")
    parser.add_argument("--before-ref", required=True)
    parser.add_argument("--after-ref", required=True)
    parser.add_argument("--compiler", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.samples < 1 or args.warmup < 0:
        parser.error("samples must be positive and warmup nonnegative")
    binaries = {key: getattr(args, key).resolve() for key in ["before", "after"]}
    if binaries["before"] == binaries["after"]:
        parser.error("before and after must be different executables")
    if args.cpu is not None:
        if not hasattr(os, "sched_setaffinity") or args.cpu not in os.sched_getaffinity(0):
            parser.error("CPU affinity is unavailable or the requested CPU is not allowed")
        os.sched_setaffinity(0, {args.cpu})
    identities, timings, orders = None, {"before": {}, "after": {}}, []
    started = time.time()
    for sample in range(args.samples):
        order = ["before", "after"] if sample % 2 == 0 else ["after", "before"]
        orders.append(order)
        for name in order:
            completed = subprocess.run([str(binaries[name]), f"--warmup={args.warmup}", f"--case={args.case}"],
                text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
            report = json.loads(completed.stdout)
            assert report["profile"] == "release", "benchmark must be a release build"
            identity = [{k: v for k, v in row.items() if k != "milliseconds"} for row in report["cases"]]
            if identities is None:
                identities = identity
            assert identity == identities, f"exact input/output comparison failed: {name}, sample {sample}"
            for row in report["cases"]:
                assert row["milliseconds"] > 0
                timings[name].setdefault(row["id"], []).append(row["milliseconds"])
        print(f"Completed paired sample {sample + 1}/{args.samples}", flush=True)
    rows = []
    for identity in identities:
        row = dict(identity)
        for name in binaries:
            values = timings[name][row["id"]]
            row[name] = {"samples_ms": values, "median_ms": statistics.median(values)}
        row["speedup"] = row["before"]["median_ms"] / row["after"]["median_ms"]
        rows.append(row)
        print(f"{row['id']}: {row['before']['median_ms']:.3f} -> {row['after']['median_ms']:.3f} ms ({row['speedup']:.2f}x)")
    cpu = platform.processor()
    if Path("/proc/cpuinfo").exists():
        cpu = next((line.partition(":")[2].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
            if line.startswith("model name")), cpu)
    result = {"schema_version": 1, "timed_operation": report["timed_operation"],
        "fixture_setup_and_hashing_excluded": True, "all_exact_checks_passed": True,
        "samples": args.samples, "warmup_batches_per_case_per_process": args.warmup,
        "untimed_correctness_runs_per_case_per_process": 1, "sample_orders": orders,
        "case_filter": args.case, "elapsed_seconds": time.time() - started,
        "platform": platform.platform(), "cpu": cpu, "compiler": args.compiler,
        "affinity": sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
        "binaries": {name: {"sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "source_ref": getattr(args, name + "_ref")} for name, binary in binaries.items()}, "cases": rows}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
