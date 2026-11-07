#!/usr/bin/env python3
"""Compare two release benchmark_smoothing binaries with interleaved samples.

Build the same rust/crates/openvector/examples/benchmark_smoothing.rs against
both revisions first. This driver runs a separate exact output/progress check,
then alternates A/B and B/A order. Each binary times only smoothing internally.
No third-party Python packages are required.

Example:
    python3 docs/performance/benchmark-smoothing-angles.py \\
        --before ../openvector-before/rust/target/release/examples/benchmark_smoothing \\
        --after rust/target/release/examples/benchmark_smoothing \\
        --samples 7 --warmup 2 --output smoothing-comparison.json

Use --case to restrict to an ID substring. --cpu pins all runs on Linux when
requested; otherwise the process inherits the caller's CPU affinity.
"""

import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import statistics
import subprocess
import sys
import time


IDENTITY_FIELDS = (
    "id", "width", "height", "nodes", "regions", "anti_alias", "input_sha256",
    "output_sha256", "random_state", "native_expected_checked",
)


def positive_int(text):
    value = int(text)
    if value <= 0:
        raise argparse.ArgumentTypeError("must be positive")
    return value


def nonnegative_int(text):
    value = int(text)
    if value < 0:
        raise argparse.ArgumentTypeError("must be nonnegative")
    return value


def run(binary, case, warmup, verify):
    command = [str(binary), "--samples=1", f"--warmup={warmup}", f"--case={case}"]
    if verify:
        command.append("--verify-observers")
    completed = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if completed.returncode:
        raise RuntimeError(f"{binary} exited {completed.returncode}:\n{completed.stderr}")
    report = json.loads(completed.stdout)
    if report["profile"] != "release":
        raise RuntimeError(f"{binary} is not a release build")
    return report


def matching_cases(reference, actual, include_observers=False):
    expected = reference["cases"]
    observed = actual["cases"]
    if len(expected) != len(observed):
        raise RuntimeError(f"case count changed: {len(expected)} versus {len(observed)}")
    fields = IDENTITY_FIELDS + (("observer_checks",) if include_observers else ())
    for before, after in zip(expected, observed):
        for key in fields:
            if before[key] != after[key]:
                raise RuntimeError(f"exact comparison failed for {before['id']}: {key}")


def cpu_model():
    path = Path("/proc/cpuinfo")
    if path.exists():
        for line in path.read_text().splitlines():
            if line.startswith("model name"):
                return line.partition(":")[2].strip()
    return platform.processor()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--samples", type=positive_int, default=7)
    parser.add_argument("--warmup", type=nonnegative_int, default=2)
    parser.add_argument("--case", default="", help="benchmark case ID substring")
    parser.add_argument("--cpu", type=nonnegative_int, help="pin to one allowed CPU (Linux)")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--before-ref", help="baseline commit SHA, for report provenance")
    parser.add_argument("--after-ref", help="candidate commit SHA or descriptive working-tree label")
    parser.add_argument("--compiler", help="exact compiler version used for both binaries")
    args = parser.parse_args()
    binaries = {"before": args.before.resolve(), "after": args.after.resolve()}
    for binary in binaries.values():
        if not binary.is_file():
            parser.error(f"binary does not exist: {binary}")
    if binaries["before"] == binaries["after"]:
        parser.error("before and after must identify different binaries")
    if args.cpu is not None:
        if not hasattr(os, "sched_setaffinity"):
            parser.error("--cpu is supported only where os.sched_setaffinity is available")
        if args.cpu not in os.sched_getaffinity(0):
            parser.error(f"CPU {args.cpu} is outside the inherited CPU affinity")
        os.sched_setaffinity(0, {args.cpu})

    started = time.time()
    print("Checking exact inputs, outputs, progress snapshots and cancellations...", file=sys.stderr, flush=True)
    checks = {name: run(binary, args.case, 0, True) for name, binary in binaries.items()}
    matching_cases(checks["before"], checks["after"], include_observers=True)
    samples = {name: {case["id"]: [] for case in checks[name]["cases"]} for name in binaries}
    orders = []
    for sample in range(args.samples):
        order = ["before", "after"] if sample % 2 == 0 else ["after", "before"]
        orders.append(order)
        for name in order:
            print(f"Sample {sample + 1}/{args.samples}: {name}", file=sys.stderr, flush=True)
            report = run(binaries[name], args.case, args.warmup, False)
            matching_cases(checks[name], report)
            for case in report["cases"]:
                values = case["samples_ms"]
                if len(values) != 1 or values[0] <= 0:
                    raise RuntimeError(f"invalid timing sample for {case['id']}")
                samples[name][case["id"]].extend(values)

    rows = []
    for expected in checks["before"]["cases"]:
        identifier = expected["id"]
        row = {key: expected[key] for key in IDENTITY_FIELDS}
        row["observer_checks"] = expected["observer_checks"]
        for name in binaries:
            values = samples[name][identifier]
            row[name] = {"samples_ms": values, "median_ms": statistics.median(values),
                         "min_ms": min(values), "max_ms": max(values)}
        before, after = row["before"]["median_ms"], row["after"]["median_ms"]
        row["speedup"] = before / after
        row["reduction_percent"] = (1 - after / before) * 100
        rows.append(row)
        print(f"{identifier}: {before:.3f} -> {after:.3f} ms ({before / after:.2f}x)", file=sys.stderr)
    metadata = {
        "schema_version": 1,
        "timed_operation": checks["before"]["timed_operation"],
        "fixture_setup_excluded": True,
        "all_exact_checks_passed": True,
        "samples": args.samples,
        "warmup_per_case_per_process": args.warmup,
        "untimed_correctness_runs_per_case_per_process": 1,
        "sample_orders": orders,
        "case_filter": args.case,
        "elapsed_seconds": time.time() - started,
        "platform": platform.platform(),
        "cpu": cpu_model(),
        "affinity": sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
        "python": platform.python_version(),
        "compiler": args.compiler,
        "binaries": {name: {"path": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                             "ref": getattr(args, f"{name}_ref")} for name, binary in binaries.items()},
        "cases": rows,
    }
    args.output.write_text(json.dumps(metadata, indent=2) + "\n")
    print(f"Wrote {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
