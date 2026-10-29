# Viewer center and scrollbar checkpoint

The canonical viewport state now owns image dimensions, viewport dimensions,
scene bounds, integer scrollbars, the native retained center and Qt's requested
center. The browser renders the resulting integer origin and maps editing input
through the same transform. Fit, 100%, zoom, wheel, drag, toolbar steps and scroll
controls use this state. There is no old x/y translation path.

The read-only native helper records cold, wide and tall workflows twice each.
Both populated source fits match scene bounds, scrollbar values/ranges/steps,
indents, requested center, retained center and owned inverse point mapping.
Replacing the pinned Qt adjoint/determinant inversion with direct division
changes the observed mapped coordinates by one binary64 step. Native integer
translation zero has no sign bit; the test expectation uses integer subtraction
instead of introducing JavaScript negative zero through unary negation.

The Qt 4.4.3 archive is pinned at SHA-256
`7b4c97e460d391787fb3406b7a36785f638fd70e5d56a73d980cf8a17d5a6b22`.
Its QGraphicsView and QAbstractScrollArea source explains range rounding,
centering truncation, resize anchors, keyboard steps and center notification
ordering. Original imports and the meta-call jump table identify toolbar steps:
left subtracts horizontal singleStep, right adds it, down subtracts vertical
singleStep and up adds it. Keyboard Up/Down instead use Qt's standard vertical
subtract/add actions. The earlier cursor-helper classification was incorrect;
the historical review now points to the corrected evidence.

Browser CSS pixels define the modern viewport. Both axes reserve the measured
17-pixel scrollbar extent. Shadcn sliders expose actual integer ranges; an empty
range renders a disabled control because Radix's percentage formula divides by
the range. The controls retain keyboard access and visible focus. Scrollbar
track clicks and autorepeat still require conformance work.

The ce-simplify-code reuse, quality and efficiency passes ran inline, with zero
independent reviewers. Applied findings: reuse 0, quality 0, efficiency 0;
skipped 0. Native-center state and requested-center state are deliberately
distinct. ResizeObserver and canvas effects synchronize external browser APIs.
No superseded exports or unreachable code remain in the changed product files.
There are no configured lint or typecheck scripts.

The consolidate-test-suites invariant is exact source-fit geometry and bounded
integer scrolling. Its primary owner is the unit layer in the existing
`gui/test/viewer-input.test.mjs` suite. The new native fixture, frozen helper,
recorder and static evidence are PROMOTE: they preserve the external reference
contract. No standalone product regression suite or mocks were added. Temporary
product instrumentation: none. No current-task probe requires deletion.

Verification: 14 focused tests pass; the isolated production build passes;
the complete GUI suite has 656 tests, 655 passing and the existing
`native processing reset: processing-batch-controls` failure. Its public batch
schema still rejects the setter-only background mode 2. That test was neither
weakened nor skipped. Core, CLI, package manifests and lockfiles are unchanged.

The browser verified settled Fit geometry, toolbar step directions, viewport
ArrowDown, horizontal Home/PageDown bounds and shared bitmap getter scale.
The driver then timed out and could not reconnect after one reset because its
kernel assets were unavailable. Drag and download completion are not claimed.
The screenshot comes from a fresh read-only Iris capture of the isolated host.
The owning unit test covers drag clipping; existing HTTP tests cover complete
native bitmap bytes, which are different failure modes from browser input.

Unfinished work includes cold-placeholder geometry, RTL, original drag-input
transitions, exact wheel-anchor arithmetic, full resize and source-load
lifetimes, batch transitions, native scrollbar interactions and full
application parity. This checkpoint does not establish 1:1 parity or readiness
for GitHub/npm publication or the later Rust/GPUI port.
