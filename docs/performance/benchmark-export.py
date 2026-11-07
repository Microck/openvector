#!/usr/bin/env python3
"""Alternate baseline/candidate export samples and verify complete output bytes.

Build benchmark_export from identical example source against each revision first.
Run without concurrent builds or benchmarks; optional `taskset -c N` is inherited.
Only the Rust export call is timed. This runner records every raw timing, output
digest, workload size and the CPU affinity used to obtain the paired results.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--fixtures", default="alpha-profile-3,ring-profile-6")
    parser.add_argument("--tiles", type=int, default=8)
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--formats", default="decimal,svg,pdf,eps,ai,dxf-splines,dxf-dense,emf")
    parser.add_argument("--base-ref", default="0efa9aad869e6b58cd7947f3172f44b8a6144215")
    parser.add_argument("--candidate-ref", required=True)
    parser.add_argument("--compiler", help="exact rustc --version used for both binaries; discovered from PATH by default")
    args = parser.parse_args()
    if args.samples < 1:
        parser.error("--samples must be positive")
    args.before = args.before.resolve(strict=True)
    args.after = args.after.resolve(strict=True)
    if args.before == args.after:
        parser.error("--before and --after must name different binaries")
    binary_hashes = {name: hashlib.sha256(path.read_bytes()).hexdigest()
                     for name, path in [("before", args.before), ("after", args.after)]}
    compiler = args.compiler or subprocess.check_output(["rustc", "--version"], text=True).strip()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    results = []
    with tempfile.TemporaryDirectory(prefix="export-equality-", dir=args.output.parent) as directory:
        for fixture in args.fixtures.split(","):
            runs = {"before": [], "after": []}
            for sample in range(args.samples):
                # The first baseline creates the byte references. Subsequent
                # pairs alternate order to avoid assigning drift to one side.
                for side in (["before", "after"] if sample % 2 == 0 else ["after", "before"]):
                    binary = args.before if side == "before" else args.after
                    command = [str(binary), "--fixture", fixture, "--tiles", str(args.tiles),
                               "--samples", "1", "--formats", args.formats,
                               "--dump-dir" if side == "before" else "--check-dir", directory]
                    completed = subprocess.run(command, check=True, text=True, capture_output=True)
                    rows = [json.loads(line) for line in completed.stdout.splitlines()]
                    expected = args.formats.split(",")
                    assert [row["format"] for row in rows] == expected, completed.stdout
                    runs[side].append({row["format"]: row for row in rows})
            for format_name in args.formats.split(","):
                before = [run[format_name] for run in runs["before"]]
                after = [run[format_name] for run in runs["after"]]
                reference = before[0]
                for row in before + after:
                    for key in ["fixture", "tiles", "nodes", "regions", "cubics", "coordinates", "bytes", "sha256"]:
                        assert row[key] == reference[key], (key, reference, row)
                before_ms = [row["median_ms"] for row in before]
                after_ms = [row["median_ms"] for row in after]
                before_median = statistics.median(before_ms)
                after_median = statistics.median(after_ms)
                result = {key: reference[key] for key in ["fixture", "tiles", "format", "nodes", "regions", "cubics", "coordinates", "bytes", "sha256"]}
                result.update(before_ms=before_ms, after_ms=after_ms,
                              before_median_ms=before_median, after_median_ms=after_median,
                              speedup=before_median / after_median,
                              percent_less_time=100 * (1 - after_median / before_median),
                              exact_bytes=True)
                results.append(result)
                print(f"{fixture:20} {format_name:12} {before_median:10.3f} -> {after_median:10.3f} ms  {result['speedup']:6.2f}x", flush=True)
    cpu_model = platform.processor()
    if Path("/proc/cpuinfo").exists():
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                cpu_model = line.split(":", 1)[1].strip()
                break
    output = {"base_ref": args.base_ref, "candidate_ref": args.candidate_ref, "compiler": compiler,
              "platform": platform.platform(), "cpu_model": cpu_model,
              "cpu_affinity": sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
              "before_binary": str(args.before), "after_binary": str(args.after),
              "binary_sha256": binary_hashes,
              "samples_per_side": args.samples, "warmups_per_invocation": 1,
              "method": "alternating paired invocations; fixture parsing, graph tiling, byte comparisons, hashes and I/O excluded from timing",
              "results": results}
    args.output.write_text(json.dumps(output, indent=2) + "\n")


if __name__ == "__main__":
    main()
