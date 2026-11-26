#!/usr/bin/env bash
# Package both Linux formats from the same staged binary and notices as the tarball.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo 'Usage: bash tools/package-linux.sh <staged Linux GUI directory> <output filename prefix>' >&2
  exit 1
fi

stage=$(realpath "$1")
output=$(realpath -m "$2")
runtime_notices=$(realpath -e "$(dirname "$0")/../appimage-runtime/licenses")
app_id=dev.micr.openvector
runtime=org.freedesktop.Platform
runtime_branch=25.08
appimagetool_version=1.9.1
appimage_runtime_version=20251108
arch=$(uname -m)
case "$arch" in
  x86_64)
    tool_sha=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
    runtime_sha=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
    ;;
  aarch64)
    tool_sha=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
    runtime_sha=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
    ;;
  *)
    echo "Unsupported Linux package architecture: $arch" >&2
    exit 1
    ;;
esac

temp=$(mktemp -d)
trap 'rm -r "$temp"' EXIT
cd "$temp"

download() {
  local url=$1 filename=$2 digest=$3
  curl --fail --location --silent --show-error "$url" --output "$filename"
  printf '%s  %s\n' "$digest" "$filename" | sha256sum --check
}

desktop-file-validate "$stage/share/applications/$app_id.desktop"
appstreamcli validate --no-net "$stage/share/metainfo/$app_id.metainfo.xml"

download "https://github.com/AppImage/appimagetool/releases/download/$appimagetool_version/appimagetool-$arch.AppImage" appimagetool "$tool_sha"
download "https://github.com/AppImage/type2-runtime/releases/download/$appimage_runtime_version/runtime-$arch" appimage-runtime "$runtime_sha"
chmod +x appimagetool

cp -a "$stage" OpenVector.AppDir
mkdir OpenVector.AppDir/usr
mv OpenVector.AppDir/share OpenVector.AppDir/usr/
ln -s openvector-gui OpenVector.AppDir/AppRun
ln -s "usr/share/applications/$app_id.desktop" "OpenVector.AppDir/$app_id.desktop"
ln -s "usr/share/icons/hicolor/256x256/apps/$app_id.png" "OpenVector.AppDir/$app_id.png"
ln -s "$app_id.png" OpenVector.AppDir/.DirIcon
cp -a "$runtime_notices" OpenVector.AppDir/licenses/appimage-runtime
# AppStream was validated above. This tool only recognizes the older .appdata.xml name.
ARCH=$arch APPIMAGE_EXTRACT_AND_RUN=1 ./appimagetool --no-appstream --runtime-file appimage-runtime OpenVector.AppDir "$output.AppImage"

# The staged executable is already compiled, so build-init can use the platform
# as both the SDK and the runtime. No SDK packages are needed to bundle it.
flatpak build-init flatpak-app "$app_id" "$runtime" "$runtime" "$runtime_branch"
install -Dm755 "$stage/openvector-gui" flatpak-app/files/bin/openvector-gui
cp -a "$stage/share/." flatpak-app/files/share/
install -Dm644 "$stage/LICENSE" "flatpak-app/files/share/licenses/$app_id/LICENSE"
cp -a "$stage/licenses/." "flatpak-app/files/share/licenses/$app_id/"
flatpak build-finish --command=openvector-gui --share=ipc --socket=wayland \
  --socket=fallback-x11 --device=dri flatpak-app
flatpak build-export flatpak-repo flatpak-app stable
flatpak build-bundle --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo \
  flatpak-repo "$output.flatpak" "$app_id" stable
