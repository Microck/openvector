#!/usr/bin/env python3
"""Exercise numeric input in a disposable, isolated X11 desktop.

Requires python3-pyatspi, xdotool, wmctrl, an active window manager and OpenVector
with an image loaded at wizard mode or palette selection. Create a fresh X11
session with no unrelated applications;
never pass --isolated-session on an everyday desktop. Global keystrokes are used.
This does not verify a screen reader's presence, speech or other backends.
The script changes the image's in-memory count and checks it after reopening.
"""
import argparse
import subprocess
import time
import pyatspi

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--isolated-session', action='store_true',
                    help='Confirm this is a disposable X11 session with no unrelated windows')
parser.add_argument('--expected-count', type=int, choices=range(2, 25))
parser.add_argument('--target-count', type=int, choices=range(2, 25))
args = parser.parse_args()
if not args.isolated_session:
    parser.error('Create an isolated X11 session, then pass --isolated-session')

def walk(n):
    try:
        yield n
        for child in n:
            yield from walk(child)
    except Exception:
        return

def nodes():
    apps=[a for a in pyatspi.Registry.getDesktop(0) if (a.name or '').lower() in ('openvector','openvector-gui')]
    assert len(apps)==1, 'Exactly one OpenVector instance is required'
    return list(walk(apps[0]))

def click(n):
    action=n.queryAction()
    assert action.nActions and action.doAction(0), 'Accessible action failed'

def find_button(items,name):
    return next((n for n in items if n.getRoleName() in ('push button','button') and n.name==name),None)

def isolated_window(require_active=False):
    windows=subprocess.check_output(['wmctrl','-lx'],text=True).splitlines()
    assert len(windows)==1, 'The isolated desktop must contain only OpenVector'
    fields=windows[0].split()
    window=fields[0]
    assert fields[2].lower()=='openvector.openvector', 'The sole window must be OpenVector'
    if require_active:
        active=subprocess.check_output(['xdotool','getactivewindow'],text=True).strip()
        assert int(active)==int(window,16), 'OpenVector must be the active window'

isolated_window()
items=nodes()
advanced=find_button(items,'Advanced')
if advanced:
    assert advanced.queryComponent().grabFocus(), 'Advanced could not receive focus'
    time.sleep(1)
    assert advanced.getState().contains(pyatspi.STATE_FOCUSED), 'Focused state not exposed'
    click(advanced)
    time.sleep(2)
else:
    assert find_button(items,'Edit Count'), 'Start at wizard mode or palette selection'
tested=False
for page in range(8):
    items=nodes()
    print('PAGE',page,[(n.getRoleName(),n.name or '') for n in items])
    edit_button=find_button(items,'Edit Count')
    if edit_button:
        assert edit_button.queryComponent().grabFocus(), 'Edit Count could not receive focus'
        click(edit_button)
        time.sleep(1)
        items=nodes()
        entry=next((n for n in items if n.getRoleName()=='entry' and n.name=='colorCount'),None)
        assert entry, 'Color count entry was not exposed'
        print('Entry interfaces:',entry.get_interfaces())
        assert 'Text' in entry.get_interfaces(), 'Text interface not exposed'
        initial=entry.queryText().getText(0,-1)
        assert initial.isdecimal() and 2<=int(initial)<=24, 'Initial count text is invalid'
        if args.expected_count is not None:
            assert initial==str(args.expected_count), 'Initial text differs from --expected-count'
        target=str(args.target_count if args.target_count is not None else (2 if initial=='3' else 3))
        assert target!=initial, 'Target count must differ from the initial count'
        assert entry.queryComponent().grabFocus(), 'Color count entry could not receive focus'
        time.sleep(0.5)
        assert entry.getState().contains(pyatspi.STATE_FOCUSED), 'Entry focused state not exposed'
        isolated_window(require_active=True)
        subprocess.run(['xdotool','key','ctrl+a'],check=True)
        subprocess.run(['xdotool','type','--clearmodifiers',target],check=True)
        time.sleep(0.5)
        assert entry.queryText().getText(0,-1)==target, 'Updated text not exposed'
        print('Entry text after typing:',repr(entry.queryText().getText(0,-1)))
        apply=find_button(nodes(),'Apply')
        assert apply, 'Apply button not exposed'
        click(apply)
        time.sleep(2)
        edit_button=find_button(nodes(),'Edit Count')
        assert edit_button, 'Edit Count button not exposed after Apply'
        click(edit_button)
        time.sleep(1)
        entry=next((n for n in nodes() if n.getRoleName()=='entry' and n.name=='colorCount'),None)
        assert entry and entry.queryText().getText(0,-1)==target, 'Applied count was not retained'
        close=find_button(nodes(),'Close')
        assert close, 'Close button not exposed'
        click(close)
        print(f'Keyboard input changed count from {initial} to {target}; verified after reopening.')
        tested=True
    if tested: break
    next_button=find_button(items,'Next')
    if not next_button: break
    click(next_button)
    time.sleep(2)
assert tested, 'No editable text control could be exercised'
print('AT-SPI focus and keyboard input passed; screen-reader presence, speech and EditableText require separate verification.')
