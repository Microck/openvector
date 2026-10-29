# Background source-lifetime review

The ce-simplify-code context script and all three shipped rubrics ran inline.
Delegation was not authorized. Independent reviewer contexts: 0. Applied reuse,
quality and efficiency findings: 0/1/0. The harness's empty-image branch was
indented consistently. One low-value efficiency suggestion was skipped: adding
another BackgroundEditor method solely to avoid snapshots returned by existing
revert/finish operations during infrequent source changes. No new unreachable
elements were identified. No lint or typecheck is configured for JavaScript/JSX.

Expected behavior: Start leaves background mode while retaining removal history;
source replacement and full Reset unhide existing visibility slots and transfer
Undo into Redo. Fresh vector acceptance updates the slot count and history.
The invariant is that background visibility outlives individual numerical image
states within a document/session owner.

The initial native-driven harness passed ordinary undo/redo, Export/Back,
re-entry and reprocessing. Reset failed with independent count 0 versus native
count 2. Source replacement also found a harness assumption: native's empty
QImage has dimensions/format zero and no pixels, corresponding to no displayed
segmentation. That branch now asserts the empty representation explicitly;
nonempty images retain every pixel comparison.

The first unintended write recreated BackgroundEditor in ImageDocument's source
reset. GuiEngine's full Reset then recreated the entire ImageDocument as well.
This discarded slot count and redo history even though core settings, keys,
parameters and pixels otherwise followed native reset. The fix retains one
background owner, calls its existing revert/finish kernels on source reset, and
clears numerical state through ImageDocument.reset rather than replacing the
document object. Separate documents still own separate visibility instances.
GuiEngine.snapshot exposes owned background metadata even with no open source;
GET /api/state continues to return null then.

Seven paired workflows use 14 native processes and retain 67 complete documents
and 182 background observations. The harness compares all seven ordered history,
visibility and available active-flag observations. The final case merges two
regions into one and proves Redo of old ID 1 moves it into Undo without hiding
the remaining region. The preceding six fixture rows remain an exact prefix.
No native input events, dialog answers or instruction patches are used.

Together with the preceding guided/history corpus, 27 available workflows compare
221 complete documents and 22,100 typed parameter records. The extra test is the
explicit hidden Play-on-Export observation. Intermediate task snapshots and
individual editor record fields remain retained evidence, not extra independent
conformance comparisons.

Frozen GUI 297/297, installed direct 264/264, HTTP 6/6 and installed CLI 19/19
pass. The new package retains 63 files and changes only image-document.mjs from
the preceding package; GUI engine bookkeeping is verified separately. Frontend
sources, locks and built bytes are unchanged. The browser passes 13 UI checks,
six native state projections and a separate closed-source Reset assertion.
Projection checks include exact controls/counts, keys, palette candidates/scores,
ordered background history, active state and SVG with hidden regions omitted.

The first lifecycle run reached its 180-second execution deadline. Its parent
and children were confirmed absent before a rerun against the same hash-verified
snapshot with a 600-second deadline. The partial timeout log is retained and is
not acceptance evidence. The replacement lifecycle run passes 354/354 tests with
no skipped or cancelled tests. Its scope covers 18 files containing document,
source, batch, profile, background and CLI tests. The earlier full-root result
of 709/711, with two native-memory-dependent failures, remains separate historical
evidence. This change does not resolve those failures or establish full parity.
