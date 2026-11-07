#!/usr/bin/env python3
"""Compare two release builds of the smoothing examples; never changes fixtures."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("before", type=Path, help="Directory containing baseline benchmark executables")
parser.add_argument("after", type=Path, help="Directory containing candidate benchmark executables")
parser.add_argument("--samples", type=int, default=7)
parser.add_argument("--cpu", type=int, help="Pin this process and its children to one Linux CPU")
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
if args.samples < 1:
    parser.error("--samples must be positive")
if args.cpu is not None:
    os.sched_setaffinity(0, {args.cpu})
rows = []
for sample in range(args.samples):
    versions = ["before", "after"] if sample % 2 == 0 else ["after", "before"]
    for version in versions:
        directory = getattr(args, version).resolve()
        jobs = [("pipeline", "bench_smoothing", ["10"])]
        jobs += [("image", "bench_smoothing_image", [str(scale), "40"]) for scale in (1, 16, 64)]
        jobs += [("full-scaled", "bench_smoothing_image", [str(scale), "4", "full"]) for scale in (8, 32)]
        for kind, binary, options in jobs:
            output = subprocess.check_output([str(directory / binary), *options], text=True)
            for line in output.splitlines():
                fields = line.split("\t")
                row = dict(sample=sample, version=version, kind=kind, case=fields[0], milliseconds=float(fields[2]))
                if kind != "pipeline":
                    row["digest"] = fields[3]
                rows.append(row)
        args.output.write_text(json.dumps(rows, indent=2) + "\n")
        print(f"Completed sample {sample + 1}/{args.samples}: {version}", flush=True)
for kind, case in sorted({(r["kind"], r["case"]) for r in rows}):
    selected = [r for r in rows if r["kind"] == kind and r["case"] == case]
    hashes = {r["digest"] for r in selected if "digest" in r}
    if len(hashes) > 1:
        raise SystemExit(f"Correctness mismatch: {kind} {case}")
    before, after = [statistics.median(r["milliseconds"] for r in selected if r["version"] == v) for v in ("before", "after")]
    print(f"{kind}\t{case}\t{before:.6f}\t{after:.6f}\t{(before-after)/before*100:.2f}%")
