# Exact native decimal formatting with bounded integers

Baseline source: `0efa9aad869e6b58cd7947f3172f44b8a6144215`.
Validation compiler: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
The paired benchmark report records the exact candidate source ref, compiler,
CPU, affinity, raw samples and output hashes used for its measurements.

## Measured results

Seven alternating paired release samples on AMD EPYC 9V74, Linux, CPU 0, using `rustc 1.99.1 (58cf8f9c3 2026-10-13)`. Candidate source: `fbcbfe060519883351097d2e47220c9be00b4ac7`. Both workloads are 8×8 translated mosaics of the committed fitted native graphs. Values are median milliseconds per call.

| Operation | Alpha/profile-3 before → after, ms | Speedup | Ring/profile-6 before → after, ms | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Coordinate formatting only | 189.047 → 20.112 | 9.40× | 125.983 → 13.236 | 9.52× |
| SVG export | 4.819 → 1.493 | 3.23× | 2.244 → 0.601 | 3.73× |
| PDF export | 4.554 → 2.018 | 2.26× | 2.514 → 0.978 | 2.57× |
| EPS export | 3.958 → 1.788 | 2.21× | 2.523 → 1.023 | 2.47× |
| AI export | 3.815 → 1.683 | 2.27× | 2.471 → 0.957 | 2.58× |
| DXF spline export | 22.759 → 10.272 | 2.22× | 15.311 → 6.249 | 2.45× |
| DXF dense-line export | 66.916 → 28.046 | 2.39× | 34.238 → 14.828 | 2.31× |
| EMF control | 0.470 → 0.444 | 1.06× | 0.256 → 0.195 | 1.32× |

The complete text exports improve 2.21–3.73× across these two workloads. Coordinate formatting in isolation improves 9.40–9.52×. EMF is an unchanged control and measures 1.06–1.32×; the shortest cases remain sensitive to shared-host timing variation. Final measurements ran with other project builds, tests and benchmarks stopped.

All 16 operation/workload outputs match the baseline byte for byte, and every timed output matches its untimed reference. [export-formatting-results.json](export-formatting-results.json) records all raw samples, output hashes, workload sizes, executable hashes, exact source refs and environment metadata. These are formatting/export-stage timings; fitting and all other conversion stages are excluded.

## Change

The native decimal conversion intentionally reproduces 80-bit intermediate
arithmetic and two rounds of decimal rounding. Its hot path previously used
heap-allocated `BigInt` values for each 16-bit limb operation, fraction digit,
and decimal accumulation. Those intermediates are bounded. They now use
`u128` for the binary arithmetic and `u64` for the 17-digit accumulator.

The original omitted product diagonals, normalization, ties-to-even binary
rounding, half-up decimal rounding and 17-digit overflow rule are preserved.
The cold exact power-of-ten calculation still uses `BigInt`. Final field
rounding/padding, arbitrary-length integer fields, stream scaling, special
value labels, coordinate transformations, caches and serializer callbacks
retain their existing implementations.

## Arithmetic bounds

Let `B = 2^16`, with normalized significands `M, N` in `[2^79, 2^80)`.
The native product retains only terms whose limb indices satisfy `a+b >= 4`:

```
P = sum(m[a] * n[b] * B^(a+b-4), a+b >= 4).
```

All terms are nonnegative. Therefore `P <= M*N/B^4 < 2^96`;
every partial sum and individual shifted term also fits. The high-word term
alone is at least `2^94`, so normalization shifts at most once and still
leaves `P < 2^96`. Removing 16 low bits leaves an 80-bit mantissa. Binary
rounding can produce exactly `2^80`; the existing carry branch halves that
value and increments the exponent. This is not a full 160-bit multiplication
followed by truncation: carries from omitted low diagonals must stay omitted.

