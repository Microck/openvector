# OpenVector desktop experience

This describes the native desktop GUI, starting with Windows 10 and default preferences.
It is the work list for the requested GUI QA and usability fixes, not a claim of full
engine parity. CLI/API behavior, the original Vector Magic app, and unpublished native
platform packages are outside this pass. Expected behavior comes from code and tests;
observations from the packaged app are recorded separately.

## Shared structure

The interaction unit is a user command or gesture: starting, ending at once,
becoming extended, while extended, finishing. Variants are current document state,
Basic/Advanced mode, keyboard versus pointer, and single image versus batch.
Every feature uses Summary, The simple case, The interaction event by event,
Modifiers, Cancel and interrupt, Interactions with other systems, Edge cases,
and Open questions and verification.

Interrupt rows, in order: Explicit cancel; Another command; Focus or process loss;
Target changes outside the app; Input device or modifier changes.
Cross-cutting paragraphs, in order: Files and permissions; History and undo;
Busy and modal states; Offline operation; Feedback; Preferences.

## Source map

`rust/crates/openvector-gui/src/app.rs` owns dispatch, navigation and rendering.
`wizard.rs` owns pages and automatic updates. `app_workflows.rs` owns forms,
profiles, save and batch controls. `jobs.rs` and `engine_worker.rs` own worker
commands and completion. `viewer/` owns gestures and view geometry. `preferences.rs`
owns persisted choices. `app-*-tests.rs` and `app_*tests.rs` exercise GUI workflows.
`docs/*-contract.json` records detailed engine boundaries.

## Structure and coverage

| Document | Scope | Status |
| --- | --- | --- |
| [Open an image](single-image/open.md) | Choose, drop, cancel and replace an image; pilot | drafted |
| [Input](foundations/input.md) | Pointer, keyboard, focus, busy states | drafted |
| [Document](foundations/document.md) | Source, result, edits and files | drafted |
| [Navigation](foundations/navigation.md) | Wizard and batch transitions | drafted |
| [Conversion](single-image/conversion.md) | Automatic, Basic and Advanced progression | drafted |
| [Inspection](single-image/inspection.md) | Zoom, pan and result layers | drafted |
| [Editing](single-image/editing.md) | Palette, segmentation and background editing | drafted |
| [Export](single-image/export.md) | Vector/bitmap save, overwrite, drag result | drafted |
| [Batch](batch.md) | Queue, folder, conversion and conflict handling | drafted |
| [Settings](settings.md) | Profiles, recent files and reset | drafted |

[Glossary](glossary.md) defines terms. [Verification](verification/README.md) records
checks and their evidence. [Bug triage](bug-triage.md) distinguishes suspected issues
from reproduced defects. Drafted does not mean runtime verified.

[Observed QA results](verification/observed-results.md) records passing tests, native
observations and remaining coverage for the current changes.
