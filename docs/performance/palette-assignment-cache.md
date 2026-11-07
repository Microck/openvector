# Exact palette assignment cache

## Scope and implementation

The Rust `assign_indices` loop compares each BGRA pixel with every palette color. Palette refinement repeats this pass up to ten times; automatic color analysis generates candidates with two through twelve colors. The same assignment kernel is also used by explicit palette assignment and supplied-palette segmentation.

The change adds a direct-mapped cache of 1,024 `(BGRA, palette index)` entries within each assignment call. A hit reuses the nearest-color index already calculated for that exact pixel value. A miss runs the original distance loop and replaces the entry. Storage is bounded at 8 KiB on the stack, with no image-sized allocation.

Every hit checks the complete 32-bit BGRA key, including alpha. Two different colors in the same hash slot cause a cache miss; collisions can reduce the speedup but cannot change the selected color. The index sentinel `-1` keeps an empty cache entry distinct from the valid all-zero BGRA value. The cache is reset on every call, so refinement cannot reuse an index calculated for an earlier palette.

## Why outputs remain exact

For a fixed palette, identical BGRA bytes have identical normalized channels and nearest-color distances. Returning their previously calculated index preserves that result. On misses, the original float32 distance arithmetic, palette traversal order and strict `<` comparison remain intact. Equal distances still choose the first palette entry.

Pixels retain their original order. Mean and weight accumulation, convergence tests, palette updates, random draws, progress callbacks and cancellation checkpoints are unchanged. The cache introduces no color quantization, approximate distance or floating-point reassociation.

The regression tests retain the original scalar assignment and refinement loops as independent references. They cover repeated and unique colors, long runs, transparent samples, signed zero, equal-distance ties, palette changes and empty clusters. A targeted case alternates two distinct transparent BGRA colors that map to the same cache slot, including the all-zero key. Weighted and unweighted refinement compare palette/weight bits, every assignment, pass count and final RNG state.

## Measured results

Seven alternating paired release samples per operation on AMD EPYC 9V74, Linux, CPU 0, using `rustc 1.99.1 (58cf8f9c3 2026-10-13)`. Both builds use the exact source revisions listed below. Final runs were serialized after other project builds, tests and benchmarks stopped. Values are median milliseconds per stage call.

| Workload | Generation before → after, ms | Speedup | Automatic analysis before → after, ms | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Recorded AA logo, 256×192 | 14.265 → 4.877 | 2.92× | 117.360 → 59.527 | 1.97× |
| Recorded AA text, 256×160 | 11.910 → 4.094 | 2.91× | 116.681 → 60.300 | 1.93× |
| Recorded fox photo, 128×85 | 2.765 → 2.230 | 1.24× | 19.272 → 14.264 | 1.35× |
| Recorded Jim Devine photo, 103×128 | 3.324 → 3.241 | 1.03× | 20.006 → 19.248 | 1.04× |
| Authored poster, AA off, 512×384 | 51.688 → 12.397 | 4.17× | 258.537 → 78.563 | 3.29× |
| Authored high-entropy noise, 256×192 | 12.961 → 12.798 | 1.01× | 96.100 → 99.011 | 0.97× |

Captured artwork generation improves 1.74–2.92×; full automatic analysis improves 1.51–1.97×. The authored poster reaches 4.17× for generation and 3.29× for automatic analysis. Workloads with few repeated colors benefit less: the high-entropy control measures 1.01× for generation and 0.97× for automatic analysis. Shared-host timings can vary.

Exact full snapshots match for all 14 inputs in both operations: 20,277,928 generation bytes and 34,248,260 analysis bytes, including integer/float bits and the separate observer/cancellation checks. Raw timing samples, fingerprints and environment metadata are retained in [palette-generation.json](palette-generation.json) and [palette-analysis.json](palette-analysis.json).

## Validation performed

| Validation | Coverage |
| --- | --- |
| New scalar-reference tests | 3 tests; assignment, weighted/unweighted refinement, collisions and palette changes |
| Native palette assignment | 1,193 cases |
| Native refinement sessions | 89 unweighted and 80 weighted cases |
| Native generation sessions | 74 unweighted and 80 weighted cases |
| Automatic-color sessions | 576 cases, including native failure and random-stream effects |
| Retained document palette state | 10 cases and 220 actions |
| Palette iteration cancellation | 480 native cases, each replayed through the direct and task APIs |
| Generation A/B snapshots | All 14 benchmark inputs; complete output, iteration observers and cancellation state compared byte for byte |
| Automatic-analysis A/B snapshots | All 14 inputs; candidate outputs, scores, features, peak weights, classification, observers, cancellation and RNG state compared byte for byte |

The available palette-kernel tests passed: six tests, including the native assignment corpus, boundary validation and contrast measurement. Two unrelated tests require `peaks-reference.json` and `score-reference.json`, which are absent from this checkout; the focused command below explicitly excludes them. This is a focused validation set, not a claim that the full repository suite or every private corpus was run. No fixture expectations are changed.

Reproduce the native and scalar checks with:

