#!/usr/bin/env python3
"""Paired stage-only fitting benchmark; requires two prebuilt example binaries."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(binary, fixtures, iterations, verify=False):
    env = os.environ.copy()
    env['OPENVECTOR_FIXTURES'] = str(fixtures)
    command = [str(binary), '--samples', '1', '--iterations', str(iterations)]
    if verify:
        command.append('--verify')
    return json.loads(subprocess.check_output(command, env=env, text=True))


def compare(before, after, checks):
    left, right = before['cases'], after['cases']
    assert len(left) == len(right), 'case count differs'
    for a, b in zip(left, right):
        keys = ['id', 'nodes', 'regions', 'contour_samples', 'cubic_slots',
                'input_sha256', 'output_sha256', 'native_oracle_sha256']
        if checks:
            keys.append('checks')
        for key in keys:
            assert a[key] == b[key], f"{a['id']}: {key} differs"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, default=Path(__file__).resolve().parents[2] / 'test/fixtures')
    parser.add_argument('--pairs', type=int, default=7)
    parser.add_argument('--iterations', type=int, default=20)
    parser.add_argument('--cpu', type=int)
    parser.add_argument('--before-ref', required=True)
    parser.add_argument('--after-ref', required=True)
    parser.add_argument('--compiler', required=True)
    args = parser.parse_args()
    assert args.pairs > 0 and args.iterations > 0
    before, after = args.before.resolve(), args.after.resolve()
    assert before != after, 'before and after must be different executables'
    fixtures = args.fixtures.resolve()
    allowed = sorted(os.sched_getaffinity(0)) if hasattr(os, 'sched_getaffinity') else []
    cpu = args.cpu if args.cpu is not None else (allowed[0] if allowed else None)
    if cpu is not None:
        assert cpu in allowed, f'CPU {cpu} not in allowed affinity {allowed}'
        os.sched_setaffinity(0, {cpu})
    start = time.time()
    # Separate exhaustive output/progress checks from reported timing samples.
    verified_before = run(before, fixtures, 1, True)
    verified_after = run(after, fixtures, 1, True)
    compare(verified_before, verified_after, True)
    orders = []
    collected = {'before': {}, 'after': {}}
    for pair in range(args.pairs):
        order = [('before', before), ('after', after)]
        if pair % 2:
            order.reverse()
        for name, binary in order:
            result = run(binary, fixtures, args.iterations)
            compare(verified_before, result, False)
            for case in result['cases']:
                collected[name].setdefault(case['id'], []).extend(case['samples_ms'])
        orders.append([name for name, _ in order])
    rows = []
    for case in verified_before['cases']:
        a, b = collected['before'][case['id']], collected['after'][case['id']]
        ma, mb = statistics.median(a), statistics.median(b)
        rows.append({key: value for key, value in case.items() if key not in ['samples_ms', 'iterations']} |
                    {'before_ms': ma, 'after_ms': mb, 'speedup': ma / mb,
                     'time_reduction_percent': (1 - mb / ma) * 100,
                     'before_range_ms': [min(a), max(a)], 'after_range_ms': [min(b), max(b)],
                     'before_samples_ms': a, 'after_samples_ms': b})
    cpu_model = None
    if Path('/proc/cpuinfo').is_file():
        cpu_model = next((line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines()
                          if line.startswith('model name')), None)
    fixture_names = ['pipeline-fitting-reference.json', 'fresh-representative-20261020.ndjson.gz',
                     'fresh-photographic-20261020.ndjson.gz']
    report = {'environment': {'platform': platform.platform(), 'cpu_model': cpu_model, 'cpu': cpu, 'compiler': args.compiler},
              'method': {'pairs': args.pairs, 'iterations_per_sample': args.iterations,
                         'order': 'alternating AB/BA', 'timed_operation': 'fit_contours',
                         'excluded': ['fixture loading', 'reference comparisons', 'hashing', 'progress/cancellation validation'],
                         'warmups': 'one reference-validation call plus one warmup per case per process; verification run is separate'},
              'binaries': {'before': {'source_ref': args.before_ref, 'sha256': digest(before)},
                           'after': {'source_ref': args.after_ref, 'sha256': digest(after)}},
              'fixture_sha256': {name: digest(fixtures / name) for name in fixture_names},
              'all_exact_checks_passed': True, 'sample_orders': orders,
              'cases': rows, 'wall_seconds': time.time() - start}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print('Case | Before ms | After ms | Speedup')
    for row in rows:
        print(f"{row['id']} | {row['before_ms']:.6f} | {row['after_ms']:.6f} | {row['speedup']:.2f}x")
    print(f"Verified {len(rows)} outputs, {sum(c['checks']['progress_events'] for c in rows)} progress snapshots, "
          f"{sum(len(c['checks']['cancellation']) for c in rows)} cancellation/retry cases")
    print(f'Saved {args.output}; total {report["wall_seconds"]:.1f}s')


if __name__ == '__main__':
    main()
