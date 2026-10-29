# Vector export interruption reference

The current project, npm name, CLI, GUI source and README use OpenVector.
Historical Ambivalence snapshots and sealed reference records retain their
original names. No runtime source, existing test, dependency or user preview
changed in this reference phase.

The remaining export gap is file lifetime, not completed serialization. The
batch writer opens a file, renders all bytes in memory and writes afterwards.
A cancellation thrown during rendering therefore leaves an empty file. The
original writes during traversal and can retain both partial bytes and buffered
writer state after unwind. Adding a Cancel button alone would preserve that
mismatch.

## Original-method evidence

Three separately preserved numerical observers call original C1 save 0x468e50.
The original ABI passes a 28-byte wide string by value and a settings pointer.
The method consumes that string and returns with ret 0x20. The settings copy is
self-assignment safe according to the retained original instruction bytes.
The observer uses owned source pixels, controls, documents and output paths.
Original frames unwind through an external C++ catch after the original cancel
method is requested at a chosen virtual callback. No original instructions,
widget setters, readiness flags or exception contexts are altered.

OVSX/1 records result, progress, cancellation, PRNG, dimensions, settings bits,
exposed palette storage, keys and geometry container sizes. OVSX/2 gives EPS
style and stroke mode separate request words. OVSX/3 also opens each evidence
file for reading after original return or caught unwind, before another save.
This observer does not flush, close or finalize an original writer.

The admitted fixture contains 660 cases, each repeated in another fresh native
process: 390 completed exports and 270 interrupted saves. The 480-case settings
matrix covers six formats, three layouts, both EPS styles and stroke choices,
and three DXF curve modes. Ninety larger EMF cases exercise intermediate buffer
boundaries. Ninety further cases distinguish file bytes at unwind from bytes
after process exit. Calibration and earlier rejected mappings remain separate.

All 390 completed cases match the independent exporters exactly. Geometry comes
from the independent engine using declared BGRA pixels and controls, not from a
native geometry dump. EPS uses only the validated captured CreationDate as
environment metadata. EMF uses measured device values. These comparisons do not
establish interruption or general input parity.

Cancellation escapes this method without a return value. The raw status word
of UINT_MAX is external caller storage initialized before the call. It is not
stored C1 status. The fixture names it callerResultWord and keeps methodResult
null. Progress, cancellation and the unchanged document projection remain
separate. Native counts identify reserved cubic slots, not initialized curves.

## File lifetime

After process exit, partial SVG, PDF, EPS, AI and DXF outputs match prefixes of
completed files, with only the actual EPS clock field normalized. EMF can be
empty or contain complete drawing records with an unfinished 236-byte header.
Its bounds and frame are [0,0,-1,-1], byte and record totals are zero, and the
handle count is one. The body matches the completed body's prefix. The entire
partial file does not match a finalized-file prefix.

The OVSX/3 corpus provides 180 observations across two repeats. Of these, 148
file reads succeed and 32 fail with CRT errno 13. Every failed read is a
cancelled EMF save. Twenty successful reads contain fewer bytes than the files
read after process exit. These are cancelled SVG, AI and DXF saves. The PDF and
EPS observations in this sample are unchanged. Unreadable EMF bytes are unknown
at unwind; they must not be represented as an observed empty file.

For example, the first SVG cancellation exposes 141 bytes at unwind and 242
after exit. The first AI cancellation exposes zero bytes at unwind and 1,456
after exit. The first DXF cancellation exposes 19,289 bytes at unwind and 19,424
after exit. A larger EMF late cancellation leaves up to 65,512 bytes after exit,
while its immediate read fails. Native GUI cleanup and the underlying Win32
sharing error have not been observed, so no handle-lifetime conclusion beyond
the measured read result is admitted.

## Rejected observations and corrected interpretations

- The first launch used a forward-slash original executable argument. Import
  loading failed before document construction, with no save calls. A native
  Windows path fixed the invocation without changing the binary. The failed
  request and logs remain evidence.
- OVSX/1 initially labelled the EPS style word as thinStroke. That corpus is
  excluded from stroke conformance. OVSX/2 initially encoded the stroke Boolean
  directly; original enum 0 enables thin stroke and 1 disables it. Those earlier
  requests also remain excluded. The admitted captures use the correct inverted
  enum, verified against both original instructions and the existing SVG probe.
- A comparison initially treated allocated cubic slots as initialized curves.
  The independent initialized count is 112 for the smaller source, while the
  native container reserves 224. Using cubicStorageSlots fixes the comparison
  metric without changing geometry or expected bytes.
- A whole-file EMF prefix hypothesis failed. The later observation distinguishes
  unfinished headers from drawing records, then distinguishes unwind from exit.
  No exporter output or failing fixture was changed to hide these findings.

## Verification and coverage ownership

The new auditor decodes physical request words, reparses all retained native
responses, validates response/file hashes, checks repeated state and files,
checks actual EPS dates against each process interval, and rebuilds the admitted
fixture semantically. It verifies partial-file relationships, retry outputs,
document state and original cancellation exceptions. It also retains the three
new observer build proofs and checks the original installation, saved settings,
desktop, earlier helpers, sealed references and protected root gui/dist.

PROMOTE the original observers, failed diagnostic records, captures, compressed
fixture, completed-output comparator and auditor as reference evidence. The
invariant is file/task state at cancellation and retry. Its primary owning layer
is integration. When the independent file path is implemented, MERGE behavioral
coverage into the existing batch-cancellation and vector-output suites. No
standalone smoke test, mock or speculative implementation assertion was added.
No newly unreachable production elements were introduced or deleted.

The ce-simplify-code review runs inline under the explicit no-delegation rule:
reuse applied 0, quality applied 1 (remove a redundant identical Buffer.from
branch), efficiency applied 0, skipped 0, independent reviewers 0. Existing
capture and proof utilities are reused. Generated C variants retain their
frozen build identity. Separate helpers preserve distinct observation protocols;
they are reference tools, not runtime compatibility paths.

JavaScript syntax and Python parsing are checked. No root lint or typecheck
script exists. No new full runtime suite or package build is claimed: all 207
runtime/source files, 61 core suites, GUI tests and the preceding checkpoint's
files remain byte-identical. Previous checks are explicitly reused, including
the known alpha-palette and batch-background failures and the earlier Adam7
scratch-storage failure. No check was skipped or weakened to obtain parity.

There were 32 completed native helper processes, one earlier failed loader
attempt and 2,714 native save calls in this phase, including rejected mappings,
calibrations and repeats. There were no GUI input events or new browser hosts.
GitHub/npm publication, full parity and Rust/GPUI remain pending. The next work
must recover native task cleanup and save failure publication before selecting
the independent writer lifecycle.
