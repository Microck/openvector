# Single-image processing progress and cancellation

The single-image host now exposes numerical progress, Cancel and Escape. It
keeps the source and current wizard page after cancellation. Retrying uses the
same numerical document and consumed random stream. This is host integration
with the existing native-verified checkpoints, not proof of native GUI timing.

## Ownership and behavior

`ProcessingJob` owns job state, progress deduplication, cancellation polling and
completion. Single-image and batch controllers retain separate view histories.
Both use the same cancellation cell under serialized host admission. Cancellation
is observed before deduplicating repeated integer percentages. A 100-percent
checkpoint is not completion. The batch queue unlocks before the finished state
is published.

The worker forwards progress from the canonical document. Its host serves cached
job state and sets the cancellation cell while the CPU worker is occupied. A
requested internal cancellation becomes `PROCESSING_CANCELLED`; unrelated
exceptions retain their error behavior. Automatic and advanced-profile replay
stop before later stages when a task actually ran and returned status 2. Cached
tasks can retain a preceding status 2, so `didWork` determines whether that status
belongs to the current request.

The GUI polls only while its operation is busy, cleans up its timer and request
on exit, and registers Escape only for an active job. Cancellation does not take
the successful navigation path. Completed display scenes remain separate from
the live numerical buffers.

## Browser failure and repair

The first compiled browser run was blank before React mounted. Its actual error
was `Buffer is not defined`. The new settings-profile cancellation check imported
image-document, bringing codec initialization through the browser's palette
selection import. The correct boundary is a shared, dependency-free cancellation
exception. `task-cancellation.mjs` owns that class; image-document exports the same
identity, and profile replay imports the shared owner directly. The small module
exists to keep browser selection independent of the numerical document.

The corrected build has no Node externalization warnings. Trusted browser input
then exercised automatic processing, the Cancel button, retained-image retry,
Escape at a visible 36-percent palette checkpoint and another successful retry.
The final SVG has 235,956 bytes. Its hash and actual host job state are recorded
in `evidence/single-processing-browser-verification.json`. Earlier browser error
records also contain an unrelated shadcn reference-page hydration error and the
failed first build. No error references the corrected host origin.

The official shadcn Progress reference was inspected at
https://ui.shadcn.com/docs/components/base/progress. The implementation retains
the requested black background, thin white determinate progress and existing
shadcn button. No looping motion was added.

## Verification ownership

Four stage-level tests in the existing GUI application suite compare complete
partial document states and retained-stream retries against the native-verified
direct path. A real-worker HTTP test covers responsive polling, conflicting
mutation rejection, cancellation and retry. The automatic document test verifies
stage-tagged progress and owned observer selections. The profile-document suite
covers cancellation and retry at each stage, including the shared exception's
identity. No mocks were added.

The fresh full core run reports 924/926 and the isolated GUI run 668/669. These
runs precede the module-boundary repair. The final focused GUI run after that
repair passes 62/62. Final document checks pass 67/67. Permission-restricted
installed-package checks pass 68/68. The npm archive contains 126
files; source/archive/installation bytes are compared, including the new signal
module. Installed tests verify actual original/workspace read and child-creation
denials; no worker permission-inheritance claim is made.

The three existing failures remain visible: unstable original Adam7 scratch
pixels, alpha-premultiplied editing with an undefined native palette index, and
the batch-reset fixture's unsupported background-mode producer. No assertion,
admission check or fixture was weakened.

The first baseline launcher failed because `--input-type=module` propagated into
a file-backed worker. A normal module launcher then proved the prior host lacks
`/api/processing` with an actual 404. The first snapshot refresh also used a wrong
relative source path; the resulting unchanged build was retained as a failed
setup attempt. Absolute paths and stop-on-error produced the corrected build.
An initial browser Cancel attempt arrived after processing had completed; later
bounded sequences captured successful interruption. Those failures remain in
the evidence rather than being presented as successful cancellation.

## Simplification and test consolidation

`ce-simplify-code` review ran inline because delegation is not authorized. The
three supplied rubrics covered reuse, quality and efficiency. Two quality fixes
use a boolean summary accessor and flatten displayed-status selection. One
efficiency fix avoids cloning the job twice in batch snapshots. Reuse findings
0, quality applied 2, efficiency applied 1, skipped 0, independent reviewers 0.
The follow-up import-boundary repair retains one exception class and removes the
dependency cycle without a browser polyfill or a second cancellation path.

Consolidate-test-suites: MERGE behavioral coverage into the existing application,
automatic-document and profile-document suites. RETAIN the owned browser captures
and package checks as integration evidence. Batch label tables and lifecycle
state moved into the shared owner; no newly unreachable production code remains.
No separate low-value smoke suite was added. Root lint/typecheck scripts do not
exist; production build and JS/Python syntax checks cover the available tooling.

## Remaining work

Classification internals, editing and export interruption, pending-request
replacement, original worker/controller timing and arbitrary-input parity remain
unfinished. The original application, settings, desktop, previous probes and
captures, root gui/dist and existing user previews remain unchanged. Both owned
test hosts were closed and no longer listen. Publication and Rust/GPUI remain
pending until the full requested behavior is verified.
