# Exact smoothing angle series

## Change

Image-based smoothing repeatedly evaluates contour angles with a 192-bit
fixed-point arctangent series. Extra precision is used for near-horizontal,
subnormal directions. This preserves the original extended-register rounding
before the stored angle and angle difference are rounded separately.

The series previously used signed arbitrary-precision division for both its
fixed-point products and its small odd denominators. It now computes powers
with unsigned magnitudes, shifts products by the fixed-point precision, divides
terms by a `u32` denominator, and restores the argument's sign once at the end.
This avoids general multiword division in the inner loop.

The equivalence is integer-exact. For `U = 2^precision`, the squared argument is
nonnegative and every subsequent power has the argument's sign. Dividing a
signed power by `U` truncates toward zero; shifting its unsigned magnitude and
restoring its sign produces exactly the same integer. Small-denominator division
likewise produces the same terms. The alternating sum, zero-term stopping point,
precision, range reduction, constants, and final floating-point conversions are
unchanged. All production arguments have magnitude at most `U / 2`, keeping the
unsigned accumulator nonnegative and the odd denominators within `u32`.

The production change is confined to `rust/crates/openvector/src/arctangent.rs`.
The CLI, desktop application and Node binding use this Rust engine. The
JavaScript reference remains available as an independent implementation.
This change is independent of the pixel-workspace optimization in
[PR #27](https://github.com/Microck/openvector/pull/27).

## Benchmark

Measurements and exact comparison results are recorded in
[`smoothing-angle-series.json`](smoothing-angle-series.json). The benchmark times
the complete `smooth_contour_geometry` operation, including its initialization
and internal validation. Input decoding, pre-smoothing contour extraction,
reference-output validation, hashing, and observer checks are outside the timer.

The inputs are all 15 checked-in pipeline smoothing cases, eight recorded
artwork inputs, three recorded photographic inputs, and four authored block
grids at 256 x 192 and 512 x 384 pixels with antialiasing enabled and disabled.
Recorded artwork and photographic cases reconstruct raw contours from their
captured segmentation and use an explicit random seed of 1. They establish
cross-revision equivalence, not replay of the recorder's final random state.
The 15 pipeline cases also check their existing native expected results.

Both executables use Rust 1.99.1 (LLVM 23.1.1) and locked release builds on the
same Debian 13 x86-64 host with an AMD EPYC 9V74 processor. The driver pins both
versions to CPU 0 and alternates their order for five paired samples. Each case
has one untimed correctness run and one additional warmup per process. No build
or test runs execute concurrently with the timings. This is a shared host.

The baseline is `0efa9aad869e6b58cd7947f3172f44b8a6144215`; the measured source
change and benchmark are in `10215ba33acd1bd1511d029df271d78c29c2f352`.

## Results

Median milliseconds per complete smoothing call, five paired samples:

| Workload | Before (ms) | After (ms) | Speedup | Time reduction |
| --- | ---: | ---: | ---: | ---: |
| Pipeline: split-profile-3 | 0.084 | 0.083 | 1.01x | 1.3% |
| Pipeline: diagonal-profile-3 | 0.438 | 0.437 | 1.00x | 0.4% |
| Pipeline: ring-profile-3 | 0.323 | 0.321 | 1.01x | 0.7% |
| Pipeline: alpha-profile-3 | 2.136 | 2.239 | 0.95x | -4.9% |
| Pipeline: three-way-profile-3 | 0.244 | 0.258 | 0.95x | -5.7% |
| Pipeline: split-profile-6 | 10.257 | 5.929 | 1.73x | 42.2% |
| Pipeline: diagonal-profile-6 | 223.134 | 92.196 | 2.42x | 58.7% |
| Pipeline: ring-profile-6 | 147.258 | 65.273 | 2.26x | 55.7% |
| Pipeline: alpha-profile-6 | 174.621 | 82.571 | 2.11x | 52.7% |
| Pipeline: three-way-profile-6 | 32.096 | 19.326 | 1.66x | 39.8% |
| Pipeline: split-profile-9 | 0.015 | 0.015 | 1.00x | -0.1% |
| Pipeline: diagonal-profile-9 | 0.372 | 0.359 | 1.04x | 3.7% |
| Pipeline: ring-profile-9 | 0.592 | 0.606 | 0.98x | -2.4% |
| Pipeline: alpha-profile-9 | 0.280 | 0.281 | 1.00x | -0.1% |
| Pipeline: three-way-profile-9 | 0.040 | 0.040 | 1.00x | 0.2% |
| Artwork: existing-15-transparent-31x37 | 357.467 | 160.294 | 2.23x | 55.2% |
| Artwork: existing-08-transparent-19x23 | 157.648 | 64.168 | 2.46x | 59.3% |
| Artwork: logo-aa-256x192 | 3,515.190 | 1,621.600 | 2.17x | 53.9% |
| Artwork: text-aa-256x160 | 3,435.656 | 1,494.866 | 2.30x | 56.5% |
| Artwork: thin-holes-lines-192x128 | 11,690.897 | 5,264.941 | 2.22x | 55.0% |
| Artwork: logo-jpeg-256x192 | 3,101.397 | 1,366.612 | 2.27x | 55.9% |
| Artwork: palette-rich-192x128 | 6,658.563 | 2,864.561 | 2.32x | 57.0% |
| Artwork: alpha-rings-192x128 | 11,835.666 | 4,716.051 | 2.51x | 60.2% |
| Photo: arctic-fox | 36.759 | 37.329 | 0.98x | -1.6% |
| Photo: mojave-desert | 37.535 | 37.648 | 1.00x | -0.3% |
| Photo: jim-devine | 58.904 | 59.917 | 0.98x | -1.7% |
| Authored: blocks-256x192-aa0 | 2.865 | 2.991 | 0.96x | -4.4% |
| Authored: blocks-256x192-aa1 | 1,609.751 | 875.164 | 1.84x | 45.6% |
| Authored: blocks-512x384-aa0 | 5.145 | 5.039 | 1.02x | 2.1% |
| Authored: blocks-512x384-aa1 | 3,503.683 | 1,944.571 | 1.80x | 44.5% |

The eight recorded antialiased artwork cases are 2.17–2.51x faster (53.9–60.2%
less smoothing time). The authored antialiased grids improve by 1.80–1.84x.
Non-antialiased cases, including the three photographic captures, do not use
this image-angle hot path and show no consistent speedup. Their measured ratios
range from 0.95x to 1.04x; all raw samples and ranges remain in the JSON report.

Timing ratios describe these workloads and compiler/host conditions, not a
universal or whole-conversion speedup.

## Exactness checks

- Three arithmetic tests perform 5,635 exact `BigInt` comparisons against the
  original signed-division series, including negative reduced arguments,
  truncation boundaries, small fixed-point grids, deterministic varied ratios,
  and precision up to 2,237 bits.
- All six Rust library unit tests pass.
- The existing full-smoothing regression passes all 15 native pipeline targets,
  including geometry, penalty counts and random state.
- The existing warm smoothing-cancellation regression passes.
- All 33 source smoke tests pass.
- The A/B benchmark fingerprints every final smoothing field, using raw IEEE
  bits for floating-point values. It also compares every progress event and
  snapshot, plus cancellation at the first, middle and final callback and the
  retained random state. These checks are separate from the timed runs.

All 30 baseline/candidate cases match these final, progress and cancellation
fingerprints exactly. Every timed repetition is also checked against its
verified output fingerprint.

The complete private differential corpus is unavailable in this checkout; see
[`test-fixtures.md`](../test-fixtures.md). No missing fixture, existing assertion,
expected result, tolerance, iteration budget or cancellation behavior is changed.
These measurements describe the smoothing stage on this host. They do not
establish a whole-conversion speedup or performance on other architectures.

## Reproduce

Build the same `rust/crates/openvector/examples/benchmark_smoothing.rs` against
the baseline and candidate, using the same Rust toolchain and locked dependencies:

```sh
git worktree add --detach ../openvector-before 0efa9aad869e6b58cd7947f3172f44b8a6144215
mkdir -p ../openvector-before/rust/crates/openvector/examples
cp rust/crates/openvector/examples/benchmark_smoothing.rs \
  ../openvector-before/rust/crates/openvector/examples/
cargo build --manifest-path ../openvector-before/rust/Cargo.toml --locked \
  --release -p openvector --example benchmark_smoothing
cargo build --manifest-path rust/Cargo.toml --locked \
  --release -p openvector --example benchmark_smoothing
python3 docs/performance/benchmark-smoothing-angles.py \
  --before ../openvector-before/rust/target/release/examples/benchmark_smoothing \
  --after rust/target/release/examples/benchmark_smoothing \
  --samples 5 --warmup 1 --cpu 0 \
  --before-ref 0efa9aad869e6b58cd7947f3172f44b8a6144215 \
  --after-ref 10215ba33acd1bd1511d029df271d78c29c2f352 \
  --compiler 'rustc 1.99.1 (58cf8f9c3 2026-10-13)' \
  --output smoothing-comparison.json
```

Choose a CPU within the process's allowed affinity or omit `--cpu` where affinity
is unsupported. The driver checks identical inputs, outputs, progress and
cancellation first, alternates A/B and B/A timing rounds, verifies every timed
result, and records raw samples and executable SHA-256 hashes. `--case` restricts
the workload by ID substring. The example's `--snapshots` option can additionally
emit complete final smoothing results.
