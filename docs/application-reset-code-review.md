# Application model reset

One coordinator resets the existing image, batch, profile-list and source-history
owners. The native model operation preserves the current workspace and page,
including batch Files and Settings. It retains the current source directory,
advanced batch controls and export options. It clears the source document,
queue, recent files, recent directories and saved custom profiles.

Domain mutations finish synchronously before the two existing stores write.
Both writes get an attempt. Storage errors return the actual reset session and
identify each failed store. These independent atomic writes do not provide a
transaction across files. The HTTP host uses its existing serialization and
origin checks, rejects a running batch, and projects the batch revision after
the worker publishes reset state. No complete Reset Everything action is
exposed in the browser while window reset and preference ownership are pending.

## Evidence corrections

The first menu capture combined the batch flag with single-image export page
19. It remains a flag/export-page observation. Corrected captures explicitly
observe batch queue page 1 and settings page 2 with the reset menu enabled.

The inherited factory-reset request encoder joined queued paths with a newline.
That produced one invalid joined filename. The corrected encoder uses the
native NUL delimiter. Seven fresh paired workflows retain 28 processes and 42
widget snapshots; queued cases now contain two separate valid paths. Their
request/response bytes are reparsed and checked against recorded hashes.
The input fixture comes from the matching capture proofs, with equality checked
across all seven cases. It does not borrow another workflow's inputs.

Historical newline captures and expectations remain intact. Their frozen
archives contain the original encoder. Re-encoding them with the corrected
current encoder is not a valid historical audit. The new reset tests compare
the corrected corpus and keep the existing failing transparency test unchanged.

The settings-page workflow exposed another implementation mismatch: adding
files returned OpenVector to Files. Native addition preserves Settings. The
shared batch command now retains its current page on addition. Native removal
and clear navigation remain unverified and were not changed in this pass.

## Review

The ce-simplify-code reuse, quality and efficiency rubrics ran inline with zero
independent contexts under the standing no-delegation instruction. The setup
script ran directly with the installed Node runtime because Bash is unavailable.
This is one review context, not independent corroboration.

- Reuse: local and application resets call the same owner operations and stores.
  There is no parallel state, compatibility format or force flag. No further
  behavior-preserving reuse change was needed.
- Quality: admission precedes mutation. Storage failures preserve truthful
  session state. HTTP tests cover the worker-to-host projection and rejection
  before mutation. No newly unreachable code or unused production element was
  identified. No quality-only edits were needed.
- Efficiency: the two writes run concurrently after the synchronous reset.
  Test input decoding was reduced from repeated decompression/parsing to one
  self-contained fixture read. No per-frame work, timer or runtime dependency
  was added. One efficiency improvement was applied; no other finding needed
  an edit.

## Verification and limits

The focused reset/batch run passes 28 tests. Ten new reset tests cover four
native workflows, five complete document comparisons with 500 parameter
records, each store failing alone, both stores failing, successful HTTP reset,
invalid origin/input, actual returned state after a write failure, and a real
batch paused for an overwrite decision.

The isolated production build passes. Its full GUI suite passes 352 of 353
tests, skips none, and retains the existing backgroundMode 2 comparison failure.
The project has no lint or typecheck scripts. The numerical engine and npm
package files did not change. Existing user previews and workspace gui/dist
remain untouched. No browser layout or branding change needs a new screenshot.

Complete window reset, native post-confirmation state, selected/batch preference
persistence, the earlier parity failures, publication and the Rust/GPUI port
remain unfinished. This checkpoint does not establish full application parity.
