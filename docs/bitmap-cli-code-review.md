# Bitmap CLI and storage review

The CLI now exports fitted vectors as BMP, JPEG, PNG or TIFF through vectorize,
output-extension inference and the existing stdout/file paths. The new
openvector/bitmap API exports the same document through exportBitmap. CLI and
future GUI callers share this operation rather than selecting codec storage
independently.

## Native storage evidence

Native 0x469ed0 classifies actual original-source opacity. It does not use the
container's alpha capability. Native 0x42df00 also calls 0x4694c0 on the
visibility owner, whose active history count controls alpha storage. Hidden
bits alone are insufficient: toggling a region twice restores visibility but
retains history, changing image storage and antialiased seam pixels. Undoing
the last edit restores the original storage path.

New helper 33 calls MainModel's world-point background business action at
0x425710 and the existing undo/redo/revert slots. Five cases ran twice in
fresh processes and produced seventeen native files. Neither pointer/key
input nor native widget setters or dialog answers were used. Earlier 32
executables remain frozen. Original files, settings and the input desktop
remain unchanged.

The captures establish another format distinction: PNG/TIFF unpremultiply
ARGB32 pixels, while BMP/JPEG serialize stored color channels. The first
implementation incorrectly included BMP in the conversion. The retained
failure log and pixel diagnostic demonstrate that mismatch. Accepted native
expectations were unchanged.

Both renderBitmap and rasterizePaths now return premultiplied storage.
exportBitmap alone selects document storage and performs format conversion.
This removes the lossy straight-to-premultiplied reconstruction that a caller
would otherwise need. All active product/test callers and documentation use
the canonical contract. No old-storage compatibility branch remains.

## CLI and API contract

The CLI accepts bmp, jpg/jpeg, png and tif/tiff. Bitmap scale is positive and
finite; raster admission still enforces its supported coordinate/dimension
range. Existing source/profile protection and exclusive output creation are
retained. Conversion and consent checks finish before writing output.

The CLI never prompts. --flatten acknowledges the native non-PNG warning for
actual source transparency with applied background mode zero. The SDK exposes
BITMAP_FLATTEN_REQUIRED and accepts flatten:true. This is a warning decision,
not a second source-image processing pipeline. PNG does not accept this flag.
Native modal acknowledgement and broader flatten configurations remain
unverified; no reference dialog was bypassed or answered.

## Review and test ownership

The create-cli guidelines, consolidate-test-suites and ce-simplify-code skills
were applied. All three simplification rubrics ran inline, with zero
independent reviewers, following the user's no-delegation instruction.
One quality simplification returns the raster result directly after moving
conversion to the canonical exporter. Applied counts: reuse 0, quality 1,
efficiency 0. Native storage corrections are correctness changes, not counted
as simplification. No now-unused production elements were identified.

The existing codec suite owns document storage and native file bytes. The
existing vectorize suite owns argument parsing, stdout/stdin, edits, format
aliases, consent, exit codes and file protection. The initial alias test
looked for JPEG in the lossless fixture; its lookup now selects the native
record containing that file. No product assertion or expected byte was
weakened. The new TIFF opacity assertion tests pixel alpha rather than its
RGBA container capability, consistent with the native opaque TIFF evidence.

The five scoped suites pass 261 tests, zero failures/skips: 115 core and 146
CLI tests. They include thirty-four complete native bitmap files. Full
historical suites were not rerun; their recorded failures remain unresolved.
The root package has no lint or typecheck script. JavaScript/Python syntax
checks pass.

The offline local package contains 123 files and 22 public imports. Installed
SDK and CLI outputs match all thirty-four files, alongside twenty-two codec
cases and sixty-seven primitive pixel cases. The installed CLI runs with
Node permissions restricted to its install directory; probes confirm denial
of Vector Magic and workspace-source reads. This verifies the scoped CLI,
not GUI/worker isolation. All three WebAssembly modules have zero host imports.

## Remaining scope

GUI bitmap controls/save decisions, broader flatten/visibility/scale cases,
native source-file equivalence, full application conformance, publication and
the Rust/GPUI port remain unfinished. Nothing was published. Existing GUI
source, the root preview build and prior native executables are unchanged.
