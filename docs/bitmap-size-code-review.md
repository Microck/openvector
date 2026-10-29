# Bitmap size backend review

BitmapSize owns one model scale and separate basic/advanced field values.
Explicit source opens refresh both panes. Model-only scale assignments retain
their displayed fields, matching the five immutable native control workflows.
The selected pane uses the recovered width, height and percentage handlers;
its peer retains field values until refresh. Native small/large increments
and coupled bound correction are represented directly.

The Windows host reads GetThreadLocale and four GetLocaleInfoW values through
an owned PowerShell boundary. The pinned Qt source uses those same queries
and takes the first code unit. Positive sign is fixed plus and all valid
pinned locale records use exponent e. The worker creates its numeric context
on first text commit. Product runtime reads no Vector Magic or Qt files.
The real host query matches the immutable native es_ES symbol capture.

GET and POST bitmapSize return the owner's snapshot. Document state projects
that snapshot. Default bitmap downloads use its scale; explicit download
scale remains local. Existing origin, serialization, pending-save and batch
guards still apply. No persistence schema or rendered control was changed.

## Correctness boundaries

Size changes compute on a small draft. Repeated axis/boundary correction
proves a cycle, returning a diagnostic without committing the size state.
This protects the host, but native interactive behavior for such cycles is
unverified and remains a parity gap. Source decoding completes before size
refresh; ordinary valid source replacement then uses the same decoded input.

The source imageChanged handler was read from original instructions at
0x41cf50. True reads QImage width/height and calls updateFromScale; false
returns without changing fields. Existing metadata fixtures establish the
ordinary source and model-scale cases. Further preprocessing and batch-related
bitmap-field transitions remain to be compared before UI lifecycle completion.
Transient native signal ordering is not exposed as an HTTP event protocol.

## Review and verification

The consolidate-test-suites invariant is settled size behavior at the owning
boundary. Component unit tests live in gui/test/bitmap-size.test.mjs alongside
the canonical GUI suites. Existing export-preferences tests own HTTP/worker
transport and whole-file comparison. Numeric primitive tests remain in their
own suite. These layers detect different failures. No mocks, expected-geometry
computation inputs, duplicate standalone tests or temporary product probes
were added. Original expected records remain unchanged.

All three ce-simplify-code rubrics ran inline with zero independent reviewers.
Reuse applied zero changes. Quality applied one clarification of assignments
and boolean grouping. Efficiency applied one change: new POST operations
return the already-owned size snapshot directly, avoiding an unused clone and
unrelated document projection. Inspection found no caller outside the current
unshipped scope; all scoped callers use the final shape. No alias or migration
path was retained. No existing code became unreachable; no findings were skipped.

The 137 scoped tests pass with no failures/skips. They include the existing
91 numeric cases, all UTF-16 whitespace classifications, five native field
workflows, size corrections and endpoint locks. All 34 complete native bitmap
files match through both explicit scale overrides and shared percentage-text
updates, for 68 file comparisons. The fresh isolated build succeeds. Its full
GUI suite passes 610 of 611 tests with no skips. The remaining failure is the
previously recorded batch backgroundMode 2 admission comparison. The workspace
run before simplification passes 609 of 611, also encountering the preserved
historical preview title. No failure was skipped, weakened or hidden.

JavaScript, PowerShell and contract syntax checks pass. The project has no lint
or typecheck scripts. The current isolated source identity and reference guard
are recorded in evidence/bitmap-size-verification-checkpoint.json.

## Remaining work

Rendered size controls, bitmap format/directory/filename choices, persisted
scale, direct save decisions and remaining lifecycle cases are incomplete.
The new text-locale host boundary currently requires Windows. Existing
previews, the original installation/settings, accepted helpers and packaged
core/CLI files remain unchanged. Full parity, publication and Rust/GPUI work
remain unfinished.
