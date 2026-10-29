# Canonical incremental text serialization

SVG, PDF, EPS, AI and DXF now expose one incremental serializer each. The
existing in-memory export function collects that same serializer's output.
There is one geometry traversal and one implementation of each format.
This removes the whole-output string as a prerequisite for a consumer to receive
bytes, which is necessary for matching interrupted file writes.

The consumer receives initialized headers before traversal checkpoints. A thrown
progress or consumer exception stops emission immediately, preserves the exact
exception and emits no footer. A successful traversal emits the existing footer.
Input and consumer validation happen before the first emission. PDF tracks
single-byte positions without retaining the whole document in its serializer;
its existing in-memory API still returns an owned Uint8Array. AI converts each
emitted chunk to CRLF rather than converting an accumulated document afterwards.
Coordinates, formatting caches, graph traversal, palettes and output options
remain unchanged.

The public functions serializeSvg, serializePdf, serializeEps, serializeAi and
serializeDxf take existing options and a synchronous write(text) callback.
PDF strings represent Latin-1 bytes, including its binary header. Other formats
emit ASCII. The contract describes these byte representations explicitly.
No asynchronous consumer, native flush boundary or filesystem result is inferred
from this text callback.

## Differential coverage

MERGE the new integration invariant into test/vector-output.test.mjs. It uses
the retained native export-interruption fixture, computes geometry independently
from declared source pixels and controls, and runs serialization through actual
ImageDocument export cancellation. It compares all 470 text cases: 282 completed
outputs and 188 interrupted prefixes. These cases use the declared 64x64 grid
across layouts, stroke options, EPS styles, DXF curve modes and retry sequences;
they are not 470 different source images.

Completed cases also verify that the existing in-memory export returns exactly
the same bytes. Consumer failures preserve exception identity and occur before
any traversal progress. The test verifies input geometry remains unchanged and
malformed consumers are rejected. No mock or separate smoke suite was added.
The owning layer is integration because emitted byte order and document
cancellation interact across components. Existing format tests continue to own
their numerical and format-specific invariants.

The initial focused check passed. There was no claimed pre-change failing run:
the prior API had no incremental serializer, and changing the old collector was
part of implementing the new seam. The installed-package check passes both the
same 470-case integration test and real original/workspace-read and child-process
denials. Fresh full core and isolated GUI results are recorded in the checkpoint.
Their pre-existing failures remain visible; no expected bytes, fixture, test or
admission constraint was weakened.

The fresh full core result is 933/935. Its existing Adam7 scratch-byte and
alpha-palette-index failures remain. The fresh full isolated GUI result is
671/672, retaining the batch-background failure. The focused integration check
passes 1/1 and the restricted installed checks pass 2/2. The GUI build passes.
The full smoothing corpus completed normally; it was not restarted or curtailed.

The package remains private and unpublished. Its source, local archive and
installed bytes must match. The isolated GUI production build uses the current
serializers; protected root gui/dist and user previews are untouched. No visual
UI change is claimed, so no new browser session or screenshot is used as proof.

## Original-runtime unwind control

Before selecting a file lifetime model, a possible observer effect needed
checking: the earlier external catch used vcruntime140, while the original uses
MSVCR71. A new separate observer forwards its frame handler to the actual
MSVCR71 __CxxFrameHandler and changes only the newly compiled observer's FuncInfo
magic to the target compiler's version. Both the original compiler object and
the modified observer object are retained, with the exact changed offset and
hashes. Original instructions, exception contexts and document results are not
patched, and no readiness/admission override is used.

Thirty cases repeated in two fresh processes cover complete, first-cancel,
retry, last-cancel and retry saves for all six formats. The original-runtime
boundary matches the earlier document state, exception observations, progress,
file readability, immediate bytes and post-exit bytes. EPS clocks are validated
against each actual process interval before that one field is normalized.
This rules out the suspected runtime effect for these observations; it does not
establish general compiler interoperability or GUI cleanup behavior.

PROMOTE this observer and its raw traces as a measurement control. It never
ships in npm. The preceding 43 accepted helpers, eleven numerical candidates
and earlier captures remain unchanged. No task-created temporary production
probe or newly unreachable production symbol remains.

## Simplification and remaining work

The ce-simplify-code rubrics ran inline under the explicit no-delegation rule.
The canonical collector and consumer validation already provide shared reuse.
Quality applied 1: remove a meaningless identity assignment in the new auditor.
Efficiency applied 1: load its EPS clock validator once rather than inside the
comparison loop. Reuse applied 0, skipped 0, independent reviewers 0. No configured
root lint or typecheck exists. JavaScript syntax and Python parsing are checked
alongside the actual runtime and package verification.

Changed runtime files are the five text format modules and vector-output.mjs;
README documents the new APIs. The existing vector-output suite owns the new
coverage. No numerical algorithm, source image handling, classifier, settings,
batch controller, GUI job lifecycle, EMF serializer or dependency changed.

The file owner still buffers the full export before writing. Native flush
positions, CRT buffer and handle ownership, EMF finalization, save-error result
differences and GUI export cancellation remain unfinished. Connecting the new
callback directly to a file and calling that parity would ignore those measured
differences. Full application parity, publication and Rust/GPUI remain pending.
