"""Serve the browser build with the isolation headers required by WebAssembly threads."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


class IsolatedFiles(SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        super().end_headers()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=4174)
    args = parser.parse_args()
    files = partial(IsolatedFiles, directory=Path(__file__).resolve().parent / "dist")
    ThreadingHTTPServer(("0.0.0.0", args.port), files).serve_forever()
