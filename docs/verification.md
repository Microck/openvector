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

## v0.1.0 candidate checks on 2026-11-06

The full cleanup runs completed on an isolated Linux build host with the retained
reference corpus. These are the initial full-run counts, not an all-green report:

| Suite | Passed | Failed | Skipped or ignored |
| --- | ---: | ---: | ---: |
| Rust engine and CLI | 645 | 8 | 0 |
| JavaScript | 958 | 20 | 1 |
| JavaScript GUI-model tests | 566 | 106 | 0 |
| Rust GUI library | 83 | 0 | 1 |

Focused follow-ups corrected missed public producer metadata in the expected PDF
batch and EMF default-export bytes. The filesystem and CLI PDF batch checks both
passed, as did the Rust default-export check. The Rust batch target compiled but
still stopped at its pre-existing SVG comparison. Actual output bytes were not
normalized, and no failing assertions were skipped or weakened.

The seven remaining observed Rust failures also occur on `main`:

- `all_16_native_gesture_workflows_and_completed_refits`
- `image::native_pixels`
- `image::native_preparation`
- `native_batch_runner_files_logs_and_task_boundaries`
- `native_command_alpha_premultiplied`
- `native_segmentation_edit_recipes`
- `single_large_alpha_rings`

The other 19 observed JavaScript failures also reproduce on `main`. All 106
GUI-model failures occur in the baseline run, which had 151 failures. Baseline
agreement does not make those behaviors correct. The full suites were not rerun
after the focused expectation fixes.

The source smoke suite passed all 33 checks. Native candidate
[run 42144952112](https://github.com/Microck/openvector/actions/runs/42144952112)
built and packaged all five architecture targets. Extracted Linux amd64, Linux
arm64, macOS Apple Silicon and macOS Intel applications converted an authored
bitmap and saved the exact SVG produced by the CLI.

Both Mac disk images have Developer ID signatures and accepted notarizations.
The stapled images were mounted read-only, and the copied applications passed
signature, ticket and Gatekeeper checks. Native Apple Silicon and Intel through
Rosetta on macOS 26.3 both produced the exact expected SVG. The Intel candidate
also passed its native macOS 15 CI check.

The updated Windows installer passed on Windows 10 build 19045 with a path
containing spaces. A real file lock made silent uninstall return failure while
preserving every installed file and its registration. Portable and installed
conversion checks passed; normal uninstall removed the application, registration
and shortcuts while preserving an unrelated file.

Windows 11 on ARM64 passed the same installer lifecycle checks through x64
emulation in that run, including the real file lock and normal uninstall
immediately after GUI conversion. Codex reported no major issues on `4880ae7e`.

The corresponding-source archive at `8407a98c` was also rebuilt offline with Rust
1.99.1. In a disposable copy, a diagnostic marker was added to the Qt-derived
`square_stroke` function. The rebuilt CLI executed that modified function four
times and produced the same PNG bytes as the unmodified build. The rebuilt GUI
contained the modified component and passed its real-window SVG conversion check.
Neither modified executable is a release artifact.

## Intentional difference

Failed Open preserves the previous image, document state, settings, and export
eligibility. This was explicitly chosen instead of reproducing the original's
measured failed-open behavior.

## Limits

- The reference suites have the failures listed above. Exact parity and an
  all-green full-suite result are not established.
- Linux packages were tested on Ubuntu 24.04 and need XCB, FreeType and xkbcommon.
  ELF version requirements reach `GLIBC_2.39` on arm64. On amd64, the highest
  non-weak GLIBC requirement is `GLIBC_2.35`; `GLIBC_2.39` is weak. Older
  distributions have not been tested. CLI/addon dependencies differ from the GUI.
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
The repository was made public after a separate source and provenance review.
That review and these checks are not legal clearance for third-party material.

[GitHub Actions source CI](ci.md) runs the small-reference and independently authored checks named in
`test:smoke`, JavaScript syntax checks, and package inventory checks. The [release runbook](release-runbook.md) requires native
build and real workflow evidence before attaching binaries to a release.
