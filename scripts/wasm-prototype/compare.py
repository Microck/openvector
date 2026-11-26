"""Compare built prototype directories against one frozen CLI output per PNG."""
import argparse
from contextlib import ExitStack
from functools import partial
from hashlib import sha256
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import random
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory
from threading import Thread

from playwright.sync_api import sync_playwright

from browser import browser_trace

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--cli", type=Path, required=True)
parser.add_argument("--native", type=Path, required=True)
parser.add_argument("--samples", type=int, default=3)
parser.add_argument("--build", type=Path, action="append", required=True)
parser.add_argument("images", type=Path, nargs="+")
args = parser.parse_args()
if args.samples < 1:
    parser.error("--samples must be positive")
if len({build.name for build in args.build}) != len(args.build):
    parser.error("Build directory names must be unique")


with ExitStack() as stack:
    scratch = Path(stack.enter_context(TemporaryDirectory()))
    web = scratch / "web"
    web.mkdir()
    native_output = scratch / "native.svg"
    playwright = stack.enter_context(sync_playwright())
    browser = playwright.chromium.launch(executable_path="/usr/bin/chromium")
    stack.callback(browser.close)
    server = stack.enter_context(ThreadingHTTPServer(("0.0.0.0", 0), partial(SimpleHTTPRequestHandler, directory=web)))
    Thread(target=server.serve_forever, daemon=True).start()
    stack.callback(server.shutdown)
    pages, profiles = {}, {}
    for build in args.build:
        name = build.name
        shutil.copyfile(build / "openvector.html", web / f"{name}.html")
        page = pages[name] = browser.new_page()
        profiles[name] = []
        def record(message, name=name):
            if message.text.startswith("STAGES "):
                profiles[name].append(dict(json.loads(message.text[7:])))
        page.on("console", record)
        page.goto(f"http://127.0.0.1:{server.server_port}/{name}.html")
        page.wait_for_function("document.querySelector('#status').textContent === 'Ready'")
    report = {"browser": browser.version, "samples": args.samples, "cases": []}
    for image in args.images:
        oracle = subprocess.check_output([args.cli, "vectorize", image])
        native_output.unlink(missing_ok=True)
        native = subprocess.run([args.native, image, native_output, str(args.samples+1)],
                                capture_output=True, text=True, check=True)
        if native_output.read_bytes() != oracle:
            raise ValueError(f"{image}: native harness differs from released CLI")
        native_ms = list(map(float, native.stdout.splitlines()))
        if len(native_ms) != args.samples + 1:
            raise ValueError(f"{image}: incomplete native timings")
        case = dict(image=image.name, input_sha256=sha256(image.read_bytes()).hexdigest(),
                    svg_sha256=sha256(oracle).hexdigest(), bytes=len(oracle),
                    native_ms=native_ms,
                    wasm_ms={name: [] for name in pages}, order=[])
        for stages in profiles.values():
            stages.clear()
        # Keep one untimed-for-summary warmup per page. Rotate which build
        # runs first on successive rounds, on the same otherwise idle guest.
        order = list(pages)
        random.Random(image.name).shuffle(order)
        for i in range(args.samples+1):
            rotated = order[i % len(order):] + order[:i % len(order)]
            case["order"].append(rotated)
            for name in rotated:
                ms, svg = browser_trace(pages[name], image)
                if svg != oracle:
                    raise ValueError(f"{image} ({name}): expected {sha256(oracle).hexdigest()}, got {sha256(svg).hexdigest()}")
                case["wasm_ms"][name].append(ms)
        if any(profiles.values()):
            case["browser_stages_ms"] = {k: v.copy() for k, v in profiles.items() if v}
            case["native_stages_ms"] = [dict(json.loads(s[7:])) for s in native.stderr.splitlines() if s.startswith("STAGES ")]
        case["byte_identical_downloads"] = (args.samples + 1) * len(pages)
        report["cases"].append(case)
        print(f"verified {image.name}: {case['wasm_ms']}", file=sys.stderr, flush=True)
    print(json.dumps(report, indent=2))
