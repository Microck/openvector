# Input and feedback

## Summary

The app supports pointer controls, keyboard focus, native file dialogs and dropped
files. The root view handles navigation shortcuts; text fields own typed text.

## The simple case

Use the toolbar to open a source, choose a conversion mode in the right panel,
and inspect the viewer. A footer reports processing and errors.

## The interaction, event by event

Controls that change a document are disabled while the worker runs. View controls
remain available where the document has that layer. Cancel requests worker
cancellation rather than undoing arbitrary completed work.

Tab and Shift+Tab move focus. Page Up/Down and Alt+Left/Right navigate the wizard.
Enter on the viewer advances a page when supported. Ctrl+V pastes an image;
Ctrl+Z/Y apply only to editing/background pages. Escape dismisses custom dialogs,
ends color picking or cancels a drag where implemented.

Advanced automatic updates wait 500 ms after a relevant change before processing.
Scrollbar repeat begins after 500 ms and repeats at 50 ms; these are implementation
settings, not claims of exact OS event delivery.

## Cancel and interrupt

| Event | Before extended | While extended |
| --- | --- | --- |
| Explicit cancel | Dismiss a dialog without accepting it. | Request worker cancellation or end the active gesture. |
| Another command | The selected command owns its interaction. | Busy/modal guards restrict conflicting commands. |
| Focus or process loss | Focus identifies where keys go. | Process exit loses in-memory state; no autosaved project is promised. |
| Target changes outside the app | Disk paths can become unavailable. | The affected operation reports failure. |
| Input device or modifier changes | Use keyboard or pointer controls. | Viewer gestures interpret held modifiers; inspect actual release behavior. |

## Open questions and verification

Keyboard-only E2E, dialog focus and button activation need runtime checks.

Source reviewed: `vector-magic-re` commit `e794628c8ea3fbcfd4903e55c95ac29894846715`. Runtime verification pending.
