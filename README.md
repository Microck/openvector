<h1 align="center">openvector</h1>
<p align="center">Raster-to-vector conversion for the desktop, terminal, and Node.js.</p>

OpenVector turns PNG, JPEG, BMP, GIF, and TIFF images into editable vectors.
One Rust engine powers the native desktop app, CLI, and Node-API library.
It runs independently and does not require Vector Magic or a network service.

[Build](docs/building.md) · [CLI and Node API](docs/usage.md) · [Verification](docs/verification.md) · [Release runbook](docs/release-runbook.md)

## What it does

- Automatic tracing, manual palettes, segmentation editing, and background removal.
- SVG, PDF, EPS, AI, DXF, and EMF export, plus rasterized PNG, JPEG, BMP, and TIFF output.
- Batch conversion, saved advanced profiles, and recent basic settings.
- A native GPUI desktop interface, with the same engine available to scripts.

This is a private source repository. No public npm package or versioned application
release is available from it yet. The supported comparison goal is complete, but
full equivalence with Vector Magic and every platform/input combination is not
established. See [verification and known limits](docs/verification.md).

## Build from source

Use Node.js 22.19 or later, npm, Rust with its `rust-docs` component, Git, and a C
compiler/linker. Native packaging also needs platform-specific toolchain notices.
The [build guide](docs/building.md) covers supported targets, cross-compilation,
Linux libraries, and the Windows Node-API linker input.

```sh
git clone https://github.com/Microck/openvector.git
cd openvector
npm ci
npm run build
node bin/openvector.mjs --help
```

Repository access requires authentication. `npm run build` produces the CLI and
Node addon for the selected platform. It does not build the desktop app.

```sh
cargo run --manifest-path rust/Cargo.toml --locked -p openvector-gui -- logo.png
```

The desktop app needs a graphical session. Use `OPENVECTOR_PREFERENCES` to select a
separate preferences file when testing.

## Convert an image

```sh
node bin/openvector.mjs vectorize logo.png --output logo.svg
node bin/openvector.mjs vectorize logo.png --output logo.pdf
mkdir vectors
node bin/openvector.mjs batch first.png second.jpg --format svg --output-dir vectors
```

The output suffix selects the format. Existing outputs require an explicit
replacement option. Processing does not rewrite the source image.

## Use it from Node.js

After building the native addon:

```js
import { readFileSync } from 'node:fs';
import { CoreSession } from 'openvector';

const document = new CoreSession().createDocument();
document.setSource(readFileSync('logo.png'), 'logo.png');
document.runAutomatic({ extension: '.png' });
const saved = document.saveVector('logo.svg', { layout: 'cutouts' });
if (saved.status !== 0 || saved.error) {
  throw new Error(saved.error?.message ?? `Export failed: ${saved.status}`);
}
```

Calls are synchronous. The [API guide](docs/usage.md) covers progress, cancellation,
batches, editing, snapshots, and profile storage. The package never downloads a
binary or falls back to the JavaScript reference implementation on import.

## Development

```sh
npm run test:smoke
```

That command runs the small reference checks included in the default checkout.
It is also the scope of repository CI, not a claim that the full native or desktop
suite passed. See [development](docs/development.md) and [test fixtures](docs/test-fixtures.md)
for the full suite and its external fixture corpus.

| Directory | Contents |
| --- | --- |
| `rust/crates/openvector` | Shared tracing engine |
| `rust/crates/openvector-gui` | Native desktop app |
| `rust/crates/openvector-cli` | CLI |
| `rust/crates/openvector-node`, `npm/` | Node-API binding and public declarations |
| `src/`, `gui/` | Development reference implementation and browser preview |
| `test/`, `docs/` | Tests, fixtures, usage guides, and behavioral contracts |

## License

Application code is [MIT licensed](LICENSE). Third-party code, fonts, and icons
retain their own licenses. Packages must include their generated notices and
editable source where required. Qt-derived code has LGPL requirements; review the
[distribution notes](docs/building.md) before a public binary release.

OpenVector is an independent project and is not affiliated with Vector Magic.
