# Completed palette analysis to segmentation interruption

The independent document API now has exact comparisons for 1,770 additional
segmentation cancellation/retry cases after completed palette analysis. Both
the direct methods and runTask pass all cases. The numerical implementation
needed no change. The same live-label/palette owner and separately retained
exported records already reproduce the original behavior.

The workflows use declared 8x8 four-color blocks and a 9x9 gradient, fixed and
unlimited palettes, aliased and antialiased profiles, and raw or changed-integer
progress observation. Warm workflows first complete segmentation, analyze the
palette, then request a different quality to force new segmentation. Every
observed cancellation ordinal is exercised, followed by same-document retry.
There are 32 uninterrupted baselines and 3,604 fresh workflow processes.

## Observation correction

Protocol 4 added completed calls to the native palette business method without
changing original instructions or involving any window/widget. Its 29-row
calibration matched 171 preceding protocol state comparisons and four older
completed palette/segmentation states. However, those older shared fields did
not prove label availability. The inherited observer knew the segmentation
producer but omitted the palette producer, so its false availability flag could
not support handoff comparisons. Its source, binary, recorder snapshot and all
raw captures remain preserved as limited evidence.

The saved assignment pseudocode shows every pixel receiving a selected palette
ID. Original bytes e8f7150200 at 0x46dca4 call candidate generation at 0x48f2a0;
its successful return is 0x46dca9. Protocol 5 adds a fourth hardware observation
point there, marking storage defined only after that actual producer. A cache
hit does not visit the point. No value-based inference, zero filling, allocator
residue reading, original instruction patch, native GUI input or widget setter
was added. This protocol deliberately forbids palette cancellation and makes
no claim about label availability within interrupted palette assignment.

The corrected protocol repeats the 171 prior state comparisons and four older
completed shared-state comparisons, and now exposes initialized palette labels.
Both calibrations account for another 124 fresh native processes. The final
auditor reparses all 3,728 new responses, verifies encoded requests and frozen
fixture rows, checks the original call bytes, and protects the 43 accepted
helpers, three earlier numeric candidates, original files/settings and desktop.

## Review and test ownership

Applied ce-simplify-code's reuse, quality and efficiency review in one context;
zero independent reviewers ran. The recorder reuses the established parser,
geometry reader, state comparison and original-state guards. The existing
document-cancellation integration suite owns the new workflow. Its only new
dispatch support is for the actual palette API and its existing progress shape.
The initial direct harness used the wrong analyze() return shape; the failure
log is retained, and the corrected call passes without changing the public API.
No additional worthwhile simplification was found. Reuse 0, quality 0,
efficiency 0, skipped 0. No root lint/typecheck script is configured; native
compilation uses warnings as errors and final JS/Python syntax checks pass.

Consolidate-test-suites disposition: MERGE new assertions into the existing
document integration suite; PROMOTE the native fixture and recorder/auditor.
Native outputs are assertions only. Declared pixels and controls supply all
independent inputs. The installed copy checks package resolution and real Node
permission denials, a distinct boundary. Failed and limited captures remain
evidence. No application code became unreachable and no deletion is pending.

The new installed harness owns a fresh consumer. Two harness writes initially
landed in the preceding consumer; only those changes were undone using its
recorded source, and the auditor verifies the preceding package and test set.
An extra fixture copy remains in evidence. Historical archives/previews and
root gui/dist are preserved. No VCS metadata, commit, push or publication was
created. JJ remains unavailable.

## Verification limits

The full document suite passes 26 tests. The fresh installed consumer passes
27 tests, including actual denied original/workspace reads and child creation.
Every one of the 205 application/source files remains byte-identical to the
preceding checkpoint. All 125 archive files match source and both installations.
A second full numerical or GUI run is not claimed for this test/evidence phase.

Large-image working-size reduction, cached or interrupted palette handoffs,
old fitted-graph validity, editing/shadow contexts, broader original scheduler
behavior and the three known failures remain open. Full parity, publication
and Rust/GPUI porting remain pending.
