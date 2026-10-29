# Cached palette calls and scheduler skips

Successful direct palette analysis now records status 0. Previously, a cancelled
segmentation could leave status 2 in the independent document even after a
successful cached analyze() call. The new native comparison fails as 2 != 0
before the fix and passes after it. The change applies to successful palette
calls generally; it adds no cache-specific branch or compatibility behavior.

The original boundaries differ. Document 0x46db00 clears cancellation, loads
the palette profile and supplied colors, commits settings and returns 0 even
when no candidate needs recomputation. ThreadManager 0x43fdb0 palette kind 2
checks 0x469dd0 first. Its cached path copies requested settings without running
the method, reloading the profile, transferring live colors or clearing the
cancellation flag. A task skip has no new document method result. The independent
API therefore retains its prior document result in that path.

## Native evidence

The separate protocol-6 observer initializes an owned QCoreApplication and
ThreadManager only for cached request observation. The original needs-work
predicate must reject work before the request. The request must return with no
active/pending task, allocated worker task or progress event. No native widget,
window input, original instruction patch, forced readiness flag, event pumping
or worker-thread startup was introduced. All 43 accepted helpers and five earlier
numeric candidates remain unchanged.

The pilot ran four fresh processes. Calibration ran 62 and retained all 31
preceding rows exactly. Another 48 processes captured 16 cases across the four
fixed/unlimited and aliased/antialiased profiles, direct cached calls versus
cached scheduler requests, changed supplied colors, prior segmentation
cancellation, first/final new-segmentation cancellation, and retry. All repeated
responses match byte for byte. The auditor reparses all 114 responses and checks
request bytes and the frozen fixture.

The recorder distinguishes a scheduler request return from a document method
return. The task comparison asserts no work, no progress and retained document
status. It does not assign the request return to the independent document's
result. All actual document calls still require exact native result comparison.

## Assertion and review ownership

The existing document-cancellation suite owns the workflow and exact partial
state comparisons. Its shared-control assertions now include usePalette,
shadowing and clusterColors, which live outside the 100 registered numeric
parameters. The same six-word assertion already used by completed document
tests was extracted into the existing native-gui helper and reused. Existing
assertion behavior and labels remain unchanged for those callers.

Applied all three ce-simplify-code review rubrics in one context; zero independent
reviewers ran. Reuse 1: shared assertions now have one owner. Quality 1: the new
recorder emits its explicit packet format directly instead of translating and
patching an older packet shape. Efficiency 0, skipped 0. No additional abstraction
earned its cost. No root lint/typecheck script exists. Native compilation uses
warnings as errors; final JS/Python syntax checks cover the edited code.

Consolidate-test-suites disposition: MERGE into the existing document integration
suite and shared assertion helper; PROMOTE the frozen native fixture and
recorder/auditor. Installed tests own package resolution and actual permission
denials, separate from numerical conformance. The pre-fix failure log remains
evidence. No production code became unreachable and no deletion is pending.

## Verification boundary

The checkpoint records the full core run, palette/batch checks, fresh isolated
GUI build/tests, and fresh installed API run. All 125 archived files match source
and installation. The original application/settings, earlier verified roots,
root gui/dist and existing previews are preserved. No VCS metadata, commit, push
or publication was created. JJ remains unavailable.

The known batch background-mode-2 failure was reviewed first. Verified native
menus expose 0, 1 and 3, and advanced-profile application preserves the batch
background field. No legitimate producer for mode 2 was established. Processing
admission and the failing comparison remain unchanged; no setter adapter,
extra model channel, widened admission or skipped expectation was introduced.

The two other known failures, interrupted palette state, old fitted-graph
validity, editing/shadow workflows, broader scheduler behavior and full parity
remain open. Publication and Rust/GPUI porting are pending.
