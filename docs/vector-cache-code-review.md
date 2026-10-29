# Cached vector requests

The original scheduler leaves document state unchanged when segmentation,
smoothing or fitting needs no work. ThreadManager kinds 3/4/5 call original
predicates 0x46a720, 0x46a860 and 0x46a900. Their skip branches jump to completion
without loading a profile or copying settings. The settings copy in palette
kind 2 is a separate observed behavior.

The independent engine already follows this boundary. No runtime or GUI code
changed. The new reference compares 36 workflows through both document APIs:
four fixed/unlimited and aliased/antialiased profiles, three cached vector
requests, and completed/first-cancelled/final-cancelled advanced palette work.
A skipped request keeps the palette profile, committed settings, progress,
cancellation, labels, region records, random state and completed geometry.

This supplies scheduler evidence for the earlier basic-fit pilot. A direct fit
on the completed key can reload its basic profile; a skipped fit request keeps
the active advanced palette profile. The earlier direct-only capture remains
preserved and is not relabeled as scheduler evidence.

## Native observer

Protocol 8 is separate from the seven frozen numeric candidates and 43 accepted
helpers. It extends the established owned QCoreApplication/ThreadManager setup
to cached vector requests. The original predicate must return false before a
request is admitted. After the request, active and pending task state, task
pointer and progress count must all remain zero. No worker starts, no event loop
is pumped, and no native widget or readiness flag is changed.

Cached contours and fitted geometry use the existing producer-tracked storage.
The prefix completes actual segmentation, smoothing and fitting before palette
work. Cubic allocation and written-slot checks remain intact. Cached graph
availability is never inferred from stale edges or memory values.

Calibration preserves 43 earlier rows across 86 fresh processes. Four baselines
and 36 workflows use another 80. All responses repeat byte for byte, and the
auditor reparses all 166 responses. Request framing, raw hashes, frozen fixture
rows, unchanged document fields and retained graph data are verified separately.

## Assertion and review ownership

The existing document integration suite owns the comparison. A skipped request
must report didWork false and retain the entire independent snapshot. Its native
request return is checked as admission information, not assigned to document
status. The assertion helper now separates fitting completion from availability
of a new method result. Its default preserves every preceding comparison; only
the explicitly cached-fitting case overrides completion to retain a complete
graph. First-chance fitting comparisons still require incomplete geometry.

Applied the ce-simplify-code reuse, quality and efficiency rubrics inline. No
independent reviewers ran because delegation is disabled by the user's rules.
The recorder reuses the frozen parser, geometry reader and guard chain. The new
packet encoder emits protocol 8 directly. No behavior-preserving cleanup earned
its cost: reuse 0, quality 0, efficiency 0 applied; skipped 0. Root lint/typecheck
scripts do not exist. Native compilation uses warnings as errors, followed by
JS/Python syntax checks and the owning document tests.

Consolidate-test-suites: MERGE the fixture and assertions into the current
document-cancellation suite. PROMOTE the observer, recorder, raw responses,
fixture and auditor as stable native reference evidence. Installed tests cover
package resolution and actual permission denials. No standalone regression
suite, runtime compatibility path or newly unreachable production code was added.

## Limits

These are idle scheduler cache hits on owned 8x8 opaque blocks and raw progress
checkpoints. Busy replacement, worker startup, automatic chaining, GUI timing,
editing/shadow interruptions and arbitrary inputs remain unverified. The three
known decoding, alpha-editing and batch-reset failures remain visible.

Only README changed among the tracked source files. The preceding full-core and
GUI results are retained as explicitly reused baselines, not new runs. The new
current/installed document checks and source/archive/installation identity are
recorded in the checkpoint. Original application/settings, previous captures,
root gui/dist and existing previews remain untouched. Publication and Rust/GPUI
remain pending.
