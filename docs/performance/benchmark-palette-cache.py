#!/usr/bin/env python3
"""Alternate identical palette benchmarks and compare complete bit snapshots.

Build benchmark_palette from this revision against both production revisions.
The benchmark times palette generation/analysis only; decoding, hashing and
observer checks are outside the timer. This driver adds no Python dependencies.
"""
from __future__ import annotations

import argparse
import filecmp
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import time


def digest_file(path: Path) -> str:
    result = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def cpu_name() -> str:
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True, help="baseline benchmark_palette executable")
    parser.add_argument("--candidate", type=Path, required=True, help="candidate benchmark_palette executable")
    parser.add_argument("--output", type=Path, required=True, help="JSON report including raw timing samples")
    parser.add_argument("--compiler", required=True, help="Rust compiler version used to build both executables")
    parser.add_argument("--rounds", type=int, default=9, help="one sample per case/revision in each alternating round")
    parser.add_argument("--warmup", type=int, default=1, help="untimed warmups per case in every timed process")
    parser.add_argument("--cpu", type=int, help="pin child processes to this logical CPU (Linux)")
    parser.add_argument("--case", default="", help="substring filter, forwarded to both executables")
    parser.add_argument("--operation", choices=["generate", "analyze"], default="generate")
    parser.add_argument("--count", type=int, default=12, help="generated palette size and observer-check palette size")
    parser.add_argument("--fixtures", type=Path, default=Path(__file__).resolve().parents[2] / "test" / "fixtures")
    parser.add_argument("--baseline-ref", default="", help="production source revision used by the baseline executable")
    parser.add_argument("--candidate-ref", default="", help="production source revision used by the candidate executable")
    parser.add_argument("--verify-only", action="store_true", help="compare full outputs/observers without recording performance samples")
    args = parser.parse_args()
    require(args.rounds > 0 and args.warmup >= 0 and args.count > 0, "invalid rounds, warmup or count")
    binaries = {"baseline": args.baseline.resolve(), "candidate": args.candidate.resolve()}
    require(binaries["baseline"] != binaries["candidate"], "baseline and candidate must be different executable paths")
    for name, path in binaries.items():
        require(path.is_file() and os.access(path, os.X_OK), f"{name} is not an executable: {path}")
    require(args.fixtures.is_dir(), f"fixtures directory does not exist: {args.fixtures}")
    if args.cpu is not None:
        require(hasattr(os, "sched_setaffinity") and args.cpu in os.sched_getaffinity(0), "requested CPU is unavailable")
    started = time.monotonic()
    child_env = os.environ.copy()
    child_env["OPENVECTOR_FIXTURES"] = str(args.fixtures.resolve())

    def execute(name: str, *, warmup: int, snapshot: Path | None = None) -> dict:
        command = [str(binaries[name]), "--samples=1", f"--warmup={warmup}",
                   f"--operation={args.operation}", f"--count={args.count}", f"--case={args.case}"]
        if snapshot is not None:
            command.extend(["--observers", f"--snapshots={snapshot}"])
        pin = None if args.cpu is None else lambda: os.sched_setaffinity(0, {args.cpu})
        result = subprocess.run(command, check=True, capture_output=True, text=True, env=child_env, preexec_fn=pin)
        report = json.loads(result.stdout)
        require(report["schema"] == 1 and report["rng_seed"] == 1, "unexpected benchmark schema or seed")
        require(report["operation"] == args.operation and report["count"] == args.count, "benchmark controls differ")
        ids = [case["id"] for case in report["cases"]]
        require(bool(ids) and len(ids) == len(set(ids)), "empty or duplicated benchmark case IDs")
        return report

    def compare(reference: dict, actual: dict, observers: bool = False) -> None:
        require([c["id"] for c in reference["cases"]] == [c["id"] for c in actual["cases"]], "benchmark case list changed")
        fields = ["controls", "input_sha256", "output_sha256"]
        if observers:
            fields.append("observer_sha256")
        for expected, current in zip(reference["cases"], actual["cases"], strict=True):
            for field in fields:
                require(expected[field] == current[field], f"{expected['id']}: {field} differs")
            if observers:
                require(bool(current["observer_sha256"]), f"{current['id']}: missing observer fingerprint")

    # These extra calls deliberately precede measurement. Compare full
    # integer/float-bit JSON bytes, not only hashes, including cancellation state.
    print("Checking complete outputs, progress and cancellation...", file=sys.stderr, flush=True)
    with tempfile.TemporaryDirectory(prefix="palette-cache-") as temporary:
        snapshots = {name: Path(temporary) / f"{name}.json" for name in binaries}
        initial = {name: execute(name, warmup=0, snapshot=snapshots[name]) for name in binaries}
        compare(initial["baseline"], initial["candidate"], observers=True)
        require(filecmp.cmp(snapshots["baseline"], snapshots["candidate"], shallow=False), "full snapshots differ")
        verification = {"exact_full_snapshots_equal": True,
                        "snapshot_bytes": snapshots["baseline"].stat().st_size,
                        "snapshot_sha256": digest_file(snapshots["baseline"]),
                        "includes": ["palette_f32_bits", "assignment_indices", "weights_f64_bits", "pass_count",
                                     "random_state", "iteration_observers", "mid_callback_cancellation_prefix_and_state"],
                        "cases": [{key: case[key] for key in ["id", "controls", "input_sha256", "output_sha256", "observer_sha256"]}
                                  for case in initial["baseline"]["cases"]]}
        if args.operation == "analyze":
            verification["includes"].extend(["candidate_score_f32_bits", "feature_f64_bits", "peak_weight_f64_bits", "classification"])

    timings = {name: {case["id"]: [] for case in initial[name]["cases"]} for name in binaries}
    orders = []
    if not args.verify_only:
        for round_index in range(args.rounds):
            order = ["baseline", "candidate"] if round_index % 2 == 0 else ["candidate", "baseline"]
            orders.append(order)
            print(f"Round {round_index + 1}/{args.rounds}: {' then '.join(order)}", file=sys.stderr, flush=True)
            for name in order:
                report = execute(name, warmup=args.warmup)
                compare(initial[name], report)
                for case in report["cases"]:
                    require(len(case["samples_ms"]) == 1, "expected one measured sample per case/process")
                    timings[name][case["id"]].extend(case["samples_ms"])

    summaries = []
    if not args.verify_only:
        for case in initial["baseline"]["cases"]:
            before = timings["baseline"][case["id"]]
            after = timings["candidate"][case["id"]]
            before_median, after_median = statistics.median(before), statistics.median(after)
            summaries.append({"id": case["id"], "baseline_ms": before, "candidate_ms": after,
                              "baseline_median_ms": before_median, "candidate_median_ms": after_median,
                              "speedup": before_median / after_median,
                              "time_reduction_percent": (1.0 - after_median / before_median) * 100.0})

    output = {"schema": 1, "benchmark": "palette-assignment-cache", "operation": args.operation,
              "generated_color_count": args.count if args.operation == "generate" else None,
              "observer_color_count": args.count, "rng_seed": 1,
              "baseline_ref": args.baseline_ref, "candidate_ref": args.candidate_ref,
              "binaries": {name: {"path": str(path), "sha256": digest_file(path)} for name, path in binaries.items()},
              "environment": {"platform": platform.platform(), "cpu": cpu_name(), "logical_cpu": args.cpu,
                              "python": platform.python_version(), "compiler": args.compiler},
              "method": {"rounds": len(orders), "alternating_orders": orders,
                         "warmup_per_case_per_timed_process": args.warmup,
                         "initial_untimed_correctness_call_per_case_per_process": 1,
                         "observer_checks_inside_timed_region": False,
                         "statistic": "median of per-case single-stage elapsed milliseconds"},
              "verification": verification, "results": summaries,
              "wall_seconds_including_validation": time.monotonic() - started}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2) + "\n")
    print(f"Verified {len(verification['cases'])} cases; wrote {args.output}", file=sys.stderr)
    for row in summaries:
        print(f"{row['id']}: {row['baseline_median_ms']:.4f} -> {row['candidate_median_ms']:.4f} ms ({row['speedup']:.2f}x)")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            print(error.stderr, file=sys.stderr)
        raise SystemExit(1)
