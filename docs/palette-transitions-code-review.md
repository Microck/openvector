# Palette state across retained document work

The existing implementation matches 64 additional native workflows through both
document APIs. The source engine and GUI did not change. The document suite now
checks 24 reduced-grid cases, 24 cases after fitting, and 16 cases after an earlier
cancellation. Those workflows contain 80 cancellations, compared through both
APIs. Throwing palette calls retain the preceding API result, including status 2
from segmentation cancellation; a successful retry records 0.

The reduced cases use declared 512x400 four-color blocks. Palette work changes
the grid dimensions before assignment. Old exported region records survive,
while labels remain unavailable until the producer runs. The fitting cases
request advanced palettes from 4 through 24 colors, cancel at the first, second
or final notified checkpoint, retry, and fit retained contours with changed
advanced Curve Complexity. Tests compare actual retained region, node and cubic
container counts. Every final advanced native fit must emit progress.

The existing frozen protocol-7 observer captured 180 fresh processes: 176 for
64 cases and 24 baselines, plus four for the initial direct-call pilot. All
responses repeat byte for byte. No new native helper, business-method patch,
widget setter, readiness flag, GUI input or thread startup was introduced.

## Direct calls and skipped tasks

The initial two-case pilot requested Curve Complexity while its final fit stayed
in basic mode. That control is ignored there. The direct native fit emitted no
work progress but reloaded its basic profile. The independent task API skipped
the completed fit and kept the palette profile instead. The direct comparison
passed; applying that same direct reference to a task produced a shared-control
mismatch. The failed log and raw pilot remain preserved as limited evidence.

The paired fixture requests advanced fitting and requires real native progress.
This changes the workflow to exercise its intended operation. It neither widens
runtime admission nor changes cache behavior or numerical assertions. The pilot
is not counted among the 64 paired workflows. Its fixture remains consumed by
the reference auditor, outside the paired runtime test matrix.

## Review and ownership

Applied the ce-simplify-code reuse, quality and efficiency rubrics inline, with
zero independent reviewers. The new recorder reuses the frozen packet encoder,
parser, guarded capture, byte comparison and candidate protection. Scenario
functions keep their distinct prefix and retry assertions explicit. One writer
owns report and fixture framing. No runtime abstraction or compatibility path
was added. Reuse 0, quality 0, efficiency 0 applied; skipped 0. No root lint or
typecheck script exists; JS/Python syntax checks cover the changed code.

Consolidate-test-suites: MERGE the new cases and assertions into the existing
document-cancellation integration suite. PROMOTE the native recorder, raw
responses, fixtures and auditor as reproducible reference evidence. The pilot
has a separate reference-validity role, not duplicate paired API coverage.
Installed checks own package resolution and actual permission denials. No new
production code became unreachable and no deletion is pending.

## Verification boundary

The final current and installed document suites include the added container
assertions. The initial runs and scoped checks remain preserved. Only the README
changed among the 205 tracked source files; engine and GUI hashes retain the
previous 917-test core and 664-test GUI results. Those are explicitly reused
baselines, not new full-suite runs. The package archive's 125 files match source
and a fresh installation. The installed suite cannot read the original program
or workspace source or create child processes.

The original application/settings, earlier references, root gui/dist and existing
previews remain unchanged. No VCS mutation or publication was performed. The
known decoding, alpha-editing and batch-reset failures remain open, as do broader
editing/shadow interruptions, GUI scheduling, arbitrary inputs and full parity.
GitHub publication and Rust/GPUI remain pending.
