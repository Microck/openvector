# Document fitting interruption

ImageDocument now retains the canonical ContourFitter for its core lifetime.
Smoothing replaces contours while preserving existing region edges and cubic
storage. Region reconstruction replaces edge identities but retains cubic storage.
Source replacement creates a new owner. The document tracks actual segmentation
object identity to choose the native operation, rather than infer it from counts.

Public fitting state owns complete, geometry, cubicAvailability and outputChains.
Each observed checkpoint exposes current geometry and live contour kinds. Requested
cancellation returns 2, sets progress 100, clears its fitting key and preserves
the previous committed settings. Retry recomputes. An unrelated JS observer error
propagates by identity and invalidates its incomplete key. This last rule is an
independent API contract; native fatal-error recovery is not claimed.

Completed export/preview eligibility uses complete. The GUI keeps its separate
completed display scene and does not render incomplete current curves. Native
GUI save-button policy and partial output-chain lifetimes are not established by
these direct business-method recordings and remain part of full conformance work.

The original four-region cases now have sixteen-region, resmoothing,
region-reconstruction and cold-fitting companions. All 176 native fitting
cancellation/retry cases repeat exactly. Direct and scheduled independent document
calls compare 352 workflows, including first-chance snapshots, 100 parameter fields,
candidate caches, settings, rasters, stage keys, live contours, metadata and every
initialized cubic slot. Lower-level ownership remains in the existing fitting suite;
document cleanup belongs to the existing document-cancellation suite.

The first proposed grid joined diagonally into seven regions. The corrected input
uses four distinct colors at each junction and yields sixteen regions. Its original
diagnostic is retained. The first resmoothing test exposed incorrectly cleared
region edges; native recordings proved that these survive smoothing. The corrected
owner matches them. The host-observer error test also caught an incomplete cache
key; its original failure log remains. No expected native result was changed.

The reference auditor reparses 2,242 native responses, verifies all five generated
fixtures against their captured inputs/outputs, and checks all 289 previous fixtures
and the three candidate sources/executables. The original 43 accepted helpers and
original installation/settings remain unchanged. It creates no native process.

BatchRunner polls fitting checkpoints through its existing shared document flow.
The CLI worker acknowledges explicit progress observers and polls shared cancellation
without an observer. The GUI controller uses its existing active/completed event
distinction. Core/worker tests verify no fitted-completion event or output after
interruption, retained state, error identity and retry. GUI tests verify source
adoption, incomplete-preview rejection and retry through the actual controller.

ce-simplify-code ran reuse, quality and efficiency reviews inline, with zero
independent reviewers under the no-delegation instruction. Applied 1/0/2: share
the fixture writer, avoid whole-graph partial snapshots when no observer can read
them, and read the availability bitmap without cloning completed geometry. Native
state/progress comparisons and 51 scoped tests pass after these changes. No lint
or typecheck script is configured. No new unreachable product code was found.

consolidate-test-suites reuses the existing fitting, document-cancellation, batch
cancellation, worker and GUI batch suites. Their distinct invariants are numerical
state ownership, document cleanup, orchestration, acknowledged IPC and GUI adoption.
Native captures, their frozen fixtures, reference/package audits and installed
consumer checks are PROMOTE. Original diagnostic captures/logs are retained as
research evidence, without becoming numerical expectations. No standalone regression
suite, mock, compatibility bridge or temporary product probe was added.

The isolated GUI copy initially omitted shared test helpers. Those source files
were copied with hashes before rerunning the suite; the failed setup log remains.
The first browser job completed before observation and is diagnostic only. A larger
owned source showed Fitting curves: 12%; trusted Cancel stopped at 47% with no
output, retained source and no page errors. Trusted retry wrote one SVG. Output
creation time follows the cancellation capture. Existing previews/gui/dist were
untouched. The independent host and browser session are closed after verification.

Octen returned a VectorWitch workflow reference and a supplementary stock progress
graphic. Both were viewed; their decorative cards/striping were not adopted. The
existing black, dense progress layout remains. Only active/completed wording changes.
Sources: https://vectorwitch.com/ and
https://www.magnific.com/premium-vector/minimalist-progress-bar-ui-showing-task-completion-percentage-black-background_417112537.htm

All 123 package files match source, archive and fresh installed bytes. The installed
public API repeats 352 document workflows with actual original/workspace reads and
child creation denied. The installed CLI matches native completed geometry; a
separate installed worker cancels fitting and retries successfully. No worker
permission-inheritance claim is made.

Full core is 885/887 with the existing Adam7 and edited-alpha failures. Isolated GUI
is 660/661 with its existing processing-reset failure. Installed checks are 11/11.
No tests were skipped. Full GUI parity, segmentation/smoothing interruption,
completed browser downloads, publication and Rust/GPUI remain unfinished.
