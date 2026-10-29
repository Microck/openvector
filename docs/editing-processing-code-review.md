# Editing completion progress and cancellation

Basic Edit result Next now uses the existing worker ProcessingJob and the
document's smoothing/fitting checkpoints. Advanced Next remains commit-only.
This closes the independent host integration gap without changing numerical
algorithms, introducing another cancellation flag or rolling back committed edits.

## Expected behavior and first failure

The invariant is commit first, then vector tasks. The document owns edit history,
the partial numerical buffers and randomness. The GUI owns its current editor
projection and last completed scene. Cancellation must stop later tasks, retain
the committed edit key and keep the editing page available for retry.

The original implementation's `finishEditing` ignored progress observers and
always ran fitting after smoothing. The worker did not admit edit finish into
its existing single-image job. Three new tests failed before the change: both
requested stage cancellations returned normally, and malformed observers were
accepted rather than rejected before committing edits.

`finishEditing(input,onStage)` now validates observers before mutation, commits
history, forwards stage-tagged progress and stops when a task actually ran with
status 2. The `didWork` check preserves the established cached-task result
lifetime. Each completed task publishes its owned snapshot. A separate final
snapshot prevents an observer from poisoning the return value. A focused failing
test demonstrated that ownership issue before its correction.

The worker wraps edit finish with its existing processing owner. After a failed
basic finish, GuiEngine refreshes the retained editor from the document before
rethrowing the original error. It adopts the vector scene only after success.
The frontend applies that actual editor tool and color from the error response;
successful navigation does not run after cancellation.

## Verification and test ownership

MERGE four document cases into `test/segmentation-editor.test.mjs`: smoothing and
fitting cancellation/retry, pre-commit validation, and observer/result ownership.
The cancellation cases compare the complete partial numerical state with
explicit commit plus the same native-verified tasks, including consumed PRNG
state. They verify that smoothing interruption never publishes fitting.

MERGE three boundary cases into `gui/test/application.test.mjs`: both interrupted
stages retain the actual Pan tool, history, edited bitmap and last completed
preview; a real HTTP worker exposes progress and Cancel and supports retry.
These cover projection and IPC failure modes, separate from document ownership.
No mocks or standalone smoke suite were introduced.

Fresh checks:

- Document-focused 4/4 and GUI/HTTP-focused 3/3.
- Existing editing/background/gesture suites 89/90. The original undefined
  alpha-palette-index comparison remains failing.
- CLI conversion/profile suites 116/116, including existing edit-plan callers.
- Final isolated full GUI 671/672. The batch-reset fixture's unsupported
  background-mode producer remains failing.
- Installed public API checks 5/5, scoped to the new finishEditing cases plus
  actual original/workspace read and child-creation denials.

The preceding full-core result is an explicitly reused baseline, not a fresh
full-core claim. Its Adam7 scratch-storage failure remains unresolved. No test,
fixture or admission constraint was weakened.

The first trusted browser run painted a region, selected Pencil, clicked Next,
observed smoothing at 4 percent, clicked Cancel and verified the editing page,
Pan selection and retained committed history. Retry produced a 235,664-byte SVG.
The final-source run painted another region and used Escape during smoothing;
its two committed edits survived and retry produced a 235,372-byte SVG. Both
hashes and job states are recorded. Page error buffers were empty.

A timing attempt completed before Cancel could be observed. Its null-element
evaluation failures and screenshots remain failed automation evidence, not proof
of interruption. A first Pencil click also arrived while gesture controls were
disabled; subsequent input waited for observed enabled state. No product timing
or event transport was altered to accommodate the driver.

## Simplification and scope

`ce-simplify-code` ran inline under the user's delegation constraint. Reuse
findings 0; quality applied 1 for separate observer/result ownership; efficiency
applied 1 for refreshing the editor only on failure rather than making a discarded
snapshot on success; skipped 0; independent reviewers 0. The final full GUI run
and final-source browser check follow that refinement. Root lint/typecheck
scripts do not exist; JS syntax checks and the isolated production build pass.

PROMOTE the reference-based failing logs, owned browser records, package identity
and checkpoint auditor as reproducible integration evidence. No newly unreachable
production elements remain. Existing sources, tests and probes were preserved.

Changed production files are `src/image-document.mjs`, `gui/engine.mjs`,
`gui/engine-worker.mjs` and `gui/src/app.jsx`; README and the live progress
contracts describe their new behavior. No styles, component libraries, runtime
dependencies, native probes or numerical algorithms changed. The original
installation/settings/desktop, earlier captures, root gui/dist and existing
user previews remain unchanged. Owned browser sessions and hosts were closed.

Native editing cancellation/controller timing, export interruption, pending
request replacement, arbitrary-input parity and the three known failures remain
unfinished. Publication and Rust/GPUI remain pending.
