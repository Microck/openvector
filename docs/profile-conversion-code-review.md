# Saved-profile conversion review

Scope: `runAdvancedProfile`, its public profile export, GUI Fast Forward
adoption, conversion CLI flags and focused tests. Numerical kernels, reference
expectations, existing user previews, original installation and original saved
settings were outside the mutation scope.

The `ce-simplify-code` context script and all three shipped rubrics ran inline
under the no-delegation constraint. Three passes do not constitute independent
review: independent contexts 0. Applied findings: reuse 1, quality 0, efficiency
1; skipped 0. The helper trusts the integer guarantee already established by
`readSettingsProfile` while keeping its required color-count bounds. The CLI
also avoids cloning/forcing advanced mode twice before the shared runner.

One replay policy now serves the GUI and CLI. The GUI adopts owned stage
notifications as they complete, preserving palette/segmentation state when later
work fails. Batch CLI selection uses the existing advanced-profile projection
and worker runner; it preserves basic and transparency settings instead of
maintaining a second conversion implementation. Profile lookup has an explicit
storage path and persists MRU selection at the declared conversion boundary.
No compatibility alias, implicit GUI storage lookup, or auto-classification
fallback was added. No new unreachable elements were identified; the private
palette helper and fitting methods still serve ordinary wizard operations.

The ownership test initially found that an observer could alter the returned
final task snapshot. The document itself remained correct. The runner now
returns a final document snapshot separate from every observer snapshot. The
original failure remains in `evidence/profile-conversion-document-tests.txt`;
the repaired ownership/native-state checks pass without weakened assertions.

Validation: frozen GUI 269/269, installed direct tests 236/236, separate HTTP
workers 6/6, installed profile CLI 19/19 and full source CLI suite 176/176. Four
browser checks verify GUI replay and preview; the backend SVG matches the
retained native two-color result. Source/packed/fresh-installed files match.
All 18 public modules and the five profile exports are verified under actual
main-process read/process denials. Worker permission inheritance is not claimed.
There is no configured lint or typecheck command in this JavaScript/JSX project.

New native evidence covers four cold batch cases, eight serial processes, four
complete documents and 400 typed parameter records, with both raw responses and
export bytes retained. Nonuniform patchwork files exercise retained random state.
Four single-image CLI cases reuse recorded Fast Forward geometry as assertions;
processing inputs come from declared actions and source images. Complete native
cold/cache-hit document comparisons also exercise the public replay helper.

The prior complete root run still contains two unresolved native-memory-dependent
mismatches. No numerical source or failing assertion changed here. Broader guided
replay, native dialogs and editing/background reset behavior remain unfinished;
this review does not establish full application parity.
