# Verification and known limits

The supported comparison goal was completed on 2026-10-29. This means the selected
native comparisons, necessary fixes, actual changed-path checks, and test resource
cleanup were completed. It does not establish full exact parity or absence of bugs.

## Historical results from 2026-10-29

- The retained 30-action recent-settings and manual-palette history matched the
  corrected Rust GUI and numerical replay, including the exported AI bytes.
- The supported photographic High/Low and additional-page/nondefault Reset
  comparisons completed at their recorded scope.
- The macOS public palette Add/Delete boundary was exercised with painted evidence.
- A real Windows held-button capture transfer showed that the original scrollbar
  keeps repeating until it reaches the pointer. The corrected Rust GUI passed the
  same behavior check. Five retained screenshot strips independently confirm thumb
  movement and stopping. Six focused scrollbar tests passed with no failures.
- Current Linux and Windows GUI packages, CLI and Node addon were retained with
  build and runtime verification. All task-owned processes, tasks, locks, build
  workspaces and temporary runtime stages were settled. Historical evidence remains
  in the local investigation archive.

## Intentional difference

Failed Open preserves the previous image, document state, settings, and export
eligibility. This was explicitly chosen instead of reproducing the original's
measured failed-open behavior.

## Limits

- Three strict Rust comparisons involving excluded native undefined behavior remain
  failing. They were not skipped or changed to produce a passing report.
- The complete JavaScript run previously reached its deadline. Scoped follow-up
  results are retained; there is no all-green full-suite claim.
- A fresh macOS package rebuild was blocked by available build disk capacity. The
  earlier verified macOS palette result remains valid for its recorded binary.
- The current Linux GUI release requires `GLIBC_2.39`, XCB, FreeType and xkbcommon.
  Linux CLI/addon and GUI artifacts have different dependency requirements.
- Wayland, wider input combinations, exact timer delivery, and early release after
  capture transfer are not established by the last Windows check.
- Existing component gaps, including decoder comparisons, are recorded in the
  [parity matrix](parity-matrix.md). A build succeeding does not close those gaps.

## Evidence and reproducibility

`docs/*-contract.json` and the parity matrix retain detailed comparison boundaries.
Their `evidence/` paths identify the local investigation archive, which is excluded
from Git because it contains host-specific diagnostics and original-application
materials. Those links are provenance references, not downloadable repository files.
The test fixture corpus contains the reference inputs and expected observations
needed by the differential suites. See [test fixtures](test-fixtures.md).

The cleanup preserves the fixture corpus, regression tests, behavioral contracts
and source provenance. Four investigation-only documents are excluded from the
source tree: `reference-probes.md`, `reverse-engineering-report.md`,
`native-gui-source-context.md` and `engine-reference-contract.json`. Their
private originals remain available in the investigation archive.
This change does not clear the retained source, fixtures, Git history or remote
pull-request refs for public distribution. Publication needs a separate review.

[Raft source CI](ci.md) runs the small-reference and independently authored checks named in
`test:smoke`, JavaScript syntax checks, and package inventory checks. The [release runbook](release-runbook.md) requires native
build and real workflow evidence before attaching binaries to a release.
