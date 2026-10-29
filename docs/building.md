# Building and packaging

## Build the native npm package

Use Node.js 22.19 or later and a current stable Rust toolchain with Cargo,
Git, and a C compiler/linker. Install the toolchain's `rust-docs` component;
notice collection reads its copyright and license files. Rust compiles the
bundled zlib 1.2.3 and IJG jpeg-6b C sources. A cross-build also needs the
selected Rust target, its C toolchain and linker, and any Node-API link
configuration required by that target. Dependency resolution needs access
to the locked crates and Git sources, including workspace GUI dependencies.
For the GNU Windows Node binding, set `LIBNODE_PATH` to the directory containing
the target's `libnode.dll` import/link input. The pinned `napi-build` requires
that file even when cross-compiling; it also searches `LIBPATH` and `PATH`.

The notice collector supports GNU/Linux and MinGW Windows targets on a
Debian-based build host, plus ARM64 macOS. GNU targets require `dpkg-query`,
the Debian copyright files and common license texts installed with the target
C runtime packages, and a linker that can locate their runtime objects and
archives. The retained ARM64 macOS link trace uses dynamic system libraries
and ships no static C runtime object; Rust and dependency notices still apply. Set
`CARGO_TARGET_<TRIPLE>_LINKER` for a custom linker, with the triple uppercased
and hyphens replaced by underscores. The collector does not read a linker
set only in Cargo config. Its retained standard-library license supplements
cover the omissions observed with Rust 1.99.1; another toolchain can require
additional license texts and fails if they are missing.

From the source checkout or the root of an extracted npm archive, in the
authorized build environment:

```sh
npm run build
npm pack
```

`npm pack` runs the same release build through `prepack`. It selects only
`openvector-cli` and `openvector-node`, not the GPUI application. Cargo uses
`rust/Cargo.lock` with `--locked`. Its configured target directory, including
`CARGO_TARGET_DIR`, is respected; a relative target directory is resolved
from the `rust` workspace. For example:

```sh
CARGO_TARGET_DIR=/tmp/openvector-target npm run build
npm run build -- --target x86_64-pc-windows-gnu
CARGO_BUILD_TARGET=x86_64-pc-windows-gnu npm pack
```

The default target is the Rust toolchain's host. Set `CARGO_BUILD_TARGET` or
pass `--target` to select another target. The target parser recognizes x64
and arm64 GNU/Linux, macOS and Windows targets. Packaging x64 macOS or MSVC
Windows binaries stops before compilation because their C runtime notices
are not implemented. ARM64 macOS CLI/addon builds and installed runtime checks
pass in the [retained Darwin verification](../evidence/full-parity-program-20261022/macos/verification.json).
This does not establish macOS GUI parity. Other recognized targets still need
a working toolchain; recognition does not establish that they have been tested.
The verified Linux ARM64 binaries import GNU libc symbols through
`GLIBC_2.34` and require `libgcc_s.so.1`; musl builds are not interchangeable.
This ABI inspection does not verify every Linux distribution. Each target is staged under
`native/<platform>-<arch>/`, with Node's platform names `linux`, `darwin` and
`win32`. That directory contains `openvector-node.node` and `openvector`
or `openvector.exe`. Previously staged targets remain available for a
multi-platform archive; start with a clean staging directory when building
a single-platform archive.

The private archive includes those native artifacts and notices, the npm
loader and declarations, CLI launcher, this README, the MIT license and
package metadata. It also keeps the native build inputs at their existing
paths:

- `rust/Cargo.toml` and `rust/Cargo.lock`.
- `rust/crates/openvector/{Cargo.toml,build.rs,src/,csrc/,vendor/}`, including
  the Rust translations and bundled C sources, headers and vendor notices.
- `rust/crates/openvector-cli/{Cargo.toml,src/}` and
  `rust/crates/openvector-node/{Cargo.toml,build.rs,src/}`.
- `rust/crates/openvector-gui/{Cargo.toml,src/,assets/}` and `rust/vendor/`,
  retaining the workspace member and local GPUI patches for Cargo resolution.
  The npm build does not compile or stage the GUI executable.
- `src/parameter-data.json`, `src/segmentation-progress-data.json`,
  `src/jpeg-licenses.txt` and `src/tiff-licenses.txt`.
