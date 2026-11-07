# Exact rolling histograms for noise reduction

## Change

`reduce_prepared_noise` previously gathered and sorted four overlapping windows
for every pixel. For amounts 2 and 3 (5×5 and 7×7), it now keeps one byte
histogram per channel and slides each window by removing one column and adding
one column. A second level of 16 coarse counts locates a rank without scanning
all 256 values. All scratch storage is bounded; no cache survives the call.

The selected byte is identical to the sorted-window result. The histogram also
implements the native nonpeak rule: when the median is the minimum or maximum,
select the next distinct value toward the interior, unless the window is
constant. Border coordinates still clamp and count repeated edge samples.
The original size check, owned output, 3×3 path (amounts 0/1), and larger-kernel
fallback remain in place. No floating-point operations or approximation are
introduced.

## Measured results

Seven alternating paired release samples on AMD EPYC 9V74 80-Core Processor, Linux, CPU 0, using `rustc 1.99.1 (58cf8f9c3 2026-10-13)`. Candidate source: `b7b1ee4861c556b964229449eeade41bcf7540e3`. All 39 workload fingerprints match across every sample. The 26 optimized-path workloads improve 2.46–10.91×.

| Workload | Kernel | Before, ms | After, ms | Speedup |
| --- | --- | ---: | ---: | ---: |
| Recorded AA logo, 256×192 | 5×5 | 7.434 | 2.546 | 2.92× |
| Recorded AA logo, 256×192 | 7×7 | 16.987 | 3.060 | 5.55× |
| Recorded fox photo, 128×85 | 5×5 | 3.588 | 0.892 | 4.02× |
| Recorded fox photo, 128×85 | 7×7 | 9.180 | 1.014 | 9.05× |
| Authored flat image, 512×384 | 5×5 | 21.169 | 8.594 | 2.46× |
| Authored flat image, 512×384 | 7×7 | 39.747 | 11.006 | 3.61× |
| Authored noisy image, 512×384 | 5×5 | 66.948 | 20.022 | 3.34× |
| Authored noisy image, 512×384 | 7×7 | 188.402 | 21.202 | 8.89× |

The unchanged 3×3 controls range from 0.92–1.14×, showing the remaining shared-host timing variation. Final measurements ran with the other project builds, tests and benchmarks stopped. All raw samples, hashes, source refs and environment details are in [noise-median.json](noise-median.json). These are stage timings; overall conversion gains depend on whether noise reduction is enabled and how much time it consumes.

## Reproduction

Build the identical `rust/crates/openvector/examples/benchmark_noise.rs` at the
baseline and candidate revisions with the same compiler and release profile:

```sh
git worktree add ../openvector-noise-before 0efa9aad869e6b58cd7947f3172f44b8a6144215
mkdir -p ../openvector-noise-before/rust/crates/openvector/examples
cp rust/crates/openvector/examples/benchmark_noise.rs ../openvector-noise-before/rust/crates/openvector/examples/
cargo build --locked --release --manifest-path ../openvector-noise-before/rust/Cargo.toml -p openvector --example benchmark_noise
cargo build --locked --release --manifest-path rust/Cargo.toml -p openvector --example benchmark_noise
python3 docs/performance/benchmark-noise.py \
  --before ../openvector-noise-before/rust/target/release/examples/benchmark_noise \
  --after rust/target/release/examples/benchmark_noise \
  --samples 7 --warmup 1 --cpu 0 \
  --before-ref 0efa9aad869e6b58cd7947f3172f44b8a6144215 \
  --after-ref b7b1ee4861c556b964229449eeade41bcf7540e3 \
  --compiler "$(rustc --version)" --output noise-median.json
```

Choose an allowed CPU when `0` is unavailable, or omit `--cpu` to inherit
affinity. The fixture directory defaults to the checkout's `test/fixtures` and
can be supplied with `OPENVECTOR_FIXTURES`.

The driver alternates A/B and B/A order, checks exact input/output SHA-256
fingerprints for every sample, and reports median milliseconds per call.
Each process performs one untimed correctness call, one warmup batch, and one
timed batch per case. The smallest cases batch multiple calls to reduce timer
noise. Allocation and output destruction are timed; decoding and hashing are
excluded. These are noise-reduction stage timings, not complete vectorization
timings. Amount 1 is an unchanged 3×3 control.

## Correctness checks

- The independent original sort oracle matches every byte on 480 combinations
  of dimensions, deterministic random patterns, extrema, repeated values,
  transparent channels, clamped borders, and amounts 0 through 4.
- Focused tests cover minimum/maximum medians crossing byte-bin boundaries.
- The existing 176 native size-error cases retain their exact error messages.
- The existing document-source suite matches all 160 recorded composition and
  preprocessing actions across 10 cases, including source and working pixels.
- The benchmark compares 39 workloads: 11 captured artwork/photo inputs and
  two authored 512×384 flat/noisy inputs, each at amounts 1, 2 and 3.

```sh
cargo test --locked --manifest-path rust/Cargo.toml -p openvector --test noise_histogram
cargo test --locked --manifest-path rust/Cargo.toml -p openvector --test image_preparation median_size_preconditions_preserve_every_native_error_reason
cargo test --locked --manifest-path rust/Cargo.toml -p openvector --test image_preparation document_background_composition_and_filtering_match_native_source_caches
```

The complete private native differential corpus is not present in this checkout;
the separate median/despeckle kernel and preparation fixture suites were not run.
