# Smoothing progress through hosts

`ImageDocument.runBatchImage` now forwards smoothing checkpoints through the
same progress sink used by palette and fitting. A status-2 document return
throws the existing internal cancellation type before a completed-stage event
can reach the batch runner. The diagnostic identifies the interrupted stage.
The numerical smoother and its retained-state contract are unchanged.

The existing runner, CLI worker, engine and GUI controller already handle stage
progress. Their implementation needed no change. In particular, the worker
polls its shared cancellation flag even without an IPC progress observer, and
the GUI checks cancellation before deduplicating displayed percentages. The
existing frontend displays the active smoothing percentage without new layout,
styles, animations or state fields.

## Verification scope

The preceding numerical foundation run finished at 890/892, with the same two
known decoding/edited-palette failures. Its source stayed fixed while the host
change was tested in an isolated copy. The eight-mask observation test passed
separately because it was added after that full run began. The four verified
source/test files were applied to the main workspace only after the full run
became terminal. The affected main-workspace batch/worker suite then passed
31/31. This is scoped post-change verification, not a second full core run.

The isolated GUI build passed; its full suite passed 661/662. Its sole failure
remains `processing-batch-controls`, whose setter-only reference supplies
backgroundMode 2 to the batch schema admitting 0, 1 and 3. Nothing was weakened
or skipped. An initial HTTP check ran before the isolated build and failed its
build prerequisite; that diagnostic log is retained separately.

A separate owned host/browser used a 256x256 block input. Trusted clicks showed
smoothing at 9 percent, cancelled it at 13 percent, preserved its source and
wrote no output. A trusted retry wrote one SVG after the cancellation capture.
The browser reported no page errors. The installed CLI produced identical SVG
bytes. Both the browser session and host were closed afterward. No original
GUI input, reference-response substitution or artificial worker delay was used.

The fresh installed package passed 18 permission-restricted document/API tests
and 31 batch/worker tests. The restricted API run actually denied original
application reads, workspace reads and child creation. Worker tests ran
separately; worker inheritance of those permissions is not claimed.

## Test ownership and simplification

Applied the consolidate-test-suites and ce-simplify-code rubrics. No independent
reviewers or subagents ran. JJ is unavailable and the directory has no VCS
metadata, so scope and preimages came from the preceding sealed checkpoint.

- Batch orchestration owns suppression of completed smoothing/fitting events
  and output writes on interruption. Its canonical cancellation suite covers
  early and final checkpoints, retained source and successful retry.
- Worker integration owns IPC acknowledgement, cancellation delivery and a
  fresh job after cancellation. Its existing suite covers both numeric stages.
- GUI controller integration owns cancellation polling at repeated 100-percent
  updates, incomplete display/export state and retry.
- Browser verification owns actual control clicks, visible progress and output
  timing. Installed checks own package identity and runtime independence.

All new tests were merged into existing canonical suites. Runtime harnesses,
captures and the initial prerequisite failure are promoted as verification
evidence. No temporary assertion was added to production code. No source
element became unused or unreachable, and no cleanup deletion is pending.

Reuse, quality and efficiency review found no further behavior-preserving
change worth applying. The production diff is one forwarding condition and
one stage-specific cancellation message. Root lint/typecheck scripts are not
configured; module syntax, scoped tests, the GUI build and recorded comparisons
provide the checks for this JavaScript scope.

Segmentation interruption, native progress for disabled masks/counter extremes,
broader original GUI/scheduler behavior, the three known failures and full
application parity remain unfinished. GitHub/npm publication and Rust/GPUI
porting remain pending.
