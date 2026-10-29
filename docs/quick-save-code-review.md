# Quick Save and direct folder export: initial checkpoint

The later source-protection fix and current verification are documented in
quick-save-batch-code-review.md and quick-save-batch-contract.json.

OpenVector now offers direct folder saving and a separate browser download.
The active basic/advanced owner supplies format, directory, disclosure and
transient filename. Only the first three fields enter the existing six-field
preferences file. No compatibility reader or second naming owner was added.

The new read-only native observer records exactly one QLineEdit filename per
Quick Save widget. Eleven cases, 22 processes and 31 reference snapshots confirm
first-dot basenames, hidden filenames, Unicode/space/case handling, independent
disclosures, directory canonicalization and filename retention through model
reset. The earlier 21 preference cases remain enforced. Whole raw responses
were reparsed, repeats matched, and no native inputs, dialogs or widget setters
were driven. The new observer is helper 28; the earlier 27 executables remain
immutable. REA analysis was saved separately before normal session shutdown.

The static save handler at 0x4212d0 supplies empty-name, directory creation,
source protection and overwrite branches. The filename editingFinished callback
at 0x420e40 invokes normalization at 0x41d7f0. The vector widget signal connects
to tryQuickSave at 0x44f790. The receiver itself and native write/error behavior
remain unexecuted and unverified. Saved is shown only after a successful
independent file write; exact native failure-notification parity is not claimed.

QuickSaveFile owns one pending target and rendered byte buffer. The worker
rejects document/settings/queue mutations until the user resolves a directory
or overwrite prompt. Cancel never writes the target. A successful write or OS
error releases the pending owner. The existing six writers produce the bytes;
EMF retains host-measured memory-only GDI device metrics. No kernel, installed
Vector Magic file or original setting changed.

All 39 Quick Save tests pass, including 32 unchanged native projections, atomic
validation, strict storage/restart, six-format HTTP downloads, six-format actual
folder writes, directory/overwrite cancellation, source protection, real OS
failure/recovery and pending-operation guards. The isolated production build
passes. Its complete GUI run has 463 tests: 462 pass, the known setter-only batch
backgroundMode 2 case fails, and none skip. The root run additionally fails the
branding assertion against deliberately preserved historical gui/dist. The fresh
isolated build resolves that assertion. The core was not changed or rerun; its
two earlier failures remain open. No native assertion was weakened.

Browser testing writes actual SVG and PDF files inside an owned runtime folder.
It verifies first-dot normalization, extension selection on blur, Enter-to-save,
empty-name errors, Cancel focus and no-write cancellation, explicit mkdir and
overwrite acceptance, saved notification disappearance, separate mode choices
and clean host restart without preference reseeding. The notification was
observed hidden 8.7 seconds after appearing; the source uses a 5000 ms timer.
This is not a tight timing benchmark. Both browser logs are empty. Both owned
hosts close normally and the owned tab is closed. No user tab, existing preview,
workspace gui/dist or viewport override changed.

The existing ImagiTool compact-editor reference was reused. Controls use the
current shadcn buttons/dialogs and Lucide icons, black surfaces and white text.
No decorative chrome or looping motion was introduced.

ce-simplify-code ran the shipped reuse, quality and efficiency rubrics inline,
with zero independent reviewer contexts. Applied: reuse 1 (shared input styling),
quality 1 (straight confirmation branches), efficiency 0. Skipped 2: existence
checks are needed for explicit confirmation, and deferring byte rendering would
change error timing and pending ownership. No unused production elements were
found. No lint or typecheck script is configured. Node syntax checks, scoped
tests, the isolated build and full GUI tests provide the current verification.

Typed completion/path edge cases, batch cancellation/skip source events, the native save
receiver, picker interaction, bitmap export, drag-result, full window reset and
active-job shutdown remain unfinished. GitHub/npm publication and Rust/GPUI
remain pending. This checkpoint does not establish full application parity.
