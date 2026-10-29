# Development

The repository includes the independent Rust implementation, JavaScript differential reference, browser preview, tests and component contracts. Large fixtures stay local as described in [test fixtures](test-fixtures.md). Historical `evidence/` links in component contracts refer to the local investigation archive, which is not part of this repository. Original application probes are development-only and are not included in this source distribution.

## JavaScript differential reference

`src/*.mjs` holds the earlier JavaScript implementation of the engine. The
npm archive excludes those modules, the package does not export them, and no
runtime falls back to them. They remain the development reference for the
Rust port.
Many modules in `rust/crates/openvector/src` name the `src/` module they port
in their header comment, and the GPUI viewer modules port `gui/viewer-*.mjs`. The
Rust engine also compiles two data tables from this directory,
`src/parameter-data.json` and `src/segmentation-progress-data.json`. The npm
archive includes both tables and the JPEG/TIFF notices from `src/` so native
rebuilds do not need the JavaScript reference.

`npm test` runs `test/*.test.mjs`. Most of those files replay retained native
captures from `test/fixtures` through `src/`. `test/rust-node.test.mjs` loads
`npm/index.mjs`, so it needs a native build, or `OPENVECTOR_NODE_BINARY` set
to a built `openvector-node.node`. The CLI tests also need the platform's native
executable under `native/`, which `npm run build` produces. The Rust tests under `rust/crates/*/tests`
replay native captures through the Rust engine and read `test/fixtures`
unless `OPENVECTOR_FIXTURES` names another directory. Each component contract
in `docs/` records its stage's settings, boundaries and comparison corpus, and
`docs/reconstruction-contract.json` collects the reference's comparison
counts. Those counts describe the JavaScript reference; the
[parity matrix](parity-matrix.md) tracks what the Rust port passes.

The reference keeps the sources and notices for its zlib 1.2.3 and IJG
jpeg-6b WebAssembly builds, its FreeType rasterizer and its Qt Bezier module
in `src/vendor/`. On Windows its file saves use
`src/native/file-win32-x64.node`, which `python3 tools/build-file-win32.py`
rebuilds. `bin/batch-worker.mjs` is the reference's batch worker and is not
packaged.

`gui/` holds the earlier browser preview, which runs the `src/` engine through
a local Node host. It is not shipped, and its runtime dependencies are listed
in the [GUI third-party notices](gui-third-party-notices.md). Install its
dependencies with `npm --prefix gui ci`; after `npm --prefix gui run build`,
`npm --prefix gui test` runs its engine and host tests.

## Development

```sh
# In the authorized build environment:
npm run build
npm test
cargo test --manifest-path rust/Cargo.toml --locked -p openvector -p openvector-cli
npm pack
```

The package is private and unpublished. Packaging always uses
the native build described above. Investigation tools, original binaries,
extracted resources and decompiler output are excluded. The MIT license
covers independently authored source; it does not relicense Vector Magic or
analysis-tool materials.

The recovery map is in `docs/reverse-engineering-report.md`. Component
contracts such as `docs/core-settings-contract.json`,
`docs/segmentation-contract.json`, `docs/smoothing-contract.json` and
`docs/fitting-contract.json` describe each stage. Reproduce the
development-only native recordings with `docs/reference-probes.md`. The tools
and original executable are not needed to run the library, CLI, or test suite.
