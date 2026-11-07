# Exact fitting with less allocation and repeated arithmetic

## Change

The fitting stage makes two changes:

1. `select_intervals` keeps the threshold multiplier for one fitting run. Every
   interval in that run uses the same thresholds, iteration count and fraction,
   so the expensive extended-precision logarithm/exponential calculation only
   needs to run once. The value is computed lazily at its original first-use
   point. The cache is discarded when the run returns or is cancelled. Direct
   calls to the public `partition_contour_interval` still compute their own
   multiplier.
2. Interior-control fitting accumulates its normal equations directly into
   fixed-size stack arrays, specialized for the existing 2- and 4-variable
   systems. It no longer allocates the design rows, residual vector, nested
   matrix or right-hand-side vector and then scans the design repeatedly.

Each matrix and right-hand-side accumulator receives exactly the original
sequence of products and additions, including zero products. Independent sums
are interleaved; their terms are not regrouped. The diagonal regularizer,
Gaussian-elimination pivot choices and arithmetic order, curve evaluation,
keeper decisions, and threshold updates are unchanged. Keeping zero products
also preserves the original behavior when extreme finite coordinates produce
non-finite intermediate arithmetic.

The cache belongs to the current run, so changing settings or retrying a
cancelled fitter cannot reuse an older schedule. Zero iterations never evaluate
the multiplier. Validation and progress checkpoints keep their original order.

## Measured results

Seven alternating paired release samples, 20 fitting calls per sample, on AMD EPYC 9V74, Linux, CPU 0, using `rustc 1.99.1 (58cf8f9c3 2026-10-13)`. Baseline: `0efa9aad869e6b58cd7947f3172f44b8a6144215`; candidate: `5867e029d36ef0b6740eecd68bf1112048441d55`.

The 11 recorded artwork/photo workloads improve 1.54–4.04×. The smallest pipeline cases change little: the 0.023 ms split/profile-9 case measures 0.98×. All 28 final output fingerprints, 251 progress snapshots and 84 cancellation/retry checks match.

| Workload | Before, ms | After, ms | Speedup |
| --- | ---: | ---: | ---: |
| Recorded AA logo, 256×192 | 5.986 | 3.442 | 1.74× |
| Recorded palette-rich image, 192×128 | 14.524 | 8.584 | 1.69× |
| Recorded fox photo, 128×85 | 5.129 | 1.680 | 3.05× |
| Recorded Jim Devine photo, 103×128 | 7.772 | 2.199 | 3.53× |
| Authored 128 short contours | 15.157 | 2.813 | 5.39× |
| Authored 32 longer contours | 11.990 | 7.843 | 1.53× |

Final measurements ran after other project builds, tests and benchmarks stopped. Shared-host timing still varies; [fitting.json](fitting.json) includes all raw samples and ranges. Results measure the fitting stage, including its normal validation, allocation and output destruction.

## Workloads and measured operation

The benchmark times the public `fit_contours` operation on a completed geometry.
This includes its normal validation, owned geometry creation, fitting work,
output construction, allocation and output destruction. Fixture decoding,
reference comparisons, hashing, and observer/cancellation checks happen outside
the timer. Timed calls do not attach a progress observer.

| Workload family | Cases | Source |
| --- | ---: | --- |
| Small native pipeline geometries | 15 | `pipeline-fitting-reference.json`, profiles 3, 6 and 9 |
| Recorded artwork and transparent images | 8 | `fresh-representative-20261020.ndjson.gz` |
| Recorded photographic images | 3 | `fresh-photographic-20261020.ndjson.gz` |
| Authored short intervals | 1 | 128 contours of 16 samples, with a marked node every four samples |
| Authored longer intervals | 1 | 32 contours of 128 samples, with one marked node per contour |

The recorded cases reuse the captured contour coordinates and decode each
fitting parameter from its exact recorded little-endian bits and declared type.
Captured node kinds already reflect completed fitting; `ContourFitter::run`
normalizes those kinds using its existing repeated-run behavior. The resulting
geometry is checked against the recorded native fitted geometry before timing.
No random-number generation or re-smoothing is involved in those inputs.

The two authored cases use exact binary coordinates and are revision-to-revision
checks. They have no recorded native output oracle.

