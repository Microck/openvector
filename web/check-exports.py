"""Compare downloads from the browser editor with native 0.1.7 output."""
import argparse
import hashlib
import io
import os
import re
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

from PIL import Image
from playwright.sync_api import sync_playwright

FIXTURES = Path(__file__).resolve().parents[1] / "test/fixtures"
JPEG = FIXTURES / "browser-session-logo.jpeg"
PNG = FIXTURES / "browser-session-logo.png"

# One native automatic trace of JPEG. EMF uses a 960x960, 254x254 mm
# reference display. The EPS digest excludes only its variable date value.
FORMATS = {
    "ai":  (920, 178, "b2f586b23830e84a9c2fae82a5b4c497fbe0c30355d7a080b5a46db5707956d0"),
    "dxf": (1020, 178, "690626f453033207cf2da085ab1511901eb637a45fb1c5a246251cd687245540"),
    "emf": (1110, 178, "dbce4970f86eecd2e5754acc754d5a76cb58de72e52f50dc148b18586a367de9"),
    "eps": (920, 214, "f92ab63178234ff0babe4e23ba23cdfa1b3c2afbcf54f64cc338c2d71f991853"),
    "pdf": (1020, 214, "82cbc53b2aad104ab881db0a64725de0d4a54e801b67df702d5e93efacb4cf12"),
    "svg": (1110, 214, "65bed12b8e6fb512d38d336487b6567745c65363b438d2ed2e7757b4d6f5624b"),
}
# Native CoreSession with setSource/runAutomatic on PNG, BMP, then JPEG.
REUSED_SVG = "940b1353b320112ec29b4dfc423f95cc2cd42c8fd4188ae7699e106a0c9c770b"
BLUE = (47, 111, 236)
SELECTED = (36, 50, 79)


def wait_frame(page, condition, description):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        frame = Image.open(io.BytesIO(page.screenshot())).convert("RGB")
        value = condition(frame)
        if value:
            return value
        page.wait_for_timeout(80)
    raise AssertionError(f"Editor did not show {description}")


def open_editor(browser, url):
    page = browser.new_page(viewport={"width": 1180, "height": 760}, device_scale_factor=1)
    page.goto(url)
    if not page.evaluate("crossOriginIsolated"):
        raise AssertionError("The release is missing cross-origin isolation")
    wait_frame(page, lambda f: f.getpixel((55, 12)) == (49, 50, 56), "Open button")
    return page


def trace(page, source):
    with page.expect_file_chooser() as chooser:
        page.mouse.click(75, 18)
    chooser.value.set_files(str(source))
    wait_frame(page, lambda f: f.getpixel((891, 154)) == (49, 50, 56), "automatic mode")
    page.mouse.click(1020, 155)
    wait_frame(page, lambda f: f.getpixel((1070, 692)) == BLUE, "review step's Next button")
    page.mouse.click(1110, 702)
    wait_frame(page, lambda f: f.getpixel((890, 542)) == BLUE, "export panel")


def download(page, fmt):
    x, y, _ = FORMATS[fmt]
    page.mouse.click(x, y, delay=60)
    page.mouse.move(500, 400)
    wait_frame(page, lambda f: f.getpixel((x - 28, y)) == SELECTED, fmt.upper())
    # Format-specific options change the vertical position of Save vector.
    def save_row(frame):
        return next((row + 4 for row in range(460, 670)
                     if all(frame.getpixel((col, row)) == BLUE for col in (890, 1145))), None)

    row = wait_frame(page, save_row, "Save vector button")
    start = time.time()
    with page.expect_download(timeout=8000) as event:
        page.mouse.click(1020, row)
    if Path(event.value.suggested_filename).suffix != "." + fmt:
        raise AssertionError(f"Expected {fmt}, received {event.value.suggested_filename}")
    content = event.value.path().read_bytes()
    if fmt == "eps":
        dates = re.findall(rb"(?m)^%%CreationDate: ([^\r\n]+)", content)
        if len(dates) != 1:
            raise AssertionError("EPS must contain exactly one creation date")
        timestamp = datetime.strptime(dates[0].decode(), "%a %b %d %H:%M:%S %Y").replace(tzinfo=timezone.utc)
        if not int(start) <= timestamp.timestamp() <= time.time():
            raise AssertionError(f"Unexpected EPS creation date: {timestamp}")
        content = content.replace(b"%%CreationDate: " + dates[0], b"%%CreationDate: <runtime>", 1)
    return hashlib.sha256(content).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("url")
    args = parser.parse_args()
    with sync_playwright() as playwright, tempfile.TemporaryDirectory() as tmp:
        browser = playwright.chromium.launch(executable_path=os.environ.get("CHROMIUM"),
            args=["--enable-unsafe-swiftshader", "--use-angle=swiftshader"])
        page = open_editor(browser, args.url)
        trace(page, JPEG)
        for fmt, (_, _, expected) in FORMATS.items():
            actual = download(page, fmt)
            if actual != expected:
                raise AssertionError(f"{fmt}: expected {expected}, got {actual}")
            print(f"{fmt.upper()} passed", flush=True)

        page.close()
        bmp = Path(tmp) / "logo.bmp"
        with Image.open(PNG) as pixels:
            pixels.save(bmp)
        page = open_editor(browser, args.url)
        for source in (PNG, bmp, JPEG):
            trace(page, source)
        actual = download(page, "svg")
        if actual != REUSED_SVG:
            raise AssertionError(f"Reused JPEG: expected {REUSED_SVG}, got {actual}")
        browser.close()
        print("Reused-session JPEG passed")


if __name__ == "__main__":
    main()
