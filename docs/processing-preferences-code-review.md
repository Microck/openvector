# Processing preferences

The worker restores one processing-preference file before constructing its image
and batch owners. Clean idle host shutdown saves those owners before terminating
the worker. The file contains selected/batch controls, their distinct counters,
background recommendation and batch export choices. Startup has no source image,
queued files or active batch workspace.

Palette colors and runtime backup are session state. They do not enter this
file. Native startup resets the selected palette count to zero while preserving
the batch palette count. Native QColor persistence rounds each channel to 16
bits, stores its high byte and restores opaque RGB. The extracted
storedSettingsProfile function supplies this existing saved-profile conversion
to both stores without changing the saved-profile JSON shape.

The reader validates the storage boundary once and reports a corrupt file with
its location and recovery options. It does not replace invalid content or
translate an old format. The writer uses an owned temporary file and atomic
rename. A shutdown save error stays visible; the host still releases its worker.
Saving while a batch is running fails explicitly. Exact native quit behavior,
including active processing and write-error presentation, remains unverified.
Native captures call the original model writer directly, so they establish
save/restore transformations rather than complete desktop quit conformance.

## Reference and test isolation

Eight paired workflows retain 34 fresh native processes, nine restart phases and
eight full post-restart load documents. They cover cold defaults, changed
selected/batch controls, custom alpha colors, supplied palettes, export paths,
model reset and repeated saves. All 800 document parameter records match.

The initial cold-document attempt exits 15 because the frozen document reader
requires an observed completed worker result. It remains rejected evidence.
Accepted cases record cold startup selection first, then load a real source
before capturing the full document. No observer guard or native instruction was
changed to admit the rejected request.

The first broad GUI run revealed existing HTTP fixtures that used the default
profile directory. New shutdown persistence created a processing-preference file
there and allowed settings to cross test boundaries. That run is retained with
363 passes and three failures. The newly created file was checked by path,
creation/write time, size and hash, then moved intact to the evidence directory.
Its cleanup record and preserved bytes remain available. Existing unrelated
profile/source files were not restored or removed.

Eleven cold-host call sites across nine test files now use a real-host fixture
with its own temporary store directory. The fixture closes the actual host and
then removes only that owned directory. Restart/persistence tests continue to
pass explicit store paths. A parser comparison confirms that all existing test
bodies, assertions and comments remain unchanged apart from the fixture import
and call names. Ast-grep is unavailable; the installed Babel parser provided
the syntax-aware edit and comparison. No native expectation was changed.

## Review and verification

The ce-simplify-code reuse, quality and efficiency rubrics ran inline with zero
independent contexts under the standing no-delegation rule. The Node setup script
ran directly because Bash is unavailable. This is not independent corroboration.
The review retained boundary checks and owned-resource cleanup. It identified
no newly unreachable production code or unused element.

The implementation already shares its saved-profile serializer. The efficiency
pass removed a redundant default batch-profile allocation during restoration
and repeated backup decoding inside a test comparison. Applied simplification
counts: reuse 0, quality 0, efficiency 2; no other behavior-preserving edit was
needed. Error diagnostics and test-store isolation are correctness changes,
not claims of behavior-preserving simplification.

Thirteen new tests pass, as do 38 profile/document/CLI tests. The final isolated
build succeeds; the GUI suite passes 365 of 366 tests with no skips. The remaining
failure is the existing setter-only backgroundMode 2 comparison. There are no
configured lint or typecheck scripts.

All 64 npm archive files match source and offline installation bytes. Only
settings-profile.mjs and saved-profiles.mjs differ from the preceding package.
The installed verifier imports all 19 public entries and matches a native saved
profile while reads of the original application and workspace are denied. This
permission evidence covers its main process, not worker inheritance.

Original Vector Magic files/settings, accepted probes, existing user previews
and workspace gui/dist remain unchanged. No browser layout change was made.
Complete window reset, export-option restart evidence, exact native quit flow,
earlier parity failures, publication and the Rust/GPUI port remain unfinished.
