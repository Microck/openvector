# Development

The repository contains the Rust engine, CLI and desktop application, plus a
JavaScript reference implementation and HTTP test host. The browser preview is
retired. The Rust engine also compiles `src/parameter-data.json` and `src/segmentation-progress-data.json`;
the npm archive includes those tables and the JPEG/TIFF notices needed for
native rebuilds.

## Checks

`npm run test:smoke` runs the original pixel-map and decimal-format checks plus
selected independently authored codec, geometry, metadata and GUI-state tests.
It does not replace the full regression suites.

The tracked reference fixtures and existing regression tests remain in the
checkout. Full JavaScript, Rust and GUI suites also require the private large
corpus described in [test fixtures](test-fixtures.md). Missing fixtures remain
errors, not skipped tests. The corpus is not part of the published package.

`npm test` runs the JavaScript suite. Build the native CLI and addon with
`npm run build` first. Run `npm --prefix gui test` for the retained GUI reference
models and HTTP integration tests; no frontend build or npm dependencies are
needed for that test host. Rust tests use `cargo test --manifest-path rust/Cargo.toml --locked`.
Do not treat smoke success as a release gate for the full product.

The WebAssembly rebuild scripts read `source-identity.json` from their
respective directories under `src/vendor/`. These manifests pin the official
source archives and retained files. The zlib 1.2.3 files and Qt 4.4.3 JPEG files
match upstream byte for byte. The rasterizer manifest records both upstream
and modified hashes; its modification notice describes the changes.
Run `python tools/build-bitmap-{deflate,jpeg,raster}.py <LLVM-bin-directory>`
separately for each component with the Windows LLVM toolchain. Each script
checks the source hashes before compiling and writes a build receipt under
the ignored `evidence/` directory. Compiler versions can change the resulting
WebAssembly bytes; run the codec and raster tests after rebuilding.

The [GUI third-party notices](gui-third-party-notices.md) preserve the retired
browser preview's attribution. Native binaries use the build and licensing
requirements in [building](building.md). They are not downloaded as a fallback
on import.

The MIT license covers the project's source. Third-party code retains its own
notices and license obligations; see the notices included with each build.
