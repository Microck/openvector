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
        yield role, name, f"{'  ' * depth}{role}: {name}"
        for child in node:
            yield from walk(child, depth + 1)
    except Exception as exc:
        yield "", "", f"{'  ' * depth}[unavailable: {exc}]"

deadline = time.monotonic() + 15
nodes = []
found_app = False
last_error = None
while time.monotonic() < deadline:
    try:
        apps = [app for app in pyatspi.Registry.getDesktop(0)
                if "openvector" in (app.name or "").lower()]
        found_app |= bool(apps)
        nodes = [entry for app in apps for entry in walk(app)]
        if any(role in ("push button", "toggle button", "button") and name
               for role, name, _ in nodes):
            break
    except Exception as exc:
        last_error = exc
    time.sleep(1)
for _, _, line in nodes:
    print(line)
if not found_app:
    sys.exit(f"OpenVector was not found on the AT-SPI desktop. Check the session bus and accessibility activation. {last_error or ''}")
if not any(role in ("push button", "toggle button", "button") and name for role, name, _ in nodes):
    sys.exit("No named buttons were exposed before the deadline. Check provider activation and the emitted tree.")
print("At least one named button is exposed. Verify navigation, focus, text input, dialogs and Orca separately.")
