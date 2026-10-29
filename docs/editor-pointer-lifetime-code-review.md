# Retained editor pointer context

The next held-button Pencil movement must use the viewer's previous world
point after replacing a segmentation or source. A background press updates
that same point, including a press whose hit test changes no visibility.
Direct edit commands do not change the point. Independent documents must not
share it merely because they share a random session.

The first wrong effect was ownership: SegmentationEditor stored the point,
so constructing a new editor reset it. BackgroundEditor could not advance
that editor-local value. ImageDocument now retains one small pointer object
and supplies it to both gesture handlers. No factory, compatibility branch,
optional argument, additional validation pass or public wire shape was added.

Static evidence distinguishes timing. Pencil press at 0x446501 stores the
point before dispatch. Pencil move at 0x44614a reads the point and stores the
new one after dispatch. Background press at 0x44687f stores it before hit
testing. The source-transition handler at 0x44b590 has no anchor reset.
The constructor initializes it to (-1,-1). The bounded member scan excludes
stack offsets, and the dispatch-table evidence identifies the relevant
handler entries; indirect aliases still require explicit handler inspection.
No new native helper, native pointer input, widget setter or dialog was used.
This is static dataflow evidence, not a new native post-stroke capture.

The ce-simplify-code reuse, quality and efficiency reviews ran inline with
zero independent reviewers, as required by the no-delegation constraint.
The shared owner avoids duplicating pointer lifetime across editors. Validation
still occurs once at the gesture boundary and returns an owned point, so no
caller-owned array leaks into retained state. The update order remains explicit.
No further behavior-preserving simplification was justified: applied reuse 0,
quality 0, efficiency 0; skipped 0. No residual unreachable product code was found.
No lint or typecheck script is configured in this JavaScript project.

The consolidate-test-suites invariant is shared pointer lifetime across editor
replacement and background input. The existing segmentation gesture suite owns
this at the ImageDocument integration boundary. Seven cases cover source,
reset, actual recomputation, background gesture versus direct command, failed
background hit and independent documents. Five proposed cases fail against the
previous code before the fix; the independent-document case was added later.
The focused gesture/background run passes all 62 tests without mocks or skips.

Browser checks cover a different boundary: a real FileReader, HTTP worker,
React source adoption and trusted mouse samples. Two DOM drops contain an
owned BMP File; Windows shell drag/drop is not exercised. After a press at
(3,3), undo and a same-file reload, a held-button move entering at (5,3)
produces exactly one record with coordinates [5,3,3,3]. Picker input between
these operations leaves the anchor intact. Undo preserves a non-fit zoom;
reloading the same bytes fits the image again. No viewer source change was
needed because ordinary edit responses already omit the source bitmap.
The final browser error list is empty and the owned browser and host close.
A mistyped browser get subcommand failed before mutation; its corrected read
was used for the recorded assertions.

Probe dispositions: static assembly, dispatch/member evidence, browser drop
producer and bounded verification reports are PROMOTE. New behavior assertions
are MERGE into the existing suite. No new temporary native probe, instrumentation
or compiler object exists. The frozen previous sources and all 289 reference
fixtures are preserved; only immutable fixtures use hard links.

The production build passes. The core run has 866 tests, 864 passing and the same
encoded-PNG and edited-alpha failures. The GUI run has 657 tests, 656 passing
and the same batch-reset failure. No test is skipped or weakened. The final
checkpoint auditor verifies 202 copied sources, 289 immutable fixtures, all 43
frozen helpers, original settings, package identity and the closed owned host.

A fresh 123-file npm archive differs from the prior pointer package in only
three source modules. The installed direct API verifies source replacement and
background sharing with original/workspace reads and child creation denied.
The consumer-only read grant also runs the installed CLI's cross-source plan;
its SVG bytes equal the source CLI. The new sequence's final native refit has
not been compared. Package publication and the Rust/GPUI port remain unfinished.

Broader window/batch ownership, real native after-stroke transitions, layer
input routing and full application parity remain open. The known encoded-PNG,
edited-alpha and batch-reset suite failures remain visible.
