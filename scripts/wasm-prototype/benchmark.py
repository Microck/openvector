"""Usage: python benchmark.py BUILD_DIR RELEASE_CLI INPUT.png [INPUT.png ...].

Requires Playwright for Python and Chromium. Writes raw samples to stdout.
"""
from functools import partial
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
from threading import Thread
from time import perf_counter

from playwright.sync_api import sync_playwright

from browser import browser_trace

build, cli, *images = map(Path, sys.argv[1:])
repetitions = int(os.environ.get("OV_SAMPLES", "5"))
with tempfile.TemporaryDirectory() as scratch, sync_playwright() as playwright:
    browser = playwright.chromium.launch(executable_path=os.getenv("CHROMIUM", "/usr/bin/chromium"))
    page = browser.new_page(accept_downloads=True)
    # Serve the benchmark page over HTTP, as it would be served on the web.
    Path(scratch, "index.html").write_bytes((build / "openvector.html").read_bytes())
    http = ThreadingHTTPServer(("0.0.0.0", 0), partial(SimpleHTTPRequestHandler, directory=scratch))
    Thread(target=http.serve_forever, daemon=True).start()
    start = perf_counter()
    page.goto(f"http://127.0.0.1:{http.server_port}/")
    page.wait_for_function("document.querySelector('#status').textContent === 'Ready'")
    report = dict(browser=browser.version, arch=platform.machine(),
                  local_page_to_ready_ms=(perf_counter()-start)*1000, cases=[])
    for image in images:
        oracle = subprocess.check_output([str(cli.resolve()), "vectorize", str(image)])
        oracle_hash = hashlib.sha256(oracle).hexdigest()
        output = Path(scratch) / "native.svg"
        native = subprocess.check_output([str(build / "native-trace"), str(image), str(output),
                                          str(repetitions+1)], text=True)
        native_samples = list(map(float, native.splitlines()))
        native_svg = output.read_bytes()
        wasm_samples, browser_hashes = [], []
        for _ in range(repetitions+1):
            ms, svg = browser_trace(page, image)
            wasm_samples.append(ms)
            browser_hashes.append(hashlib.sha256(svg).hexdigest())
        native_median = statistics.median(native_samples[1:])
        wasm_median = statistics.median(wasm_samples[1:])
        report["cases"].append(dict(image=image.name, input_sha256=hashlib.sha256(image.read_bytes()).hexdigest(),
            cli_sha256=oracle_hash, native_sha256=hashlib.sha256(native_svg).hexdigest(),
            wasm_sha256=browser_hashes, exact=native_svg == oracle and
            all(h == oracle_hash for h in browser_hashes),
            output_bytes=len(svg), paths=svg.count(b"<path "), native_ms=native_samples,
            wasm_ms=wasm_samples, native_median_ms=native_median, wasm_median_ms=wasm_median,
            wasm_over_native=wasm_median/native_median))
        print(json.dumps(report["cases"][-1]), file=sys.stderr, flush=True)
    print(json.dumps(report, indent=2))
    http.shutdown()
    http.server_close()
    browser.close()
