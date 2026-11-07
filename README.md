<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/assets/openvector-logo-reversed.png">
    <img src=".github/assets/openvector-logo.png" width="160" height="160" alt="openvector logo">
  </picture>
  <h1>openvector</h1>
  <p>raster-to-vector conversion for the desktop, terminal, and node.js.</p>
  <p>
    <a href="https://github.com/Microck/openvector/releases/latest"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/release-0.1.2.svg?variant=secondary&amp;size=xs&amp;logo=false&amp;mode=dark"><img src="https://shieldcn.dev/badge/release-0.1.2.svg?variant=secondary&amp;size=xs&amp;logo=false&amp;mode=light" alt="release 0.1.2"></picture></a>
    <a href="https://github.com/Microck/openvector/releases"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/github/downloads/Microck/openvector.svg?variant=secondary&amp;size=xs&amp;logo=false&amp;mode=dark"><img src="https://shieldcn.dev/github/downloads/Microck/openvector.svg?variant=secondary&amp;size=xs&amp;logo=false&amp;mode=light" alt="total release downloads"></picture></a>
    <a href="https://github.com/Microck/openvector/actions/workflows/source.yml"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/github/ci/Microck/openvector.svg?workflow=source.yml&amp;branch=main&amp;variant=secondary&amp;size=xs&amp;logo=false&amp;mode=dark"><img src="https://shieldcn.dev/github/ci/Microck/openvector.svg?workflow=source.yml&amp;branch=main&amp;variant=secondary&amp;size=xs&amp;logo=false&amp;mode=light" alt="source ci status"></picture></a>
    <a href="LICENSE"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/license-MIT.svg?variant=secondary&amp;size=xs&amp;logo=false&amp;mode=dark"><img src="https://shieldcn.dev/badge/license-MIT.svg?variant=secondary&amp;size=xs&amp;logo=false&amp;mode=light" alt="MIT license"></picture></a>
  </p>
</div>

---

openvector turns png, jpeg, bmp, gif, and tiff images into editable vectors.
one rust engine powers the native desktop app, cli, and node-api library.
it runs independently and does not require vector magic or a network service.

[build](docs/building.md) | [cli and node api](docs/usage.md) | [verification](docs/verification.md) | [release runbook](docs/release-runbook.md)

## what it does

- automatic tracing, manual palettes, segmentation editing, and background removal.
- svg, pdf, eps, ai, dxf, and emf export, plus rasterized png, jpeg, bmp, and tiff output.
- batch conversion, saved advanced profiles, and recent basic settings.
- a native gpui desktop interface, with the same engine available to scripts.

## download

download the desktop app from [github releases](https://github.com/Microck/openvector/releases/latest).
each package includes its third-party notices.

### v0.1.2

| platform | package | run |
| --- | --- | --- |
| macos, apple silicon | [dmg](https://github.com/Microck/openvector/releases/download/v0.1.2/openvector-0.1.2-macos-arm64.dmg) | open the disk image and drag OpenVector to Applications |
| macos, intel | [dmg](https://github.com/Microck/openvector/releases/download/v0.1.2/openvector-0.1.2-macos-x64.dmg) | open the disk image and drag OpenVector to Applications |
| linux, amd64 | [tar.gz](https://github.com/Microck/openvector/releases/download/v0.1.2/openvector-0.1.2-linux-amd64.tar.gz) | extract, then run `./openvector-0.1.2-linux-amd64/openvector-gui` |
| linux, arm64 | [tar.gz](https://github.com/Microck/openvector/releases/download/v0.1.2/openvector-0.1.2-linux-arm64.tar.gz) | extract, then run `./openvector-0.1.2-linux-arm64/openvector-gui` |
| windows 10 and 11, x64 | [installer](https://github.com/Microck/openvector/releases/download/v0.1.2/openvector-0.1.2-windows-x64-Setup.exe) or [portable zip](https://github.com/Microck/openvector/releases/download/v0.1.2/openvector-0.1.2-windows-x64-portable.zip) | run the installer, or extract the whole zip and open `openvector-gui.exe` |

macos packages are developer id-signed, notarized, and stapled.

windows executables are unsigned; the portable package needs no installation
but still saves preferences in the normal per-user location.

linux packages target ubuntu 24.04 with glibc 2.39 and were tested with X11.
the required desktop libraries are in the [build guide](docs/building.md).
wayland remains unverified. the release includes matching source and SHA-256 checksums.
there is no published npm package.

these checks do not establish exact parity or coverage of every input.
v0.1.2 improves text-field accessibility, but full screen-reader support remains unverified.
see the [release coverage limits](https://github.com/Microck/openvector/releases/tag/v0.1.2)
and [verification and known limits](docs/verification.md).

## build from source

use node.js 22.19 or later, npm, rust with its `rust-docs` component, git, and a c
compiler/linker. native packaging also needs platform-specific toolchain notices.
the [build guide](docs/building.md) covers supported targets, cross-compilation,
linux libraries, and the windows node-api linker input.

```sh
git clone https://github.com/Microck/openvector.git
cd openvector
npm ci
npm run build
node bin/openvector.mjs --help
```

`npm run build` produces the cli and node addon for the selected platform.
it does not build the desktop app.

```sh
cargo run --manifest-path rust/Cargo.toml --locked -p openvector-gui -- logo.png
```

the desktop app needs a graphical session. use `OPENVECTOR_PREFERENCES` to select a
separate preferences file when testing.

## convert an image

```sh
node bin/openvector.mjs vectorize logo.png --output logo.svg
node bin/openvector.mjs vectorize logo.png --output logo.pdf
mkdir vectors
node bin/openvector.mjs batch first.png second.jpg --format svg --output-dir vectors
```

the output suffix selects the format. existing outputs require an explicit
replacement option. processing does not rewrite the source image.

## use it from node.js

after building the native addon:

```js
import { readFileSync } from 'node:fs';
import { CoreSession } from 'openvector';

const document = new CoreSession().createDocument();
document.setSource(readFileSync('logo.png'), 'logo.png');
document.runAutomatic({ extension: '.png' });
// saveVector creates a new file and never overwrites an existing path.
const saved = document.saveVector('logo.svg', { layout: 'cutouts' });
if (saved.status !== 0 || saved.error) {
  throw new Error(saved.error?.message ?? `Export failed: ${saved.status}`);
}
```

calls are synchronous. the [api guide](docs/usage.md) covers progress, cancellation,
batches, editing, snapshots, and profile storage. the package never downloads a
binary or falls back to the javascript reference implementation on import.

## development

```sh
npm run test:smoke
```

that command runs small reference checks and independently authored tests included
in the default checkout. it is also the test scope of [GitHub Actions source CI](docs/ci.md),
not a claim that the full native or desktop suites pass. see
[development](docs/development.md) and [test fixtures](docs/test-fixtures.md) for
the broader checks and their private fixture corpus.

| directory | contents |
| --- | --- |
| `rust/crates/openvector` | shared tracing engine |
| `rust/crates/openvector-gui` | native desktop app |
| `rust/crates/openvector-cli` | cli |
| `rust/crates/openvector-node`, `npm/` | node-api binding and public declarations |
| `src/`, `gui/` | development reference implementation, models, and HTTP test host |
| `test/`, `docs/` | tests, fixtures, usage guides, and behavioral contracts |

## license

application code is [mit licensed](LICENSE). third-party code, fonts, and icons
retain their own licenses. packages must include their generated notices and
editable source where required. qt-derived code has lgpl requirements; review the
[distribution notes](docs/building.md) before a public binary release.

openvector is an independent project and is not affiliated with vector magic.
