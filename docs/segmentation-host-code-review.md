# Segmentation interruption through application hosts

The batch document operation now forwards all three vector stages through its
existing progress observer. Segmentation uses the same owner and producer
callbacks verified by the preceding 688-case document checkpoint. There is no
second segmentation or cancellation implementation in the hosts.

An abort unwinds the document before a completed segmentation event, smoothing,
fitting or output write. The existing CLI worker acknowledgement and GUI shared
flag polling apply unchanged. Repeated integer percentages still check the
cancel flag before deduplication, so active 100-percent work remains incomplete.

The first incorrect display write was GuiEngine adopting an unfinished map
because exported region counts existed. Counts can be populated before the last
segmentation callback returns. The completed stage key now authorizes that
display write. No partial-label normalization, fabricated palette, geometry
repair or replacement renderer was introduced. The source remains visible.

README and CLI help now describe numerical-stage cancellation. An initially
unsupported CLI comparison command is retained in its failure log. The final
command uses the documented --settings option with the exact browser-job
controls. The pre-help archive/preflight remain preserved; the final archive
includes the corrected help text. That final delta contains no numerical change.

## Review and coverage ownership

Applied ce-simplify-code's complete reuse, quality and efficiency rubrics inline;
zero independent reviewers ran. The production change has one canonical path,
no new state or compatibility bridge, and no additional worthwhile finding.
Reuse 0, quality 0, efficiency 0, skipped 0. No production source became
unreachable and no deletion is pending. No root lint/typecheck script exists.

Consolidate-test-suites disposition: MERGE the additional cases into the existing
batch-cancellation, batch-worker and GUI batch suites. Their distinct boundaries
are document-to-runner abort state, worker acknowledgements, GUI shared-flag
polling and adoption of completed display maps. Canonical document tests already
own native interruption fractions and complete partial-state comparisons.
PROMOTE the repeatable package/source/snapshot auditor and trusted browser
evidence. No standalone regression suite, new native fixture or helper was added.

## Verified scope

Current and isolated batch/worker runs pass 34 tests each. The fresh isolated
GUI build passes 663 of 664 tests; the existing processing-batch-controls
failure remains visible. The final installed API passes 25 tests with actual
denials of original/workspace reads and child creation. Its separate batch/worker
suite passes 34 tests; worker inheritance of those permission denials is not
claimed. All 125 package files match source, archive and installation.

Trusted browser Process displayed Creating regions at 16 percent. Trusted Cancel
ended segmentation at 56 percent, retained the 512x512 source, displayed no
incomplete map and wrote no output. Trusted retry wrote one SVG with the same
SHA-256 as installed CLI output. Browser errors are empty. The owned browser and
host closed; a real connection attempt confirmed that the host port was closed.

The preceding full numerical foundation remains 902 of 904 tests with its two
known failures. This host phase does not claim a second full core run. Source
hashes prove unchanged numerical kernels. The original installation/settings,
43 accepted helpers, three numeric candidates, sealed reference evidence, root
gui/dist and user previews remain untouched. No VCS metadata, commit, push or
publication was created. JJ remains unavailable.

Resized retained storage, palette-analysis handoffs, editing/shadow interruption
contexts, old graph validity, original GUI/scheduler timing and the three known
failures remain open. Full parity, publication and Rust/GPUI porting are pending.
