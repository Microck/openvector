# Smoothing pixel workspace

The image-based smoothing objective now retains its per-pixel workspace and tracks
pixels reached by contour tracing. Each evaluation clears the previously touched
lists, initializes backgrounds only for newly touched pixels, and clips and adds
gradients only for touched pixels. Sorting those indices preserves the previous
row-major clipping and floating-point accumulation order. The energy sum still
visits every pixel in exactly its original order; cached colors, flags, fragment
scratch semantics, optimizer budgets and progress/cancellation behavior are unchanged.
The extra index vector holds one `usize` per touched pixel, plus capacity slack.
Dense contours can touch the entire image, so this is not a constant-memory change.

## Measurements

Baseline: `0efa9aad869e6b58cd7947f3172f44b8a6144215`. Both builds used identical
benchmark sources, dependencies, Rust 1.99.1 / LLVM 23.1.1 and default Cargo release
settings (`--locked --offline --release`). Host: Debian 13 x86_64, AMD EPYC 9V74,
shared five-vCPU cloud instance; all benchmark processes pinned to CPU 0. Seven
paired samples alternate baseline/candidate order. Builds and tests were stopped
while timing. These are wall-clock measurements on a shared host, not confidence
intervals or a claim about every image or platform.

| Workload | Before median | After median | Time reduction |
| --- | ---: | ---: | ---: |
| Full smoothing, authored scaled ring, 544×416 | 410.881 ms | 381.271 ms | 7.21% |
| Full smoothing, authored scaled ring, 136×104 | 93.166 ms | 92.190 ms | 1.05% |
| Persistent image energy + gradient, 1088×832 | 7.380 ms | 5.875 ms | 20.39% |
| Persistent image energy + gradient, 272×208 | 0.594 ms | 0.576 ms | 3.01% |
| Persistent image energy + gradient, 17×13 | 0.0333 ms | 0.0347 ms | **4.07% more time** |
| Sum of all 15 original small full-smoothing cases | 553.623 ms | 551.865 ms | 0.32% |

Small-case differences are near host variation; no reliable overall small-corpus
speedup is claimed. The smallest objective regresses by about 1.4 microseconds.
The useful gain is reduced empty-pixel work on larger, sparsely traced images.
These inputs do not establish photographic, dense-contour, end-to-end conversion,
GUI responsiveness or cross-platform performance.

The full original corpus uses 10 repetitions per case per sample and compares
geometry, penalty counts and RNG state to the existing checked-in references.
The image-objective benchmark uses 40 evaluations per sample after four warmup
states, moving contours through four coordinate states and returning to the start.
Its timed region excludes JSON, hashing and snapshot construction. Full scaled
smoothing uses four repetitions per sample after one warmup. It replicates the
checked-in ring's BGRA and labels, scales geometry, region pixel counts and boundary
sample counts, and resets the original RNG seed on every call. The full smoothing
function, including its preparation, is timed; input construction is excluded.

Every baseline/candidate image snapshot hash matched at all three sizes, including
energy, raw gradient, cached colors, flags, clipped records and invalid regions.
Both full scaled outputs matched, including geometry, penalties, references,
border counts and final RNG state. All samples, per-case timings and snapshot hashes
are retained in [the measurement record](smoothing-pixel-workspace.json).

Temporary diagnostic timing before optimization measured 2,843 ms in objective
calls, with only 26 ms in the prior loop, across the small pipeline corpus. Those
instrumented runs guided inspection and are excluded from the A/B measurements.

## Reproduce

Use the same compiler and dependency cache for both builds. Copy the two benchmark
examples from this change into a separate checkout of the baseline; they depend
only on existing test helpers and checked-in fixtures. Keep the resulting
executables in separate directories before rebuilding.

```sh
# Run in each checkout; preserve both resulting executables before the next build.
cargo build --manifest-path rust/Cargo.toml --locked --offline --release \
  -p openvector --example bench_smoothing --example bench_smoothing_image
# Copy rust/target/release/examples/bench_smoothing{,_image} into BEFORE or AFTER.
python3 docs/performance/benchmark-smoothing.py BEFORE AFTER --samples 7 --cpu 0 \
  --output smoothing-results.json
```

The driver alternates order, checks matching hashes, records raw samples and prints
per-case medians. Choose a CPU allowed by your host's affinity mask. Run without
concurrent builds or tests. Individual examples also work directly:

```sh
bench_smoothing 10
bench_smoothing_image 64 40
bench_smoothing_image 32 4 full
```

Correctness checks completed locally:

- Existing `contour_smoothing::full_smoothing_matches_all_15_original_pipeline_targets`:
  all 15 original pipeline cases match exactly.
- New `image_measurement_workspace` regression: persistent full snapshots match the
  unchanged baseline across entering/leaving pixels and repeated coordinate cycles.
  This test was also run against the unchanged baseline implementation.
- Engine library unit tests: 3 passed.
- Source smoke checks: 33 passed, no failures or skips.

The complete private differential corpus is unavailable here. These focused checks
do not establish complete parity or resolve the release's documented missing-capture
and strict-suite limits. No existing fixture, expectation, release gate, accessibility
control or iteration budget was changed.