```sh
cargo test --release --locked --manifest-path rust/Cargo.toml -p openvector \
  --test palette_assignment_cache --test palette_session --test document_palette_native

cargo test --release --locked --manifest-path rust/Cargo.toml -p openvector \
  --test palette_kernels -- \
  --skip histogram_peaks_and_their_tie_ordering_match_all_native_scans \
  --skip unweighted_candidate_scores_match_all_unmodified_original_engine_calls

cargo test --release --locked --manifest-path rust/Cargo.toml -p openvector \
  --test document_palette_cancellation palette_iteration_cancellation -- --exact
```

## Benchmark scope and inputs

`generate` measures a complete palette-generation call, including histogram preparation, antialias measurement when enabled, iterative assignment/refinement and final palette conversion. The recorded workload requests twelve colors.

`analyze` measures complete automatic color analysis: preparation, candidate generation for two through twelve colors, candidate scoring, feature construction and classification. This includes work outside the optimized assignment loop. These are Rust stage timings; the benchmark does not measure complete image-to-SVG conversion.

The fourteen inputs are:

- All eight working rasters from `fresh-representative-20261020.ndjson.gz`: two small transparent graphics, an antialiased logo, text, thin holes/lines, a JPEG logo, a palette-rich image and alpha rings.
- All three working rasters from `fresh-photographic-20261020.ndjson.gz`.
- Two authored 512×384 posters with flat panels, diagonal ribbons, circular marks, type-like strokes and transparency: one without antialiasing and one with four subpixel samples per output pixel.
- A deterministic 256×192 high-entropy noise image, included to expose cache-overhead regressions.

Captured rasters use their captured palette controls. Every stage call starts at RNG seed 1. These are reproducible cross-revision comparisons on captured inputs; they do not claim to replay the recorder's final RNG state. Photos and noise remain in the report even when their speedup is smaller.

## Reproduce the paired measurements

Production source revisions:

- Main baseline: `0efa9aad869e6b58cd7947f3172f44b8a6144215`.
- Candidate: `ae08356112983027197b2564bd7d054212dff1e9`.

Use the same Rust compiler, release build settings and fixture directory for both revisions. The Python driver requires Python 3.10 or later and has no external dependencies. Start from the candidate checkout. Create the baseline worktree below, or reuse an existing `../openvector-before` checkout at the exact baseline revision. Copy only the benchmark example into it so both builds use the identical harness:

```sh
git worktree add --detach ../openvector-before 0efa9aad869e6b58cd7947f3172f44b8a6144215
mkdir -p ../openvector-before/rust/crates/openvector/examples
cp rust/crates/openvector/examples/benchmark_palette.rs ../openvector-before/rust/crates/openvector/examples/benchmark_palette.rs
cargo build --release --locked --manifest-path ../openvector-before/rust/Cargo.toml -p openvector --example benchmark_palette
cargo build --release --locked --manifest-path rust/Cargo.toml -p openvector --example benchmark_palette
```

Run the two workloads separately with seven alternating baseline/candidate rounds, one warmup per case per timed process, and logical CPU 0:

```sh
python3 docs/performance/benchmark-palette-cache.py \
  --baseline ../openvector-before/rust/target/release/examples/benchmark_palette \
  --candidate rust/target/release/examples/benchmark_palette \
  --baseline-ref 0efa9aad869e6b58cd7947f3172f44b8a6144215 \
  --candidate-ref ae08356112983027197b2564bd7d054212dff1e9 \
  --compiler "$(rustc --version)" \
  --operation generate --count 12 --rounds 7 --warmup 1 --cpu 0 \
  --output docs/performance/palette-generation.json

python3 docs/performance/benchmark-palette-cache.py \
  --baseline ../openvector-before/rust/target/release/examples/benchmark_palette \
  --candidate rust/target/release/examples/benchmark_palette \
  --baseline-ref 0efa9aad869e6b58cd7947f3172f44b8a6144215 \
  --candidate-ref ae08356112983027197b2564bd7d054212dff1e9 \
  --compiler "$(rustc --version)" \
  --operation analyze --rounds 7 --warmup 1 --cpu 0 \
  --output docs/performance/palette-analysis.json
```

CPU pinning is optional; select an available logical CPU when reproducing elsewhere. Stop other builds and benchmarks during measurement.

Before collecting timing samples, the driver compares complete serialized integer/float-bit outputs from both executables. It also checks matching case IDs, controls and input hashes, and rejects identical resolved executable paths.

Observer and cancellation checks use a separate `PreparedPaletteAnalysis::evaluate` call at the requested `--count` (twelve in the commands above). They capture every iteration callback and cancellation at a middle callback, including the partial event sequence and RNG state. No observers are attached to timed calls.

Each timed process performs one initial untimed correctness call per case in addition to its requested warmup. Fixture loading, input construction, hashing, exact comparisons, observer checks and destruction of the returned output are outside the timer. Every timed result is checked against its initial output fingerprint after the timer stops.

The report contains every raw timing sample, the median for each revision, per-case speedup and time reduction, alternating run order, source revisions supplied on the command line, binary hashes, compiler metadata, CPU/platform information, and one set of input/output/observer fingerprints. It does not duplicate full output payloads for each timing round.

Use `--verify-only` to compare outputs without collecting performance samples, `--case=substring` to select inputs, or `--count=6` to change the generated palette size. Automatic analysis always evaluates counts two through twelve. The standalone Rust example accepts `--snapshots=path.json` to retain complete outputs for inspection.
