#!/usr/bin/env python3
"""Inspect OpenVector's live AT-SPI tree from the same desktop session.

Run with the system Python and pyatspi installed while OpenVector is open:
    /usr/bin/python3 scripts/check_linux_accessibility.py
Enable accessibility through the desktop or a screen reader first. AccessKit's\nUnix adapter may stay inactive when the AT-SPI session is not enabled; an\ninspector alone may not enable it. This is not a substitute for an Orca test.
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
        yield node, role, name, f"{'  ' * depth}{role}: {name}"
        for child in node:
            yield from walk(child, depth + 1)
    except Exception as exc:
        yield None, "", "", f"{'  ' * depth}[unavailable: {exc}]"

deadline = time.monotonic() + 15
nodes = []
found_app = False
last_error = None
desktop_names = []
while time.monotonic() < deadline:
    try:
        apps = list(pyatspi.Registry.getDesktop(0))
        desktop_names = [app.name or "<unnamed>" for app in apps]
        trees = [list(walk(app)) for app in apps]
        matches = [tree for tree in trees
                   if any("openvector" in name.lower() for _, _, name, _ in tree)]
        found_app |= bool(matches)
        nodes = [entry for tree in matches for entry in tree]
        if any(role in ("push button", "toggle button", "button") and name
               for _, role, name, _ in nodes):
            break
    except Exception as exc:
        last_error = exc
    time.sleep(1)
for _, _, _, line in nodes:
    print(line)
if not found_app:
    sys.exit(f"OpenVector was not found on the AT-SPI desktop. Applications: {desktop_names}. Check the session bus and accessibility activation. {last_error or ''}")
if not any(role in ("push button", "toggle button", "button") and name for role, name, _ in nodes):
    sys.exit("No named buttons were exposed before the deadline. Check provider activation and the emitted tree.")
print("At least one named button is exposed. Verify navigation, focus, text input, dialogs and Orca separately.")

if "--test-about" in sys.argv[1:]:
    about = next((node for node, role, name, _ in nodes
                  if role in ("push button", "toggle button", "button") and name == "About"), None)
    if about is None:
        sys.exit("About button not found.")
    try:
        actions = about.queryAction()
        names = [actions.getName(i) for i in range(actions.nActions)]
        if not names:
            sys.exit("About exposes no accessible action.")
        if not actions.doAction(0):
            sys.exit(f"About accessible action failed: {names}")
    except Exception as exc:
        sys.exit(f"Could not invoke About via AT-SPI: {exc}")
    deadline = time.monotonic() + 10
    dialog_found = False
    while time.monotonic() < deadline:
        trees = [list(walk(app)) for app in pyatspi.Registry.getDesktop(0)]
        if any(role == "dialog" and name == "About OpenVector"
               for tree in trees for _, role, name, _ in tree):
            dialog_found = True
            break
        time.sleep(0.5)
    if not dialog_found:
        sys.exit("About action did not expose its dialog before the deadline.")
    print("About opened through AT-SPI and its dialog was exposed.")
