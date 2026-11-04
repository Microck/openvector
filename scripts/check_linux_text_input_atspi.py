#!/usr/bin/env python3
"""Exercise loaded-image numeric text input with AT-SPI and a running X11 session.

Requires python3-pyatspi, xdotool, an active window manager and OpenVector with
an image loaded. This does not verify spoken output or other desktop backends.
The script changes the loaded image's in-memory palette count to three.
"""
import time, subprocess, pyatspi
def walk(n):
    try:
        yield n
        for child in n:
            yield from walk(child)
    except Exception:
        return
def nodes():
    apps=[a for a in pyatspi.Registry.getDesktop(0) if (a.name or '').lower() in ('openvector','openvector-gui')]
    assert apps, 'OpenVector is absent from AT-SPI'
    return [n for a in apps for n in walk(a)]
def click(n):
    action=n.queryAction()
    assert action.nActions and action.doAction(0), 'Accessible action failed'
def find_button(items,name):
    return next((n for n in items if n.getRoleName() in ('push button','button') and n.name==name),None)
items=nodes()
advanced=find_button(items,'Advanced')
assert advanced, 'Advanced button not exposed'
assert advanced.queryComponent().grabFocus(), 'Advanced could not receive focus'
time.sleep(1)
assert advanced.getState().contains(pyatspi.STATE_FOCUSED), 'Focused state not exposed'
click(advanced)
time.sleep(2)
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
        print('AFTER EDIT COUNT',[(n.getRoleName(),n.name or '') for n in items])
    if edit_button:
        entry=next((n for n in items if n.getRoleName()=='entry' and n.name=='colorCount'),None)
        assert entry, 'Color count entry was not exposed'
        print('Entry interfaces:',entry.get_interfaces())
        assert 'Text' in entry.get_interfaces(), 'Text interface not exposed'
        assert entry.queryText().getText(0,-1)=='2', 'Initial text not exposed'
        assert entry.queryComponent().grabFocus(), 'Color count entry could not receive focus'
        time.sleep(0.5)
        assert entry.getState().contains(pyatspi.STATE_FOCUSED), 'Entry focused state not exposed'
        subprocess.run(['xdotool','key','ctrl+a'],check=True)
        subprocess.run(['xdotool','type','--clearmodifiers','3'],check=True)
        time.sleep(0.5)
        assert entry.queryText().getText(0,-1)=='3', 'Updated text not exposed'
        print('Entry text after typing:',repr(entry.queryText().getText(0,-1)))
        apply=find_button(nodes(),'Apply')
        assert apply, 'Apply button not exposed'
        click(apply)
        time.sleep(2)
        items=nodes()
        print('AFTER APPLY',[(n.getRoleName(),n.name or '') for n in items])
        assert find_button(items,'Edit palette color 3'), 'Typed value was not applied to color count'
        print('Keyboard text entry changed and applied the color count with Orca running.')
        tested=True
    if tested: break
    next_button=find_button(items,'Next')
    if not next_button: break
    click(next_button)
    time.sleep(2)
assert tested, 'No editable text control could be exercised'
print('AT-SPI focus and keyboard text entry exercised with Orca running; speech output and editable-text interface remain unverified.')
