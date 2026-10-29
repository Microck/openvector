# Shared vector and bitmap saves

QuickSaveFile now owns both output kinds. A save command requires kind vector
or bitmap and mode basic or advanced. Directory selection uses the same explicit
kind. All current callers use that shape; no legacy request alias is accepted.
Bitmap targets come from the selected pane's filename, directory and format,
with scale from the shared BitmapSize owner. Vector export options and host EMF
metrics retain their existing owner.

One pending transaction freezes its path, source filename and rendered bytes.
The worker blocks mutations while any decision remains pending. Directory and
overwrite questions remain cancellable. Source protection and filesystem writes
use one common path. Errors clear the transaction; successful writes return Saved.
The unchanged notification timer keeps separate vector/bitmap and basic/advanced
records for five seconds.

## Native warning and ordering

Original QuickSaveWidget savePressed at 0x4212d0 handles filename, directory
creation, source protection and overwrite before emitting its save signal.
The bitmap receiver at 0x451430 calls 0x429bb0, which calls the flatten warning
before scheduling its bitmap task. Instructions at 0x44c4b8 and 0x44c4bd both push
0x400 into QMessageBox::warning. The pinned Qt 4.4.3 header identifies that value
as OK. This warning has no Cancel option. The earlier independent download
confirmation allowed cancellation; current downloads and direct saves now use
the native OK-only warning instead.

The shared transaction preserves that visible decision order. Its acknowledge
command applies only to a pending flatten warning; decide cannot accept or cancel
it. The independent exporter prepares owned bytes before filesystem decisions,
including the bitmap bytes that require acknowledgement. It writes no target
before all decisions finish. This preserves pending output if the engine is used
directly as well as through the worker. It does not claim identical native CPU
task scheduling or render-failure timing.

## Rendered fields

SaveLocation renders folder, filename, Browse, Save and notification controls
for both kinds. It normalizes committed text even when the filename stem does
not change. It holds the latest field commit so a rejected edit cannot save the
previous target. Parent operations still share the existing application queue.
Enter saves after normalization and returns focus to the current field identified
by its stable DOM id, including when normalization replaces the input node.

Browser verification covers cancelled and accepted directory creation, overwrite
cancellation preserving existing bytes, overwrite before the transparency warning,
the OK-only warning, direct JPEG and browser-download bytes, filename Enter,
normalization, rejected-NUL edits, successful notifications and restored focus.
A vector SVG save also matches its canonical HTTP export through the shared UI.
The native folder picker was not driven by the observer.

## Review and tests

All three ce-simplify-code rubrics ran inline with zero independent reviewers.
No additional behavior-preserving simplifications were needed: reuse 0, quality 0,
efficiency 0, skipped 0. The implementation shares the file transaction, existing
filename owner and location component. The focus defect found in browser testing
was corrected separately as observable behavior. No existing code became unused.

The consolidate-test-suites invariant is a correctly ordered, owned save across
real filesystem and worker boundaries. Integration coverage belongs in the existing
quick-save-file suite. Its four bitmap writers compare complete native files.
The existing HTTP decision test now runs for both kinds; a separate warning test
checks the cross-kind lock, acknowledgement and read-only access. Existing batch
tests only received the required explicit vector kind. No mocks or new standalone
tests were added, and no reference expectation was changed.

The 69 scoped tests pass. The final isolated GUI suite passes 640 of 641 tests,
with zero skips. Its failure remains the known processing-batch-controls comparison
for backgroundMode 2. The isolated GUI builds successfully. No lint or typecheck
script is configured. A temporary console probe in the isolated copy confirmed
that the notification reaches the field and expires after about five seconds.
Its logging was removed before the clean build and final browser verification;
the canonical workspace was never instrumented.

## Limits

Native dialogs, savePressed and error notifications were not executed. Original
source protection beyond exact resolved filename equality, unusual filename
completion, native asynchronous failure/notification behavior, view-scale and
preset producers, remaining bitmap lifecycle transitions and full application
parity remain unverified or incomplete. Existing previews, the original application
and settings, accepted native helpers, package core and CLI stay unchanged.
Publication and Rust/GPUI work are not complete.

The authoritative checkpoint is evidence/bitmap-direct-save-verification-checkpoint.json.
