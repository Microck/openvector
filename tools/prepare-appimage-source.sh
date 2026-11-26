#!/usr/bin/env bash
# Preserve the upstream runtime, its libfuse patch and the component notices for distribution.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo 'Usage: bash tools/prepare-appimage-source.sh <new AppImage source directory>' >&2
  exit 1
fi

mkdir "$1"
cd "$1"
mkdir licenses

while read -r digest filename url; do
  curl --fail --location --silent --show-error "$url" --output "$filename"
  printf '%s  %s\n' "$digest" "$filename" | sha256sum --check
done <<'INPUTS'
f5fec23be76e50e2445ed2d018bac49b367490fc483c62c4637e99ec705d27ba type2-runtime.tar.gz https://codeload.github.com/AppImage/type2-runtime/tar.gz/dd6cebedcbddde9c82f89b011e8e1d40b6e43868
70589cfd5e1cff7ccd6ac91c86c01be340b227285c5e200baa284e401eea2ca0 fuse-3.15.0.tar.xz https://github.com/libfuse/libfuse/releases/download/fuse-3.15.0/fuse-3.15.0.tar.xz
db0238c5981dabbd80ee09ae15387f390091668ca060a7bc38047912491443d3 squashfuse-0.5.2.tar.gz https://codeload.github.com/vasi/squashfuse/tar.gz/refs/tags/0.5.2
a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4 musl-1.2.5.tar.gz https://musl.libc.org/releases/musl-1.2.5.tar.gz
7055266497633c9025b777c78eb7235af13922117480ed5c674677adc381c9d8 licenses/zstd-LICENSE.txt https://raw.githubusercontent.com/facebook/zstd/v1.5.6/LICENSE
845efc77857d485d91fb3e0b884aaa929368c717ae8186b66fe1ed2495753243 licenses/zlib-LICENSE.txt https://raw.githubusercontent.com/madler/zlib/v1.3.1/LICENSE
19c99805e7a44a34b297a75d1edea9985e300066dfc024d5c99d4236d4573b5d licenses/mimalloc-LICENSE.txt https://raw.githubusercontent.com/microsoft/mimalloc/v2.1.7/LICENSE
INPUTS

tar -xOf type2-runtime.tar.gz --wildcards '*/LICENSE' > licenses/type2-runtime-LICENSE.txt
tar -xOf fuse-3.15.0.tar.xz fuse-3.15.0/LICENSE fuse-3.15.0/AUTHORS > licenses/libfuse-NOTICE.txt
tar -xOf fuse-3.15.0.tar.xz fuse-3.15.0/LGPL2.txt > licenses/libfuse-LGPL2.txt
tar -xOf fuse-3.15.0.tar.xz fuse-3.15.0/GPL2.txt > licenses/libfuse-GPL2.txt
tar -xOf squashfuse-0.5.2.tar.gz squashfuse-0.5.2/LICENSE > licenses/squashfuse-LICENSE.txt
tar -xOf musl-1.2.5.tar.gz musl-1.2.5/COPYRIGHT > licenses/musl-COPYRIGHT.txt
cat > licenses/SOURCE.txt <<'SOURCE'
The OpenVector release's corresponding source archive contains the AppImage
runtime source and dependency archives in appimage-runtime/. The libfuse
release archive is fuse-3.15.0.tar.xz, and the matching runtime patch is in
patches/libfuse/mount.c.diff inside type2-runtime.tar.gz. The upstream build
applies that patch and links libfuse into the rebuilt runtime.

Extract type2-runtime.tar.gz, then run from the extracted runtime source on
a native amd64 or arm64 Linux host with Docker:

  mkdir -p out
  (cd out && ARCH=$(uname -m) bash ../scripts/docker/build-with-docker.sh)

The resulting runtime is out/runtime-x86_64 or out/runtime-aarch64.
See BUILD.md in the runtime source for more about its build environment.

Use appimagetool with --runtime-file to combine a rebuilt runtime with the
application payload extracted by running the OpenVector AppImage with
--appimage-extract.

AppImage runtime upstream source:
https://github.com/AppImage/type2-runtime/tree/dd6cebedcbddde9c82f89b011e8e1d40b6e43868
SOURCE
