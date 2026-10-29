# Buffered text file output

The text exporters now share native LF serialization with explicit flush
markers. In-memory output translates the collected text to CRLF. The Node file
owner fills a 4096-byte raw buffer and translates LF only when writing physical
bytes. PDF object offsets continue to count physical Latin-1 bytes, including
the binary header. Completed output return types and bytes stay unchanged.

Successful saves flush and close. A traversal exception keeps the actual
descriptor and pending buffer until process exit. Retained descriptors keep their
own file position when a later save truncates the same target. No footer is
appended after cancellation. This models the observed original leak rather than
closing early or reopening a filename to append a saved prefix.

SVG flushes line boundaries before tags and leaves the current path pending.
AI flushes dynamic path commands but leaves its initial header and final paint
pending. EPS/PDF command boundaries drain the buffer. DXF numeric group records
flush, while string, subclass and handle records do not. Fixed DXF records are
compiled once into ordered text/flush pairs; dynamic records use the same field
writer. No finished DXF is parsed back to guess flush offsets.

The internal file owner preserves the captured open-failure distinction. SVG,
PDF, EPS and AI return 4 before traversal. DXF traverses discarded output and
returns 0, retaining an I/O diagnostic separately. Unsupported, missing and
uppercase suffixes return 1. Dispatch uses a literal suffix comparison, including
the native batch filename `.svg`; Node's extname classifies that name differently.
BatchRunner uses this owner instead of rendering an entire file and closing in
a finally block. CLI exclusive-output admission remains separate and unchanged.

## Verification ownership

The invariant is actual file/resource state across cancellation, retry and
process exit. Its primary owning layer is integration. Extend the existing
vector-output suite with a child process helper, since process shutdown must
flush retained buffers after an earlier observation. Compute geometry from
declared pixels and controls. Never seed native expected geometry or normalize
current output. Supply already interval-validated captured EPS dates as
environment metadata.

Each final lifecycle run attempts 558 saves: 470 text interruption-corpus cases,
25 resource cases, 15 long-path cases, 25 same-target steps and 23 file-error/suffix steps. Compare
completed and post-exit bytes, immediate bytes where native captures exist,
method results, cancellation/progress state, absent files, directories and
unchanged read-only markers. Actual Windows process handle growth is 10 for
the resource sequence, 5 for the long-path sequence, 10 for same-target retry
and 0 for file-open failures.
The existing serializer test owns emitted-byte ordering, consumer-error identity
and graph immutability. Existing batch and CLI suites own controller output and
admission behavior. These are distinct failure modes, not duplicate lifecycle
assertions in several layers.

PROMOTE the process helper and MERGE lifecycle checks into vector-output. No
standalone smoke suite, mock, test-only runtime cleanup or descriptor-inspection
API was added. The helper is small because it owns a real IPC/process boundary.
No task-created runtime probe remains. The superseded local package was rebuilt
before installation. Failed path-pilot, suffix-test and test-runner permission
logs remain diagnostic evidence; their outcomes are not counted as passes.

The first file test used fixture IDs as filenames; IDs contain `/`, causing an
unintended missing parent. It now uses numeric owned filenames while retaining
case IDs in assertions. The broader suite then found `.svg` dispatch and both
native filename tests passed after the suffix correction. Permission tests run
with test isolation disabled because the normal test runner itself needs a
child process. Child creation remains denied inside the installed serializer
checks. The separate installed lifecycle run explicitly uses real children.

The long-path reference repeats 15 original saves in two fresh processes using
the existing frozen CRT-buffer observer. An explicit 128x176 comb raster yields
14 fitted regions. Its third and fourth SVG paths are 6664 and 6506 bytes long.
After the fourth path, cancellation leaves 2410 raw bytes pending and an already
written 4096-byte portion of that path. Independent geometry and physical files
match at unwind and exit across the five text formats. PROMOTE this recorder,
its raw request/response/handle/buffer data and compressed fixture as a distinct
rollover control. No new native helper or original instruction modification was
needed. General all-input buffer behavior remains unproven.

The full core run overlaps the addition of this last 15-case data set to the
existing lifecycle test. Runtime bytes stay fixed. Preserve the earlier test
source and hash, then rerun the entire vector-output file and installed lifecycle
checks with the extended corpus. The focused final checks own the added cases;
the full core result is not presented as proof of that later test revision.

The initial GUI run against root gui/dist also finds the historical Ambivalence
build. That artifact is protected, so the complete GUI suite runs again from an
isolated source tree with newly compiled assets. The first isolated setup lacks
fixtures and fails before execution. Populate immutable fixture hardlinks and
run the unchanged suite again. Preserve both setup failure logs. The fresh
complete suite retains only the known processing-reset failure. The auditor
checks source/archive/installed identity and unchanged reference hashes after
these runs.

## Review and remaining scope

The ce-simplify-code setup and all three rubrics ran inline because the user's
no-delegation rule applies. Reuse applied 0, quality applied 0, efficiency
applied 0, skipped 0, independent reviewers 0. The shared serializer is the
canonical representation. Retained resources are required observed behavior;
an eager cleanup would change it. No newly unreachable symbol remains after
replacing the old batch writer. No separate compatibility path was retained.

JJ is unavailable and this directory has no Git metadata. Source/test hash
ledgers preserve the baseline. Root and GUI packages define no lint/typecheck
scripts; full test suites and an isolated GUI build cover the changed seams.
The build writes to its own artifact directory, preserving root gui/dist and
user preview channels. The npm archive remains private and unpublished.

This does not establish general all-input buffer behavior, post-open disk
failure semantics, EMF lifetime, original GUI cleanup/cancellation or complete
application parity. The prior decoder, alpha-edit and processing-reset reference
failures remain open. GitHub publication and the Rust/GPUI port remain pending.
