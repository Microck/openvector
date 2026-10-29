# Bitmap export reference review

Scope: the new `bitmap-export` build kind, its conditional model-operation branch in `tools/probe-profiles-native.c`, and `evidence/record-native-bitmap-export.py`. Product sources are unchanged.

The ce-simplify-code reuse, quality and efficiency rubrics were read and applied inline. There were zero independent reviewers. The shell is PowerShell; the installed Node context script ran directly because Bash is unavailable.

One reuse improvement replaced a second SHA-256 implementation with the existing capture digest function. Quality and efficiency review found no further behavior-preserving edits worth applying. Applied findings: reuse 1, quality 0, efficiency 0. No newly unused elements remain in this scope.

The helper uses a new executable rather than rebuilding an accepted oracle. Its bitmap operation calls the original MainModel method with a caller-owned QStringList, waits for the original task manager, then invokes the original list destructor. Its scale command invokes the existing model slot. The caller does not drive native controls or answer dialogs. The inherited replay observer terminates on a native modal before response code.

The first capture exposed missing imageformat plugin discovery: PNG and BMP succeeded while JPEG and TIFF files were empty. That failed raw response and its files remain under `evidence/native-bitmap-export-opaque-four-*` and `evidence/bitmap-export-native-output/opaque-four`. The corrected environment sets `QT_PLUGIN_PATH` to the original installation root. All four formats then repeated byte-for-byte. No fallback or plugin copy was introduced.

The capture script originally expected every requested target to exist. Direct model calls with scale zero produced no file. This is retained as a diagnostic outside the GUI controls' admission boundary. The positive-scale captures still require every requested output, and the audit separately checks the observed absence of files for both nonpositive cases. The interrupted zero capture remains retained; repeated diagnostics use their own paths.

Verification: all eight cases were replayed in two fresh processes, producing eighteen bitmap files and eighty-eight metadata frames. The audit reparses every raw response, checks exact non-timing state and output bytes, decodes retained files again, validates input/request/probe hashes, and checks all twenty-eight prior accepted helper hashes. It also verifies the forty-three installation files, original settings fingerprint, input desktop and removal of the private reference namespace.

The checkpoint verifies the prior 491-file product snapshot, the 65-file core package/archive and four prior fixtures unchanged. A separate 3 by 2 codec case adds two processes and eight image files with identical repeats; its raw responses and files were reparsed and audited separately. That case exercises BMP row padding. The selected official Qt source files remain reference artifacts only. The checkpoint code received the same inline reuse, quality and efficiency review, with no further edits indicated.

This scope has no configured lint or typecheck command. The helper compiled with warnings treated as errors. Python syntax compilation and the fresh-process capture audit are the relevant checks. No frontend bundle or native app window was opened or changed.

Limits: no independent bitmap renderer, encoder, CLI command or GUI action was added. The dimensions do not yet test half-integer rounding. GUI size bounds, flatten choices, file-failure presentation and broader raster geometry remain unresolved. JPEG RGBA values come from Pillow decoding, not a native internal QImage observation. The existing processing-reset backgroundMode comparison and the two existing core failures remain outside this change.
