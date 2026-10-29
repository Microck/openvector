# Source metadata after cancelled batch loads

Two native comparisons exposed stale metadata after completed-load cancellation.
The numeric image was already discarded, but GuiEngine retained its attempted
source path and Quick Save folders. The original MainModel clears that path and
sends the normal empty-source event to window owners. Both folders become the
parent of the working directory while visible filenames and choices survive.

GuiEngine now shares one private resetSourceFile operation between ordinary
reset and completed-load cancellation. It clears loaded-file metadata and the
current path, then invokes the existing QuickSave empty-source event. The numeric
document has already reset through cancelLoad and is not reset a second time.
The session, worker result and other cancellation boundaries remain unchanged.

The frozen helper 28 records seven cancellation cases across 14 fresh processes
and 40 Quick Save snapshots. A native model task hook cancels after load with or
without a prior source, first-image palette/segmentation, second-image smoothing/
fitting, and first-image export. The latter five already match. All raw responses
are reparsed; clocks/durations remain recorded and only those established fields
are normalized for repeat checks. Other state and actual SVG bytes match repeats.
The observer aborts on any native modal. No control or dialog was driven.

The unchanged native expectations first fail the two load cases, with captured
red results preserved. After the production fix, the same real BatchController
publisher calls its ordinary cancel command at the captured task boundary. Tests
compare source path, loaded-source availability and both widget snapshots at
every captured profile frame. Existing nine ordinary cases retain all assertions;
source-availability assertions now strengthen all sixteen native workflows.

Independent real-filesystem cases additionally cover skipping one target,
skipping all targets, cancelling overwrite preflight and a pre-aborted engine
job. These preserve source/window owners until an actual load and preserve the
existing output bytes. They do not establish unobserved native skip-window
behavior. No mock, alternate cancellation path or test-only production option
was added. Old raw recordings and fixtures remain unchanged.

All 73 focused tests pass: 61 GUI Quick Save tests and 12 existing core batch
cancellation tests. The complete isolated GUI suite has 485 tests: 484 pass, the
known batch settings case fails, and none skip. A new 491-file source snapshot
uses the identical four-file frontend bundle after all frontend sources and
build inputs match the prior verified copy. No frontend rebuild or browser
interaction was needed for this backend change. Native completed-load state
remains distinct from arbitrary cancellation timing and failed-load cancellation.

ce-simplify-code ran reuse, quality and efficiency rubrics inline with zero
independent reviewer contexts. No additional edits were warranted: reuse 0,
quality 0, efficiency 0. The existing final source-availability cleanup was kept
because all failure paths, not only the new cancellation branch, own that
contract. No unused production elements remain. No lint or typecheck script is
configured. Node syntax checks and the affected tests are the configured gates.

GUI sources, preference schema, processing kernels, original installation and
workspace gui/dist are unchanged. Overwrite-skip native completion, arbitrary
in-task cancellation, active-job shutdown, remaining export owners and full
parity remain unfinished. The existing internal-setter backgroundMode 2 mismatch
remains visible. Publication and Rust/GPUI remain pending.