The existing decimal-exponent selection and scaling normalize the value below
one before digit extraction. The preserved fixed-point conversion appends
`shift = 8 + exponent - 0x3ffe` bits to the normalized mantissa; `shift` is
between zero and eight. Debug assertions check both input mantissa bounds and
this shift invariant. Consequently the starting fraction is below `2^88`.
Each step multiplies by ten (less than `2^92`), takes a decimal digit from the
bits above bit 87, and masks back to 88 bits. Every digit is in `0..=9`.
After 17 digits the decimal accumulator is at most `10^17-1`; the final
rounding increment makes it at most `10^17 < 2^57`, safely inside `u64`.

## Equivalence gates

- All 74,062 original decimal strings: 16,406 at precision two, 28,956 at
  precision three, and 28,700 at precision sixteen. Existing special-value
  and general-format tests also pass.
- A frozen arbitrary-precision implementation checks 10,049 normalized
  mantissa products, including limb carry/rounding patterns and deterministic
  random pairs. It separately checks digit strings and decimal points across
  every normal binary exponent, subnormal boundaries, decimal transitions,
  and 4,096 deterministic raw floating-point patterns.
- The 54 committed native CLI fixtures compare complete bytes for SVG, PDF,
  EPS, AI and EMF, across layouts, output scaling and stroke settings. They
  use the repository's existing PDF producer and EMF public-header adapters.
- The export benchmark compares complete candidate output bytes against
  baseline output outside the timed region, including DXF spline and dense
  line modes. It also verifies deterministic bytes on every timed repetition.

The full larger serialization corpora were unavailable in this checkout:
`svg-reference.json`, `eps-reference.json.gz`, `pdf-reference.json.gz`,
`ai-reference.json.gz`, `emf-reference.ndjson.gz`, the six
`dxf-reference/part-000.ndjson.gz` through `part-005.ndjson.gz` files,
`dxf-coordinates-reference.ndjson.gz`, and
`emf-coordinates-reference.ndjson.gz`. No pass is claimed for those corpora.

## Reproduce

Use the same compiler, build flags and example source for both revisions.
The example consumes committed fitted native graphs directly; reading JSON,
fitting, translating graph tiles, checksums, byte comparisons and disk I/O
are outside the timed interval. The default workloads are 8-by-8 translated
mosaics of `alpha-profile-3` and `ring-profile-6`. These are deterministic
fixture workloads, not a claimed corpus of production documents. Each
individual export is measured, including its normal allocation and traversal.
EPS uses a fixed date and EMF uses fixed reference-device dimensions.

```sh
git worktree add --detach ../openvector-export-baseline 0efa9aad869e6b58cd7947f3172f44b8a6144215
mkdir -p ../openvector-export-baseline/rust/crates/openvector/examples
cp rust/crates/openvector/examples/benchmark_export.rs ../openvector-export-baseline/rust/crates/openvector/examples/
CARGO_BUILD_JOBS=2 cargo build --manifest-path ../openvector-export-baseline/rust/Cargo.toml --locked -p openvector --release --example benchmark_export
CARGO_BUILD_JOBS=2 cargo build --manifest-path rust/Cargo.toml --locked -p openvector --release --example benchmark_export
python3 docs/performance/benchmark-export.py \
  --before ../openvector-export-baseline/rust/target/release/examples/benchmark_export \
  --after rust/target/release/examples/benchmark_export \
  --candidate-ref "$(git rev-parse HEAD)" \
  --output docs/performance/export-formatting-results.json
```

Run with other compilation and benchmarking stopped. Optional `taskset -c N`
on the Python invocation pins both binaries to the same allowed CPU. Seven
paired samples alternate baseline/candidate order, with an untimed warmup
per invocation. EMF does not use this decimal formatter and is an unchanged
control. The `decimal` row isolates formatting the actual fitted coordinates
at precisions two, three and sixteen; it is not an end-to-end export result.

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openvector --release --lib --test decimal_format
cargo test --manifest-path rust/Cargo.toml --locked -p openvector --lib decimal_format::tests
cargo test --manifest-path rust/Cargo.toml --locked -p openvector --release --test vector_serializers committed_cli_fixtures_preserve_all_export_bytes
cargo test --manifest-path rust/Cargo.toml --locked -p openvector --release --test vector_serializers dxf_all_144_visible_overlong_fields_are_errors
```
