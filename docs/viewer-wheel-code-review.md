# Wheel arithmetic conformance

Wheel zoom must retain the original slider quantization and pointer anchor,
then clamp the requested center before integer scrollbar centering. The native
handler at 0x4474a0 advances the slider by signed delta divided by 12, with
truncation toward zero. Its anchor uses the old and new slider getters, which
remain distinct from displayed transform scale.

The isolated reference helper copies the reciprocal and anchor arithmetic at
0x44755a, 0x44756d and 0x44757c. It substitutes owned stack inputs, an owned
binary64 one constant and offsets into an owned center pair. Every arithmetic
instruction is retained. The auditor verifies the copied bytes, wrapper,
constant value and both displacement substitutions. The helper imports only
kernel32 and msvcrt; it initializes neither Qt nor the original application.

The accepted source-written getter formulas come from the existing frozen
scalar helper source. Its older entry point remains part of that unchanged
source; this helper executes only its own new entry point and numeric functions.
All getter bits are checked against the earlier 301-position original corpus.
The current helper and its source are now frozen by the build proof.

Two fresh processes produce identical 780,208-byte responses. All 8,127 cases
compare native instructions with separate source-written C arithmetic, bit for
bit. The auditor checks the complete planned grid: 301 old slider positions,
nine signed deltas and three pointer/center samples. These include odd and even
viewport sizes, fractional retained centers, same-position events, both clamp
edges and signed 32-bit delta extremes. The sampled grid does not exhaust all
possible old/new slider pairs or pointer coordinates.

The existing JavaScript expression matches each recorded center result after
the original center clamp. No runtime arithmetic change is justified by this
evidence. The unit test uses the original anchor bits and verifies slider
position, final center and immutability of the prior center. The native C
comparison separately checks unclipped intermediate anchor bits.

The original meta-method 38, setIsBatchMode(bool), dispatches to 0x44b6a0 and
writes flag +0x148. The wheel handler tests that flag before custom zoom and
delegates to QGraphicsView when set. OpenVector renders its separate batch
workspace without the single-image zoom handler. This static mapping does not
prove batch entry/exit, original input dispatch or native scrollbar behavior.

The ce-simplify-code reuse, quality and efficiency passes ran inline, with zero
independent reviewers. Applied findings: reuse 0, quality 1, efficiency 1;
skipped 0. The auditor now verifies every planned input combination rather than
relying on case count. The test reuses its three viewport objects. No product
export or existing reference helper became unused. There are no configured lint
or typecheck scripts.

The consolidate-test-suites invariant is native wheel-anchor arithmetic and
center clipping. Its primary owner is the unit layer in the existing
gui/test/viewer-input.test.mjs suite. The frozen helper, recorder, numeric
fixture, instruction audit and batch-guard disassembly are PROMOTE as reference
evidence. No standalone product regression suite, mocks or temporary product
instrumentation were added. The current compiler object is DROP; the source,
executable and accepted responses are retained. Pinned Qt wheel source excerpts
are PROMOTE as primary material for the remaining input-routing work.

The new isolated verification copy shares only immutable test fixtures through
hard links. Editable source is copied normally. Fixture hashes are checked
after the run. Earlier verified copies, the root build, source manifests,
lockfiles, core engine, CLI and all runtime GUI modules remain unchanged.
The production build is byte-identical to the preceding checkpoint. All 15
focused tests pass. The full GUI suite has 657 tests, 656 passing and the same
processing-batch-controls failure. All 288 shared fixture hashes are unchanged.
Browser interaction is unnecessary because no rendered product code changed.

Remaining parity work includes complete native event routing and guards,
horizontal hardware behavior, non-Chromium input, broader numeric and sequential
domains, scrollbar interactions, source/resize/batch lifetimes, the existing
batch-reset failure and full application verification. GitHub/npm publication
and the later Rust/GPUI port remain unfinished.
