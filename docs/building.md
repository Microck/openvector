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
Debian-based build host, plus ARM64 and x64 macOS. GNU targets require `dpkg-query`,
the Debian copyright files and common license texts installed with the target
C runtime packages, and a linker that can locate their runtime objects and
archives. The retained ARM64 trace and the [Intel GUI linker trace](https://github.com/Microck/openvector/actions/runs/42129483005/job/127604265369)
use dynamic macOS system libraries and ship no static C runtime object;
Rust and dependency notices still apply. Set
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
and arm64 GNU/Linux, macOS and Windows targets. Packaging MSVC Windows binaries
stops before compilation because their C runtime notices are not implemented.
ARM64 macOS CLI/addon builds and installed runtime checks passed previously; they have not been reverified for this cleaned source.
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

These notices do not by themselves satisfy the source and relinking terms for
the statically linked LGPL code. The release source archive includes the editable
Qt-derived Rust module, application source, local GPUI patches and locked Cargo
dependencies. Its `REBUILD.txt` explains how to change that module and rebuild
the application offline. This is a whole-application rebuild, not a replaceable
runtime library or a separate kit of prebuilt object files.

An offline GUI and CLI rebuild with a modified module passed the
[rebuild checks](verification.md). That verifies the procedure, not legal
clearance for third-party material. Preserve the component licenses and
modification notices when redistributing a build.

## Build and run the native GUI

The GPUI application links the same `openvector` Rust crate as the CLI and
Node addon.

On Ubuntu 24.04, install the packages from the candidate workflow's
[Install Linux build dependencies step](../.github/workflows/desktop-candidates.yml#L89-L93)
before building. That list includes the compiler tools, desktop development
libraries and Vulkan/X11 runtime used for GUI checks. Install these system
packages and Rust before disconnecting for an offline rebuild; the source
archive vendors Cargo dependencies, not operating-system packages.

Build the GUI separately from the npm package:

```sh
cargo run --manifest-path rust/Cargo.toml --locked -p openvector-gui
cargo run --manifest-path rust/Cargo.toml --locked -p openvector-gui -- logo.png
```

Run it in a graphical desktop session. The Linux build enables X11 and
Wayland; live verification uses X11 with a Vulkan driver and a working
desktop file-chooser portal. Wayland remains unverified. Native conversion checks
passed on Apple Silicon and Intel Macs. Windows packages passed on Windows 10
x64 and on Windows 11 ARM64 through x64 emulation. A non-interactive SSH window
station cannot open the Windows GPUI window. See [verification](verification.md)
for the tested workflows and remaining gaps.

The GUI stores its preferences, including recent basic settings, in
`$XDG_CONFIG_HOME/openvector/gui.json`, or `$HOME/.config/openvector/gui.json`
when `XDG_CONFIG_HOME` is unset. Saved advanced profiles live in
`profiles.json` beside that file. For an isolated verification run, set
`OPENVECTOR_PREFERENCES` to a separate JSON file.
The headed smoke check uses the retained reference corpus. Install the fixtures
described in [test fixtures](test-fixtures.md) before running:

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openvector-gui \
  --profile dev --test headed_smoke -- --ignored --nocapture
```

This check needs a display and Vulkan support. See the [parity matrix](parity-matrix.md)
for historical coverage and known gaps. A successful build does not verify every workflow.


## Package the desktop application

Use the existing packager from the repository root. The destination must not already exist. Replace the target with the one you are building and supply its compiler and linker.

```sh
node tools/package-gui.mjs --target x86_64-pc-windows-gnu dist/openvector-windows-x64
```

Create `dist/` first. The packager builds the GUI and includes its application, font, icon, Rust and toolchain notices. It does not produce an installer or sign the executable. See the [release runbook](release-runbook.md) before distributing an archive.

To wrap a staged Windows x64 directory in both a portable ZIP and a per-user
installer, install NSIS and `zip` on the Linux cross-build host, then run:

```sh
node tools/package-windows.mjs dist/openvector-windows-x64 dist/windows-downloads
```

The output directory must be new. The installer adds Start menu shortcuts and
an uninstall entry without requiring administrator rights. Its uninstaller
removes only packaged files and preserves unrelated files and user preferences.
Neither download is Authenticode-signed by this command.

On macOS, license files are inside `OpenVector.app/Contents/Resources` so they
remain with the app when it is copied out of a disk image. The staging command
adds only an ad-hoc signature for local verification.

The desktop candidate workflow builds from a corresponding-source archive,
including the locked Cargo dependencies. Create that archive from a clean
Git checkout on Linux. The packaging script requires GNU `realpath -m`;
the macOS system `realpath` is not supported:

```sh
mkdir -p dist
bash tools/package-release-source.sh dist/openvector-source.tar.gz
```

Its `REBUILD.txt` identifies the editable LGPL component and the offline Cargo
rebuild command. The archive does not include the compiler or operating-system
SDK. Validate the rebuild and the distribution requirements before publishing.

Windows builds embed the app icon and version information in `openvector-gui.exe`; the window, taskbar and Explorer all read that resource. GNU targets need MinGW `windres` (for example `x86_64-w64-mingw32-windres`, override with `WINDRES`) and MSVC targets need `rc.exe` (override with `RC`). The build fails if neither is found. Linux packages add `share/applications/openvector.desktop` and `share/icons/hicolor/256x256/apps/openvector.png`; copy `share/` into `~/.local/share` and put `openvector-gui` on `PATH` so Wayland desktops match the window (app id `openvector`) to its icon. macOS packages contain `OpenVector.app` (bundle id `dev.micr.openvector`) with the compiled icon catalog `Contents/Resources/Assets.car` and an `OpenVector.icns` fallback. Packaging on a Mac seals it with an ad-hoc `codesign` signature for local use; it is not Developer ID signed or notarized, so other Macs show a Gatekeeper warning. The icon sources are `rust/crates/openvector-gui/assets/app-icon.svg` and the Icon Composer document `assets/OpenVector.icon`; `build.rs` documents how to regenerate `app-icon.ico`, `app-icon-256.png` and the compiled macOS files in `assets/macos/`.
