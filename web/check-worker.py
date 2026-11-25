"""Exercise a long trace and cancellation through the GPUI browser interface."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import time

from PIL import Image
from playwright.sync_api import sync_playwright

root = Path(__file__).resolve().parents[1]
fixture = root / ".github/assets/openvector-logo.png"
reference = json.loads((root / "docs/performance/wasm-prototype.json").read_text())
expected = next(case["cli_sha256"] for case in reference["cases"] if case["image"] == fixture.name)


def screenshot(page):
    return Image.open(io.BytesIO(page.screenshot()))


def trace(browser, url, cancel):
    page = browser.new_page(viewport={"width": 1180, "height": 760}, device_scale_factor=1)
    try:
        page.goto(url)
        assert page.evaluate("crossOriginIsolated"), "Missing HTTPS or cross-origin isolation headers"
        page.locator("canvas").wait_for()
        # GPUI attaches its canvas before the renderer and document thread are ready.
        deadline = time.monotonic() + 20
        while screenshot(page).getpixel((55, 12))[:3] != (49, 50, 56):
            assert time.monotonic() < deadline, "The editor did not paint the Open button"
            page.wait_for_timeout(100)
        with page.expect_file_chooser() as chooser:
            page.mouse.click(75, 18)
        chooser.value.set_files(str(fixture))
        page.mouse.move(500, 500)
        assert page.workers, "The document worker did not start"
        # At this viewport size the automatic-mode button has the idle header
        # button's fill, while the same point on the load page has the accent fill.
        deadline = time.monotonic() + 15
        while True:
            frame = screenshot(page)
            if frame.getpixel((891, 154)) == frame.getpixel((116, 18)):
                break
            assert time.monotonic() < deadline, "The wizard did not open the source"
            page.wait_for_timeout(100)
        # At this position the idle footer is replaced by the Cancel button.
        cancel_pixel = (1127, 748)
        idle = frame.getpixel(cancel_pixel)
        page.evaluate("""() => {
            window.traceTicks = [performance.now()];
            window.traceTimer = setInterval(() => traceTicks.push(performance.now()), 50);
        }""")
        page.mouse.click(1020, 155)
        deadline = time.monotonic() + 5
        while screenshot(page).getpixel(cancel_pixel) == idle:
            assert time.monotonic() < deadline, "The page did not paint the busy state during the trace"
            page.wait_for_timeout(50)
        if cancel:
            page.mouse.click(1140, 744)
        deadline = time.monotonic() + (3 if cancel else 90)
        while screenshot(page).getpixel(cancel_pixel) != idle:
            assert time.monotonic() < deadline, "The document did not return to idle"
            page.wait_for_timeout(100)
        gaps = page.evaluate("""() => {
            clearInterval(traceTimer);
            traceTicks.push(performance.now());
            return traceTicks.slice(1).map((t, i) => t - traceTicks[i]);
        }""")
        if cancel:
            with page.expect_file_chooser(timeout=2000):
                page.mouse.click(75, 18)
            return
        assert len(gaps) >= 3 and max(gaps) < sum(gaps) / 2, "Main thread stalled during tracing"
        page.mouse.click(1100, 702)
        deadline = time.monotonic() + 10
        while screenshot(page).getpixel((890, 542))[:3] != (47, 111, 236):
            assert time.monotonic() < deadline, "The export screen did not show the Save button"
            page.wait_for_timeout(100)
        with page.expect_download(timeout=5000) as download:
            page.mouse.click(1020, 542)
        digest = hashlib.sha256(download.value.path().read_bytes()).hexdigest()
        assert digest == expected, f"SVG differs from the native reference: {digest}"
        print(f"Native SVG matched. UI timer ran {len(gaps)-1} times; longest gap {max(gaps):.0f} ms.")
    finally:
        page.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("url")
    parser.add_argument("--browser", choices=["chromium", "firefox", "webkit"], default="chromium")
    parser.add_argument("--headed", action="store_true")
    args = parser.parse_args()
    with sync_playwright() as playwright:
        options = {"headless": not args.headed}
        if args.browser == "chromium":
            options.update(executable_path=os.getenv("CHROMIUM"),
                           args=["--enable-unsafe-swiftshader", "--use-angle=swiftshader"])
        browser = getattr(playwright, args.browser).launch(**options)
        trace(browser, args.url, cancel=True)
        trace(browser, args.url, cancel=False)
        browser.close()