These measurements cover fitting only. They do not measure image decoding,
segmentation, smoothing, export, or full conversion latency.

## Reproduction

Build the identical benchmark example at the baseline and candidate revisions
with the same compiler and release profile. From the candidate checkout:

```sh
git worktree add --detach ../openvector-before 0efa9aad869e6b58cd7947f3172f44b8a6144215
mkdir -p ../openvector-before/rust/crates/openvector/examples
cp rust/crates/openvector/examples/benchmark_fitting.rs ../openvector-before/rust/crates/openvector/examples/
cargo build --locked --release --manifest-path ../openvector-before/rust/Cargo.toml -p openvector --example benchmark_fitting
cargo build --locked --release --manifest-path rust/Cargo.toml -p openvector --example benchmark_fitting
python3 docs/performance/benchmark-fitting.py \
  --before ../openvector-before/rust/target/release/examples/benchmark_fitting \
  --after rust/target/release/examples/benchmark_fitting \
  --output docs/performance/fitting.json \
  --pairs 7 --iterations 20 --cpu 0 \
  --before-ref 0efa9aad869e6b58cd7947f3172f44b8a6144215 \
  --after-ref 5867e029d36ef0b6740eecd68bf1112048441d55 \
  --compiler "$(rustc --version)"
```

`5867e029d36ef0b6740eecd68bf1112048441d55` identifies the candidate source and benchmark revision.
Choose an allowed CPU if CPU 0 is unavailable. On Linux, omitting `--cpu`
selects the first CPU in the current affinity mask. The driver supplies the same
fixture directory to both binaries; `--fixtures PATH` overrides its default of
this checkout's `test/fixtures`.

The driver first runs separate complete-output, progress, cancellation and retry
checks on both binaries. It then collects seven paired samples, alternating
A/B and B/A execution order. Each sample batches 20 fitting calls and reports
elapsed milliseconds divided by 20. Every case in each process has one untimed
reference-validation call followed by one explicit warmup call before timing.
The driver checks case identity, inputs and outputs on every process run.

The report stores all per-call sample values, sample order, median times,
minimum/maximum sample ranges, exactness fingerprints, fixture hashes, compiler,
CPU, source references and executable hashes. Speedup is the baseline median
divided by the candidate median. Run the measurements after builds and other
benchmarks finish; results from a shared host can still vary.

## Correctness checks

- Three solver tests perform 1,484 bitwise comparisons with the original
  materialized-design solver: 576 degenerate/boundary combinations, 12
  non-finite cases, and 896 deterministic varied sample sequences. They cover
  quadratic and cubic fitting, signed zero, subnormal values, large finite
  coordinates and sample counts up to 257.
- A fourth unit test verifies that zero iterations leave the multiplier cache
  empty and preserve the original initial threshold and keeper records.
- Three integration tests compare changing settings and cancellation/retry
  histories with an uncached controller, and check that invalid controls return
  before callbacks without changing retained state. The comparison preserves
  the original inactive nodes' retained cubic slots.
- The benchmark matches all 26 public native/recorded fitted outputs and checks
  all 28 final outputs between revisions. Coordinates and control points are
  fingerprinted from their raw `f64` bits. Other covered fields include node
  kinds and cubic assignments, cubic storage slots, region metadata, contours,
  and fitted edges.
- The complete progress runs compare 251 corresponding event/snapshot pairs,
  including raw progress-fraction bits, cubic availability, retained cubic
  contents and region boundary records.
- Cancellation occurs at the first, middle and last callback of every case:
  84 corresponding cancellation boundaries. Each instance retains the observed
  state, then retries; the retained snapshots, resumed results and resumed
  snapshots match between revisions.

Run the focused tests and the public pipeline regression with:

```sh
cargo test --locked --manifest-path rust/Cargo.toml -p openvector --lib fitting::tests
cargo test --locked --manifest-path rust/Cargo.toml -p openvector --test fitting_cache
cargo test --locked --manifest-path rust/Cargo.toml -p openvector --test fitting complete_fitting_matches_all_15_pipeline_targets
```

The large private fitting-sample, interval-cost, keeper and full-fitting fixture
corpora are absent from this checkout. The checks above use the committed public
fixtures and the independent original-arithmetic oracle.
