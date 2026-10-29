# Quick Save source protection after batch: initial checkpoint

The completed-load cancellation fix and latest verification are documented in
quick-save-cancellation-code-review.md and quick-save-cancellation-contract.json.

The native model changes its current source filename during batch loads,
including failed attempts. Its Quick Save widgets retain their previous text,
formats and folders. Source protection must compare against the model path.
Keeping that path in QuickSave incorrectly treated a former source as current.

GuiEngine now owns the resolved current source filename. Successful single-image
commit, reset and every completed batch-load attempt update it. QuickSave owns
only its window fields. quickSaveTarget combines the selected target with the
current engine path. The stale private path was moved out of QuickSave; no second
path owner or compatibility behavior remains. The writer and its confirmations
are unchanged. Existing UI and preference formats remain unchanged.

Nine cases use the frozen helper 28 across 18 fresh processes and 49 Quick Save
snapshots. All raw responses are reparsed. Raw clocks and durations are retained;
only those established timing fields are normalized for fresh-run comparison.
Other state and native SVG bytes match across repeats. Cases include queue-only,
cold success, two successes, a hidden last filename, success then missing/corrupt,
missing then success, and cold/prior-source all-missing batches. Native dialogs
abort observation without being answered. No native controls or widget setters
were driven. The 28 accepted helpers and original installation remain guarded.

The independent tests compare current source paths and both widget snapshots to
those native records at every captured profile frame. Owned temporary paths are
translated in fixture setup only. Additional real-disk and HTTP cases prove that
the former source requests overwrite consent while the current source remains
protected. Invalid single-image decoding preserves the current path; reset clears
it while retaining the visible filename. No mock or native fixture was changed.

All 50 focused Quick Save tests pass. The full isolated GUI suite has 474 tests:
473 pass, the same known batch case fails, and none skip. The four frontend
bundle files are reused from the earlier verified build after verifying all
frontend sources and build inputs are unchanged. The server and worker run from
a new 488-file source snapshot. No frontend rebuild or browser interaction was
needed for this backend-only change. The old processing-reset boundary was rechecked: 8 of 9 tests pass;
the existing setter-only backgroundMode 2 mismatch remains unchanged. The native
menu still exposes 0, 1 and 3. No processing admission was broadened to hide it.

ce-simplify-code ran the reuse, quality and efficiency rubrics inline with zero
independent reviewer contexts. No further behavior-preserving edits were needed:
reuse 0, quality 0, efficiency 0. A shared test-setup extraction was skipped because
it would add indirection to small, fixture-specific setup. No unused production
elements remain after moving the stale private path. No lint or typecheck script
is configured. The existing UI, original installation, workspace gui/dist and
core package are outside the code change.

Native overwrite-skip source events, arbitrary in-task cancellation, save failure presentation,
remaining export/widget owners, full window reset and active-job shutdown remain
unfinished. Full parity, publication and Rust/GPUI remain pending.
