# Cache unchanged pixel energies during smoothing

The image measurement kernel retains pixel colors between evaluations. Only
`color_pixel` changes those colors, and it changes one pixel at a time. Recomputing
BGRA normalization, four residuals and squared error for every unchanged pixel was
therefore unnecessary. Cache each pixel's exact double error and update it whenever
its color changes. Sum the cached errors in the same row-major order, with the same
arithmetic grouping and final multiplication by 0.5. Gradient computation is unchanged.

A failed `color_pixel` can partially change its pixel before returning an error.
That pixel's cache entry is updated before propagating the error, preserving the
original partially committed cache on the next evaluation. The new authored
fragment-error regression passes against both unchanged main and the candidate;
no reference expectation was changed.

This adds **8 bytes per image pixel**, plus vector overhead/capacity slack, to each
prepared image kernel (about 63 MiB at 3840×2160). It trades memory for repeated
objective speed. The cache is initialized on first energy evaluation, not model
construction, and is owned/cloned with the prepared kernel.

## Measurements

This change is independent of PR #27. Its baseline is unchanged main
`0efa9aad869e6b58cd7947f3172f44b8a6144215`, with none of PR #27's touched-pixel
workspace changes. Results cannot be added to PR #27's percentages.

Both builds used identical benchmark sources/dependencies, Rust 1.99.1 / LLVM
23.1.1 and default Cargo release settings (`--locked --offline --release`). Host:
Debian 13 x86_64, AMD EPYC 9V74, shared five-vCPU cloud instance; CPU affinity 0.
Seven paired samples alternated A/B order with no concurrent builds/tests. Values
are wall-clock medians on a shared host, not confidence intervals.

| Workload | Before | After | Time reduction |
| --- | ---: | ---: | ---: |
| Full smoothing, authored scaled ring, 544×416 | 437.626 ms | 249.654 ms | 42.95% |
| Full smoothing, authored scaled ring, 136×104 | 94.472 ms | 83.852 ms | 11.24% |
| Persistent image energy + gradient, 1088×832 | 7.558 ms | 4.303 ms | 43.07% |
| Persistent image energy + gradient, 272×208 | 0.613 ms | 0.419 ms | 31.73% |
| Persistent image energy + gradient, 17×13 | 0.0348 ms | 0.0339 ms | 2.58% |
| First image energy + gradient, 1088×832 | 17.699 ms | 15.035 ms | 15.05% |
| Sum of all 15 original small full-smoothing cases | 563.645 ms | 576.499 ms | **2.28% more time** |

The small original corpus does not demonstrate a speedup; its alpha/profile-6
case used 5.83% more time. All per-case and per-sample timings are in
[the measurement record](smoothing-energy-cache.json). These sparse authored rings
do not establish photographic, dense-contour, GUI, end-to-end conversion or
cross-platform performance. The additional memory and small-case results matter
when deciding whether to adopt the optimization.

The full original benchmark repeats each of its 15 cases ten times per sample;
one untimed warmup and one untimed final call check against the original references.
Full scaled smoothing repeats four times after a warmup and times the entire
smoothing function, including model preparation and first cache initialization.
It uses the checked-in ring/profile-6 input, replicated BGRA/labels, scaled geometry,
region pixel counts and boundary sample counts, and the same initial RNG state.
Input construction is outside the timed region.

The persistent image benchmark repeats 40 evaluations after four warmup coordinate
states. The first-evaluation benchmark instead uses ten fresh model clones per
sample and includes initial workspace/cache allocation in energy evaluation. Model
cloning and gradient allocation are outside that timer. Snapshot construction,
serialization and hashing are outside every timer.

Temporary baseline diagnostic timers measured 93 ms in tracing and 269 ms in the
energy loop, of which only 23 ms was pixel coloring, on 1088×832 across 48 evaluations.
This pointed to residual arithmetic and summation for unchanged pixels. Instrumented
runs are excluded from the A/B results.

## Correctness and reproducibility

All A/B hashes match for complete image measurement snapshots at three sizes,
first-evaluation snapshots, and both full scaled outputs. These include energy,
raw gradients, cached colors, flags, clipped records, invalid regions, completed
geometry, penalties/references/border counts and final RNG state as applicable.
A temporary side-by-side test of the unchanged kernel and candidate also matched
all public measurement fields across 1,920 successful deterministically perturbed
persistent states over all 15 inputs. That probe encountered no error states; the
separate authored regression covers the retained fragment error and its next calls.

Local checks passed: three engine library unit tests; the new error/cache regression;
the existing full-smoothing test comparing all 15 original pipeline outcomes; and
all 33 source smoke checks. Complete private differential suites remain unavailable.
The existing missing-capture/strict-suite limits are not resolved. No existing
fixtures, expectations, optimizer budgets, progress/cancellation controls,
accessibility behavior or release gates were changed.

Copy this PR's two benchmark examples into a separate checkout of the baseline,
then build and save both executables separately with the same compiler/cache:

```sh
cargo build --manifest-path rust/Cargo.toml --locked --offline --release \
  -p openvector --example bench_smoothing --example bench_smoothing_energy
# Copy rust/target/release/examples/bench_smoothing and bench_smoothing_energy
# into BEFORE or AFTER before building the other checkout.
python3 docs/performance/benchmark-smoothing-energy.py BEFORE AFTER \
  --samples 7 --cpu 0 --output smoothing-energy-results.json
```

Choose an allowed CPU and avoid concurrent builds/tests. The driver alternates
order, compares hashes, writes raw samples and reports medians. Individual modes:

```sh
bench_smoothing 10
bench_smoothing_energy 64 40
bench_smoothing_energy 32 4 full
bench_smoothing_energy 64 10 cold
```
