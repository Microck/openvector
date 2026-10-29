# Export resource retention and pending CRT bytes

The original save method leaves one open output-file handle after each observed
cancellation. Successful saves add no retained handle. The pending text bytes
explain the difference between files read after unwind and after process exit.
This makes the next file-owner design depend on measured ownership and buffering
rather than an assumed close-on-exception policy.

Three new, separately preserved observers use the existing original-runtime
catch boundary. Each repeats the same 30 save calls in two fresh processes:
six formats, complete/first-cancel/retry/last-cancel/retry, the declared 64x64
opaque grid, stacked layout and thin stroke. This is six native processes and
180 calls, not 90 distinct source inputs. No runtime source, existing test,
package dependency, browser host or user preview changed in this phase.

## Process counts and file identity

The first observer records GetProcessHandleCount before and after saving and
after the read-only file observation. Its cross-process absolute-count assertion
failed: the two initial counts were 151 and 143. Both ended exactly twelve
handles higher. The raw captures and failure log are preserved. That recorder
did not save its process intervals before the assertion, so those first captures
do not support a fresh EPS clock-interval claim.

The corrected measurement compares growth from each process's own initial
count, retaining all absolute values. This changes a faulty environmental
assumption, not an original output expectation. Each complete/retry save adds
zero, each cancellation adds one, and the file-reading observer adds zero.

Counts alone do not identify resources. A second observer reads the current
process's handle table before and after each save, then queries type and path
only for added handles. All twelve added handles per repeat are disk files, and
each path equals that cancelled output. The final observation still contains
all twelve. No handle is duplicated, closed, flushed or otherwise modified.
Successful saves have no added handle to query.

The diagnostic's x86 snapshot declaration follows the maintained
[System Informer header](https://raw.githubusercontent.com/winsiderss/systeminformer/master/phnt/include/ntpsapi.h).
It uses the read-only process query documented by
[Microsoft](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntqueryinformationprocess).
Returned byte length, entry stride and handle count must match before parsing.
This is an observer for the current host, not a new runtime dependency or a
claim about every Windows version.

## Initialized buffer storage

The original CRT initialization at 0x7c341cf8 maps twenty builtin FILE entries
from 0x7c38b4e8 to 0x7c38b768 at stride 32. Its _fileno implementation reads the
descriptor at offset 16. The retained DLL disassembly and DLL hash establish
those bounds before the third observer reads its data.

For each added OS file handle, the third observer finds active matching FILE
entries through the original CRT's own descriptor-to-handle function. It reads
only the initialized range between the live base and current pointer, after
checking that its length fits the recorded capacity. Reserved buffer bytes are
not captured. Ten cancelled text streams per repeat match, each with capacity
4096 and flags 10. Cancelled EMF handles have no matching entry in this examined
builtin CRT array; their disk-file identity remains separately verified.

First/last raw pending sizes are SVG 101/227, PDF 0/0, EPS 0/0, AI 1410/2 and
DXF 115/0 bytes. For the nonempty text buffers, expanding LF to CRLF reproduces
exactly the remaining physical bytes written at process exit. Thus first AI
cancellation has 1410 raw pending bytes representing 1456 physical bytes; first
DXF has 115 raw bytes representing 135 physical bytes. This is not a guessed
truncation length or a finalized-file-prefix substitution.

Normal outputs, progress, exception observations and retained document state
match the preceding original-runtime captures. EPS dates in the admitted groups
are checked against their actual recorded process intervals before normalizing
that field for comparison. No other native bytes are normalized.

## Verification and scope

PROMOTE the three observers, their build proofs, raw records, rejected absolute
count assertion, original CRT disassembly, primary reference fixture and auditor
as distinct measurement controls. They never ship in npm. Their invariant is
observable file and resource state across save interruption. Its primary owning
layer is integration. The eventual independent file behavior belongs in the
existing batch-cancellation/vector-output suites. No speculative standalone
runtime test or mock was added, and no newly unreachable runtime code exists.

The auditor reparses physical requests, complete responses, handle telemetry,
added-file records and initialized CRT buffers; verifies exact original-state
and file comparisons; and reconstructs the compressed fixture. It checks all
new observer hashes, unchanged runtime/source and existing test hashes, prior
sealed evidence, original installation/settings/desktop and protected gui/dist.
The previous core 933/935, GUI 671/672 and installed 2/2 checks are explicitly
reused because runtime bytes have not changed. No new full-suite run or package
build is claimed. The three known failures remain open.

The ce-simplify-code rubrics ran inline under the no-delegation rule. Existing
encoders, response parsers, proof checks and capture logic are reused. The common
capture function supports the actual three telemetry observers without adding
runtime compatibility paths. No simplification finding changes native outputs,
errors or ordering. Quality applied 1: remove an unused import introduced in the
new recorder. Reuse applied 0, efficiency applied 0, skipped 0, independent
reviewers 0. Root lint/typecheck scripts do not exist; Python parsing and
the actual native captures/auditor provide the relevant checks. Generated C
sources retain their frozen build identity.

The remaining design needs the complete flush schedule, behavior at large
buffer boundaries, post-open write errors and native task/controller cleanup.
Document-destruction cleanup has not been observed. A file owner that eagerly
closes on cancellation or equates emitted bytes with bytes readable at unwind
would contradict these captures. Independent file lifetime, full application
parity, publication and Rust/GPUI remain unfinished.
