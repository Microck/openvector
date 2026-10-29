# Interrupted palette state

The palette refinement loop now supplies its actual colors and initialized labels
to the existing private document service before each assignment. The document
publishes that view before notifying the host. The public observer still receives
progress metadata; snapshot() returns owned copies. Palette arithmetic, random
draw order and the completed-candidate path remain unchanged.

Previously the document published only completed candidates. It therefore had no
live map at the first callback and retained the previous candidate's palette at
later callbacks. The frozen native comparison fails before the fix and passes
after it. The first unintended omission was at the kernel/document boundary,
not in snapshot serialization or cancellation handling.

## Native reference

Protocol 7 is a separate numeric observer. It leaves all 43 accepted helpers and
six preceding numeric candidates unchanged. It verifies the original call bytes
at 0x48dc4c and uses a hardware point at its 0x48dc51 return. The palette finder's
label-grid pointer must alias the core map. This proves a full-grid assignment
has run; allocated storage and its values do not establish availability.

An external C++ catch receives the real cancellation unwind from the unchanged
palette business method. A throwing method has no result. The recorder retains
its UINT32_MAX caller sentinel, and tests compare the independent document's
retained state without manufacturing a method result.

Calibration preserves 31 previous rows across 62 fresh processes. Twelve pilot
rows use 24 processes. The matrix uses 992 processes for 16 baselines and 480
cancellation cases: owned blocks and a gradient, aliased/antialiased profiles,
raw/changed-integer observers, cold/prior-segmentation state and counts 2..6.
Every response repeats byte for byte. First-chance and post-unwind palette states
match. Candidate counts above the block image's peak count exercise retained
random draws. Retry is compared with its actual native result; it need not
reproduce the random state of an uninterrupted run.

## Review and test ownership

Applied the ce-simplify-code reuse, quality and efficiency rubrics inline. Zero
independent reviewers ran because delegation is disabled by the user's rules.
The existing refinement loop remains the only arithmetic path. The private
service carries live arrays, while public snapshots own their buffers. Optional
kernel callbacks avoid allocating diagnostic views for standalone palette work.
No additional abstraction or behavior-preserving cleanup was warranted. Applied
findings: reuse 0, quality 0, efficiency 0, skipped 0. No root lint or typecheck
script is configured. Native sources compile with warnings as errors.

Consolidate-test-suites disposition: MERGE the unique live-state assertions into
the existing document-cancellation integration suite; PROMOTE the numeric
recorder, observer, raw captures, fixture and auditor as reproducible reference
evidence. The earlier palette tests retain distinct callback/cache coverage.
The installed test owns package resolution and actual filesystem/child-process
permission denials. No temporary runtime probe, newly unreachable production
code or deletion is pending.

## Limits

These checks establish the captured numeric palette boundaries. They do not
establish original GUI timing, scheduler work, busy replacement or automatic
chaining. The known image-decoding, post-edit alpha and batch reset failures
remain visible. Editing/shadow interruptions, old graph validity, broader inputs
and full application parity remain unfinished. GitHub publication and the
Rust/GPUI port remain pending. No original application input, widget setter,
instruction patch, VCS mutation or publication was performed.