- `tools/build-native.mjs`, `tools/native-notices.mjs`,
  `tools/rust-std-licenses.json` and `tools/rust-upstream-licenses.json`.

This is source for rebuilding the native CLI and Node library, with locked
external Cargo dependencies available. It is not an offline dependency
bundle. The archive excludes target caches, the JavaScript engine modules,
browser preview, OpenVector's integration tests and fixture corpus,
investigation tools and evidence.
There is no install hook, automatic download, build-on-import, or engine fallback.
Install an archive containing your platform's artifacts. Missing artifacts
require an explicit `npm run build` from the extracted package root in a
supported build environment. No checkout or Vector Magic installation is
needed for that rebuild. The archive does not contain the full development
test suite; run those checks from the checkout.
The package remains private and is not published.

The build writes license notices for the compiled Rust crates and toolchain
code beside each platform's binaries, in `rust-licenses.txt`,
`toolchain-licenses.txt` and `rust-std-COPYRIGHT-library.html`. The build
also copies third-party notices into `native/licenses/`:

- zlib 1.2.3 and IJG jpeg-6b C sources, whose notices the build copies to
  `native/licenses/`. This software is based in part on the work of the
  Independent JPEG Group.
- A Rust translation of the FreeType gray rasterizer, under the FreeType
  Project License, with `freetype-raster-FTL.txt` and modification notes.
  This software is based in part on the work of the FreeType Team.
- A Rust translation of selected Qt 4.5.3 QBezier and QStroker routines for
  bitmap seam strokes, under LGPL 2.1. The license, Qt LGPL exception and
  modification notes use the `qt-bezier-` filename prefix.
- JPEG decoding code derived from jpeg-js and IJG jpeg-6b, in
  `jpeg-decoder-licenses.txt`, and TIFF codec code derived from UTIF.js and
  libtiff, in `tiff-decoder-licenses.txt`.

These notices do not by themselves satisfy the source and relinking terms
for the statically linked LGPL code. The archive now includes the editable
Qt-derived Rust module and the application source and build inputs listed
above. Modifying that module requires rebuilding each affected native
binary; it is not a runtime-replaceable library. The archive supplies no
separate relinking kit or prebuilt application object files. Source
inclusion does not establish legal compliance; the remaining distribution
requirements are unresolved before any public binary release.

## Build and run the native GUI

The GPUI application links the same `openvector` Rust crate as the CLI and
Node addon. Build it separately from the npm package:

```sh
cargo run --manifest-path rust/Cargo.toml --locked -p openvector-gui
cargo run --manifest-path rust/Cargo.toml --locked -p openvector-gui -- logo.png
```

Run it in a graphical desktop session. The Linux build enables X11 and
Wayland; live verification uses X11 with a Vulkan driver and a working
desktop file-chooser portal. Wayland remains unverified. ARM64 macOS uses
the pinned font-kit backend; native Open, Save, clipboard, export drag and
viewer interactions pass, but one worker export test remains unresolved.
Windows builds and earlier interactive-desktop workflows pass; current
binary verification is tracked separately in the parity matrix. A
non-interactive SSH window station cannot open the GPUI window.

The GUI stores its preferences, including recent basic settings, in
`$XDG_CONFIG_HOME/openvector/gui.json`, or `$HOME/.config/openvector/gui.json`
when `XDG_CONFIG_HOME` is unset. Saved advanced profiles live in
`profiles.json` beside that file. For an isolated verification run, set
`OPENVECTOR_PREFERENCES` to a separate JSON file.
The headed smoke check opens a real window, processes a captured image and
compares its exported SVG with the original reference:

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openvector-gui \
  --profile dev --test headed_smoke -- --ignored --nocapture
```

This check needs a display and Vulkan support. See the
[parity matrix](parity-matrix.md) for exercised workflows, platform
limits and known failing comparisons. A successful build is not full parity.


## Package the desktop application

Use the existing packager from the repository root. The destination must not already exist. Replace the target with the one you are building and supply its compiler and linker.

```sh
node tools/package-gui.mjs --target x86_64-pc-windows-gnu dist/openvector-windows-x64
```

Create `dist/` first. The packager builds the GUI and includes its application, font, icon, Rust and toolchain notices. It does not produce an installer or sign the executable. See the [release runbook](release-runbook.md) before distributing an archive.
