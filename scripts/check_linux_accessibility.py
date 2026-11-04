#!/usr/bin/env python3
"""Inspect OpenVector's live AT-SPI tree from the same desktop session.

Run with the system Python and pyatspi installed while OpenVector is open:
    /usr/bin/python3 scripts/check_linux_accessibility.py
This is a diagnostic for issue #10, not a substitute for an Orca test.
"""
import sys
import time

try:
    import pyatspi
except ImportError:
    sys.exit("Install the distribution's python3-pyatspi package and use /usr/bin/python3.")

def walk(node, depth=0):
    try:
        role, name = node.getRoleName(), node.name or ""
        print(f"{'  ' * depth}{role}: {name}")
        yield role, name
        for child in node:
            yield from walk(child, depth + 1)
    except Exception as exc:
        print(f"{'  ' * depth}[unavailable: {exc}]", file=sys.stderr)

deadline = time.monotonic() + 15
apps = []
while time.monotonic() < deadline:
    try:
        apps = [app for app in pyatspi.Registry.getDesktop(0)
                if "openvector" in (app.name or "").lower()]
    except Exception as exc:
        print(f"AT-SPI bus unavailable: {exc}", file=sys.stderr)
    if apps:
        break
    time.sleep(1)
if not apps:
    sys.exit("OpenVector was not found on the AT-SPI desktop. Check the session bus and accessibility activation.")
nodes = [entry for app in apps for entry in walk(app)]
if not any(role in ("push button", "toggle button", "button") and name for role, name in nodes):
    sys.exit("No named buttons were exposed. Check the provider activation and emitted tree.")
print("At least one named button is exposed. Verify navigation, focus, text input, dialogs and Orca separately.")
