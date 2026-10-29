# Initial Pencil movement and browser gesture admission

Expected behavior: a fresh Pencil move with the left button held uses the
view's previous world point, then advances it even when the line paints
nothing. The source constructor initializes that point to (-1,-1). The first
move therefore has an outside-image endpoint and adds no history; a second
valid move can paint. A hover without the left button leaves the anchor alone.

The first wrong effect was an exception in SegmentationEditor.gesture before
its native-equivalent command could run. A null sentinel imposed a first-press
requirement absent from the original move path. The source at 0x44614a checks
the left button, reads the previous point at +0x108/+0x110, submits the command
and stores the new point after dispatch. Constructor 0x4475d0 assigns -1 to
both coordinates. The input-mode handler has no direct anchor reset.

A new read-only helper records current and previous world points at startup
and after two automatic source loads. Two complete process runs agree, with
both pairs equal to (-1,-1) in all three snapshots. The helper invokes no
pointer/key events, native widget setters or dialogs. All earlier 42 helpers
remain frozen. These captures establish initial state; they do not prove the
complete pointer lifetime after real strokes or across every transition.

The independent editor now starts with that outside-image point. Its existing
command bounds check makes the first move a no-op, and the existing ordering
advances the point. The obsolete first-press diagnostic was removed as part of
this correction. No additional unreachable product code was found.

The browser now admits a held-button Pencil move entering the viewer without
inventing a press. It captures that pointer and retains every later sample in
the existing ordered queue. Processing and confirmations still block a new
gesture; an ordinary pending gesture request no longer does. Picker release
also reaches the model without requiring an earlier press in the viewer.
Navigation remains disabled while queued gestures drain.

The ce-simplify-code reuse, quality and efficiency passes ran inline with zero
independent reviewers. Applied findings: reuse 0, quality 2, efficiency 0;
skipped 0. The release condition has a named picker predicate. The recorder
also checks initial-control equality and freezes its two observer source files.
No lint or typecheck script is configured; C compilation passes Wall/Wextra/Werror.

The consolidate-test-suites invariant is first-move anchor advancement without
an artificial press. The unit owner is the existing segmentation gesture suite,
which checks exact pixels, history coordinates, hover handling and revert.
The existing GUI application HTTP test owns transport/worker acceptance of an
ordered command batch. Browser checks own DOM event forwarding and pointer
capture. These layers cover different failure modes and use no mocks.

Probe dispositions: the native observer, recorder, three-state corpus and
static instruction evidence are PROMOTE. The new assertions are MERGE into
existing suites. The current compiler object is DROP. Temporary product
instrumentation: none. Existing reference helpers and captures are preserved.

Verification: 17 focused gesture tests and 34 GUI application tests pass.
The full core run has 859 tests, 857 passing and the same encoded-PNG and
edited-alpha failures. The full GUI run has 657 tests, 656 passing and the same
batch-reset failure. No test is skipped or weakened. The production build passes.

A fresh 123-file npm install matches source/archive bytes and contains only one
changed package file, src/segmentation-editor.mjs. Its direct first/second move
checks pass with original/workspace reads and child creation denied. Its CLI
processes the no-press gesture plan and emits exactly the same SVG as the source
CLI. The new sequence's final refit was not compared with a fresh original run.

The original browser driver could not load its kernel assets. The installed
agent-browser skill provided an isolated headless session. Real mouse actions
verified the initial no-op, the next line's exact record, and picker release
without a press inside the viewer. The picker samples black and restores Pencil
without adding history. One test click initially needed PowerShell ref quoting;
its extra stroke was undone. Header text selection canceled another attempted
release sequence before reaching the viewer, so the valid release check starts
from a nonselectable toolbar button. No production behavior was changed to make
those harness mistakes pass. The final browser error list is empty.

The editable source copy, fixture hashes and fresh package are independently
checked. Only immutable fixtures use hard links. Both owned HTTP hosts and the
owned browser close normally. Existing user previews, root gui/dist, manifests,
lockfiles and original installation/settings remain unchanged.

Remaining work includes viewer-owned anchor lifetime across new segmentation,
source changes after real strokes, batch transitions, layer/input interactions,
native-equivalent behavior around browser text selection, and full application
parity. The known three suite failures, GitHub/npm publication and Rust/GPUI
port remain unfinished.
