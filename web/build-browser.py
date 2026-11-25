"""Build the desktop GUI against the browser's JavaScript and WASI hosts."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

web = Path(__file__).resolve().parent
workspace = web.parent / "rust"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--out", type=Path, default=web / "dist")
output = parser.parse_args().out.absolute()
if output.is_symlink():
    parser.error("the output directory cannot be a symbolic link")
destination = output.resolve()
# Serialize writers to wasm-bindgen's shared generated/ directory.
build_lock = Path(__file__).open()
fcntl.flock(build_lock, fcntl.LOCK_EX)
if destination.exists() and destination != web / "dist":
    parser.error("--out must name a new directory")
sdk = Path(os.environ["WASI_SDK_PATH"]).resolve()
sysroot = sdk / "share/wasi-sysroot"
target = Path(os.environ.get("CARGO_TARGET_DIR", workspace / "target")).resolve()
os.environ.update(PATH=f"{sdk}/bin:{os.environ['PATH']}", WASI_SYSROOT=str(sysroot),
                  CC_wasm32_wasip1_threads=str(sdk / "bin/wasm32-wasip1-threads-clang"),
                  CFLAGS_wasm32_wasip1_threads="-pthread -mllvm -wasm-enable-sjlj -mexception-handling",
                  CARGO_TARGET_DIR=str(target))
os.environ["RUSTUP_TOOLCHAIN"] = "nightly-2026-11-26"
cargo = ["cargo"]
metadata = json.loads(subprocess.check_output(
    cargo + ["metadata", "--locked", "--format-version", "1", "--filter-platform", "wasm32-wasip1-threads"], cwd=workspace))

# The browser supplies JS imports alongside WASI. Enable the ordinary wasm JS
# bindings in this build only. Keep Cargo's native sources and lockfile intact.
versions = {"wasm-bindgen": "0.2.129", "wasm-bindgen-macro-support": "0.2.129", "js-sys": "0.3.106"}
gate = 'all(target_family = "wasm", not(target_os = "wasi"))'
with tempfile.TemporaryDirectory(prefix="browser-build-", dir=workspace) as temporary:
    stage = Path(temporary)
    manifest = (workspace / "Cargo.toml").read_text()
    shutil.copy(workspace / "Cargo.lock", stage)
    for directory in ("crates", "vendor"):
        (stage / directory).symlink_to(workspace / directory, target_is_directory=True)
    patches = []
    cli = subprocess.check_output(["wasm-bindgen", "--version"], text=True).strip()
    if cli != "wasm-bindgen " + versions["wasm-bindgen"]:
        raise RuntimeError(f"Install wasm-bindgen-cli {versions['wasm-bindgen']} (found {cli})")
    for name, version in versions.items():
        package, = (p for p in metadata["packages"] if p["name"] == name and p["version"] == version)
        source = stage / name
        shutil.copytree(Path(package["manifest_path"]).parent, source)
        changed = 0
        for path in (source / "src").rglob("*.rs"):
            code = path.read_text()
            if gate in code:
                path.write_text(code.replace(gate, 'target_family = "wasm"'))
                changed += 1
        if not changed:
            raise RuntimeError(f"{name} no longer contains the pinned browser target gate")
        patches.append(f'{name} = {{ path = "{name}" }}')
    (stage / "Cargo.toml").write_text(manifest.replace("[patch.crates-io]", "[patch.crates-io]\n" + "\n".join(patches)))
    # wasm-bindgen initializes a stack and TLS for each JS worker instance.
    exports = ("wasi_thread_start", "__wasm_init_tls", "__tls_size", "__tls_align", "__tls_base", "__heap_base")
    linker_args = ["-C", f"link-arg={sysroot}/lib/wasm32-wasip1-threads/libsetjmp.a"]
    for symbol in exports:
        linker_args.extend(["-C", f"link-arg=--export={symbol}"])
    build_log = subprocess.check_output(cargo + ["rustc", "--message-format=json-render-diagnostics", "--release", "--target", "wasm32-wasip1-threads", "-p", "openvector-gui",
                            "--lib", "--", *linker_args], cwd=stage, text=True)
    artifacts = [message for line in build_log.splitlines() if (message := json.loads(line))["reason"] == "compiler-artifact"]
    resolved = json.loads(subprocess.check_output(
        cargo + ["metadata", "--locked", "--format-version", "1", "--filter-platform", "wasm32-wasip1-threads"], cwd=stage))
    subprocess.run(["wasm-bindgen", "--target", "web", "--no-typescript", "--out-name", "openvector",
                    "--out-dir", web / "generated", target / "wasm32-wasip1-threads/release/openvector_gui.wasm"], check=True)
    # Cargo's patched sources and their licenses live in stage until bundling finishes.
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent) as temporary:
        staged = Path(temporary) / "site"
        subprocess.run(["node", "bundle.mjs", staged], cwd=web, check=True,
                       input=json.dumps([artifacts, resolved]), text=True)
        if destination == web / "dist" and destination.exists():
            destination.rename(Path(temporary) / "previous")
        staged.rename(destination)
