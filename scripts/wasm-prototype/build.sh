#!/usr/bin/env bash
# Build both harnesses from the checkout's core and resolved crate versions.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out=$(realpath -m "${1:?pass an output directory}")
mkdir -p "$out"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$out/target}"
stage=$(mktemp -d)
trap 'rm -r "$stage"' EXIT
cd "$stage"
mkdir -p rust/crates/harness src
cp -R "$root/rust/crates/openvector" rust/crates/
cp "$root"/src/{parameter-data,segmentation-progress-data}.json src/
cp "$root/rust/Cargo.lock" rust/
sed -n '/^\[workspace.package\]/,/^$/{p;}' "$root/rust/Cargo.toml" > rust/Cargo.toml
cat >> rust/Cargo.toml <<'TOML'
[workspace]
resolver = "2"
members = ["crates/openvector", "crates/harness"]
TOML
cp "$here"/{trace,editor}.rs rust/crates/harness/
cat > rust/crates/harness/Cargo.toml <<'TOML'
[package]
name = "trace-prototype"
version.workspace = true
edition.workspace = true
[[bin]]
name = "trace-prototype"
path = "trace.rs"
[dependencies]
openvector = { path = "../openvector" }
serde_json = "1"
TOML
cd rust
cargo build --release -p trace-prototype
cp "$CARGO_TARGET_DIR"/release/trace-prototype "$out/native-trace"
cargo rustc --release -p trace-prototype --target wasm32-unknown-emscripten -- \
  -C link-arg=-sMODULARIZE=1 -C link-arg=-sEXPORT_NAME=createTracer \
  -C link-arg=-sEXPORTED_FUNCTIONS=_convert_png,_editor_command,_main -C link-arg=-sEXPORTED_RUNTIME_METHODS=FS \
  -C link-arg=-sALLOW_MEMORY_GROWTH=1 -C link-arg=-sSTACK_SIZE=4194304 \
  -C link-arg=-sENVIRONMENT=worker -C link-arg=-sSINGLE_FILE=1
python3 - "$here" "$out" <<'PY'
import json, os, pathlib, sys
here, out = map(pathlib.Path, sys.argv[1:])
engine = (pathlib.Path(os.environ['CARGO_TARGET_DIR']) / 'wasm32-unknown-emscripten/release/trace-prototype.js').read_text()
worker = engine + '\n' + (here / 'worker.js').read_text()
(out / 'tracer-worker.js').write_text(engine + '\n' + (here / 'editor-worker.js').read_text())
page = (here / 'page.html').read_text().replace('/* WORKER */ null', json.dumps(worker).replace('<', '\\u003c'))
(out / 'openvector.html').write_text(page)
PY
