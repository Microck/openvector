# PNG to SVG in a browser worker

The prototype passes the original PNG bytes to the Rust decoder, runs automatic tracing with default settings, and exports an SVG. The standalone HTML embeds the engine and starts a worker. It needs no conversion server.

## Initial results

On November 25, 2026, Chromium 156.0.8078.79 ran the prototype on a CloudSSH Raft Linux x86-64 guest with an Intel Xeon Platinum 8370C. All 30 browser downloads matched the released 0.1.5 CLI byte for byte. The native harness also matched the CLI on all five inputs. The [raw results](wasm-prototype.json) contain timing samples and SHA-256 hashes.

| PNG | Native median | Browser median | SVG paths | SVG bytes |
|---|---:|---:|---:|---:|
| triangle.png | 4.7 ms | 9.1 ms | 2 | 710 |
| alpha-logo.png | 682.1 ms | 923.7 ms | 2 | 1,427 |
| recent30-artwork.png | 27.5 ms | 42.5 ms | 4 | 1,945 |
| gradient.png | 49.4 ms | 64.6 ms | 31 | 8,413 |
| openvector-logo.png | 5851.1 ms | 8317.0 ms | 2 | 1,552 |

Local HTTP navigation to worker readiness took 322 ms on one fresh page. The standalone HTML was 3,576,728 bytes, or 541,583 bytes with gzip. The run used default settings and one desktop browser. Phone performance, other browsers, photography, other export formats, and editing remain unmeasured.

## Optimization run

On a separate AMD EPYC 9V74 CloudSSH Raft guest, the original HTML rebuilt to the same SHA-256 as the Intel run. The following medians are all from the AMD guest. The native column uses the selected Rust change. Each reported median excludes the first conversion and uses five more. The browser builds rotated order with each round. Native timings were collected in separate runs on the same machine. See the [timings, stage profiles, input hashes, and output hashes](wasm-optimization.json).

| PNG | Original Wasm (ms) | Selected Wasm (ms) | Selected native (ms) |
|---|---:|---:|---:|
| alpha-logo.png | 706.6 | 660.4 | 422.7 |
| curves-and-hole.png | 374.3 | 377.0 | 240.3 |
| gradient.png | 48.8 | 48.8 | 38.8 |
| near-colors.png | 167.1 | 155.6 | 105.4 |
| triangle.png | 5.8 | 5.8 | 3.9 |
| recent30-artwork.png | 33.3 | 32.4 | 21.2 |
| openvector-logo.png | 6,786.6 | 6,066.9 | 4,547.2 |

Across seven PNGs and all experiment builds, 354 downloaded SVGs matched the 0.1.5 CLI bytes. The five original inputs also retained the hashes from the first benchmark. These checks cover automatic SVG export in desktop Chromium. The selected build also passed both energy-cache tests and all three tests in the image-measurement workspace target. The new regression compares each moving-contour evaluation against a dense sum and includes residuals for which reversing the sum changes its bits.

### Trials on the 1024 × 1024 logo

| Change | Paired baseline | Candidate | Browser time change |
|---|---:|---:|---:|
| Fat Rust LTO | 6.968 s | 7.159 s | +2.7% |
| Wasm SIMD128 | 6.968 s | 7.318 s | +5.0% |
| LTO and SIMD128 | 6.968 s | 7.037 s | +1.0% |
| Exact division by small denominators | 6.557 s | 6.797 s | +3.7% |
| Update only touched pixels | 6.673 s | 6.135 s | −8.1% |
| Update only touched pixels, confirmation | 6.787 s | 6.067 s | −10.6% |
| Also skip blocks known to have zero error, confirmation | 6.787 s | 5.916 s | −12.8% |

The compiler and arithmetic trials used three measured rounds after warm-up. LTO used Cargo's fat release LTO. SIMD used `-C target-feature=+simd128` for the Wasm Rust target. The division trial calculated quotient and remainder with two base-2³² digits for small divisors. It passed 1,000,180 boundary and deterministic randomized comparisons against Rust's `u128` arithmetic on each of native and Wasm.

Stage timings came from an instrumented build on the 160 px and 1024 px logos. Smoothing on the 1024 px input took 4.13 s in Wasm and 2.33 s native in the warm profiled trace. Sampling the worker with Chromium's CPU profiler put `ImageKernel::energy` first by self samples. Timing trials used ordinary builds without profiling.

The selected change updates cached squared errors for the contour pixels visited in each evaluation. It then sums the entire cache in the original row-major order. A second implementation tracked which 64-pixel blocks had ever contained nonzero error. It saved another 151 ms on the large logo, while the near-color fixture rose from 156 ms to 174 ms. The selected version needs no extra per-image state.

To compare two built directories against the released CLI, run the following with the original and candidate artifacts in separately named directories:

```sh
python3 scripts/wasm-prototype/compare.py --samples 5 --cli "$OPENVECTOR_CLI" \
  --native "$CANDIDATE/native-trace" --build "$BASELINE" --build "$CANDIDATE" \
  scripts/wasm-prototype/fixtures/*.png test/fixtures/recent30-artwork.png \
  .github/assets/openvector-logo.png
```

## Reproduce the benchmark

Use a Linux x86-64 environment with Rust 1.100.0, the `wasm32-unknown-emscripten` Rust target, Emscripten 4.0.22 on PATH, Python with Playwright, and Chromium. The benchmark accepts the released OpenVector CLI as an independent output reference. Set `CHROMIUM` if the browser is installed outside `/usr/bin/chromium`.

From the repository root, with the released binary's absolute path in `OPENVECTOR_CLI`, run

```sh
bash scripts/wasm-prototype/build.sh artifacts/wasm-prototype
python3 scripts/wasm-prototype/benchmark.py artifacts/wasm-prototype "$OPENVECTOR_CLI" \
  scripts/wasm-prototype/fixtures/triangle.png \
  scripts/wasm-prototype/fixtures/alpha-logo.png \
  test/fixtures/recent30-artwork.png \
  scripts/wasm-prototype/fixtures/gradient.png \
  .github/assets/openvector-logo.png > wasm-results.json
```

The build produces `openvector.html` and `native-trace`. Both targets time the same Rust function from encoded bytes to an SVG string, with a fresh document for each conversion. The timer excludes user file selection, copying data into the worker, preview painting and disk writes. The harness measures page navigation to worker readiness separately over local HTTP.

Each case has six native and six browser conversions. Sample zero is retained in the JSON and excluded from the medians. The browser uses one worker throughout the run. Each sample requires a fresh blob URL and downloads the SVG through the page. The benchmark compares the browser downloads and the final native harness output with the released CLI. The full-size and resized logos are the same artwork. The other fixtures are flat shapes and a synthetic gradient.
