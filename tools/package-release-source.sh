#!/usr/bin/env bash
# Package the exact committed sources and locked Cargo dependencies for rebuilds.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ "$#" -ne 1 ]; then
  echo 'Usage: bash tools/package-release-source.sh <new .tar.gz file>' >&2
  exit 1
fi
output=$(realpath -m "$1")
test ! -e "$output"
git diff --quiet HEAD --
version=$(node -p 'JSON.parse(require("fs").readFileSync("package.json", "utf8")).version')
commit=$(git rev-parse HEAD)
name="openvector-$version-source"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
mkdir "$temporary/$name"
git archive HEAD | tar -xf - -C "$temporary/$name"
(
  cd "$temporary/$name"
  mkdir -p .cargo
  cargo vendor --locked --manifest-path rust/Cargo.toml cargo-vendor > .cargo/config.toml
  cargo metadata --locked --offline --manifest-path rust/Cargo.toml --format-version 1 > /dev/null
  printf '%s\n' "$commit" > SOURCE-COMMIT
  cat > REBUILD.txt <<'INSTRUCTIONS'
This archive contains the application source, local GPUI patches, and vendored
Cargo dependencies from the exact commit recorded in SOURCE-COMMIT.

Install Rust 1.99.1 and your platform's native compiler, linker and desktop
library development packages. The compiler and operating-system SDK are not
included. Platform requirements are documented in docs/building.md.

To modify the LGPL Qt-derived component, edit:
  rust/crates/openvector/src/bitmap_raster/square_stroke.rs
Its license and modification notices are under:
  rust/crates/openvector/vendor/qt-bezier/

From this archive's root, rebuild the desktop application with:
  cargo build --manifest-path rust/Cargo.toml --locked --offline --release -p openvector-gui
The executable is in rust/target/release unless CARGO_TARGET_DIR overrides it.

You may modify this component and rebuild the application for your own use.
The application license does not prohibit reverse engineering to debug those
modifications. Preserve third-party licenses when redistributing your build.
A modified macOS application needs a new signature; a local ad-hoc signature
is sufficient for local testing, not a Developer ID distribution signature.
INSTRUCTIONS
)
tar -czf "$output" -C "$temporary" "$name"
printf 'Packaged corresponding source for %s in %s\n' "$commit" "$output"
