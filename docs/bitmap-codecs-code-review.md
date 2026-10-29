# Bitmap codec implementation review

Scope: `src/bitmap-codecs.mjs`, pinned WebAssembly compressors and build sources, `test/bitmap-codecs.test.mjs`, its fixtures, and documentation. New helper 32 observes owned QImage JPEG output. Prior native executables and application UI code are unchanged.

## Behavior and evidence

The library exposes `encodeBmp`, `encodeJpeg`, `encodePng` and `encodeTiff` through `openvector/bitmap-codecs`. Each writer consumes tightly packed top-to-bottom BGRA bytes, validates that input boundary once, returns owned file bytes and preserves its input. The separate renderBitmap API assembles fitted geometry, cutouts and seam strokes to native premultiplied storage. The canonical exportBitmap operation selects that storage from document alpha/history and performs per-codec conversion.

Thirteen lossless native files cover BMP row padding, TIFF directory layout, transparent PNG, scale changes, and PNG outputs at 512 by 384 and 2048 by 1536. Nine additional native JPEG files cover single pixels, odd dimensions, gray RGB input, gradients, checkers and noise. Their two fresh processes repeat whole raw responses and file bytes exactly. The largest JPEG is 21,399 bytes and exercises repeated destination growth.

JPEG uses the default native RGB32 writer profile: IJG jpeg-6b defaults, quality 75, baseline quantization, default 4:2:0 sampling and 96-DPI JFIF density. The 60,493-byte WebAssembly module imports no host functions. Its 32 selected IJG files are unmodified, with complete README license and wrapper notice included. A separate MIT wrapper supplies RGB scanlines, a growing memory destination, per-call allocation and trap-based error reporting. There is no installed Qt/Vector Magic dependency, environment lookup, filesystem destination or codec fallback inside the module. Rebuilds reproduce SHA-256 f971e38852db7ceb2fadecdeeb468c39dda8dc9ae82f6992292d09b007b5a5fa.

The combined codec/raster suite passes 115 tests. JPEG compares every encoded byte and dimensions; its lossy pixels are not asserted equal to input. Existing lossless pixel assertions remain unchanged in scope. Two JPEG files also match complete source-to-fit-to-scene exports, alongside fifteen lossless files. Input preservation, ignored JPEG alpha, owned output across arena reuse and the IJG dimension limit are covered. No native expectation was changed, skipped or weakened.

The initial modern Node compressor passed the small PNG cases but failed both larger cases. Their filtered scanlines were identical. `evidence/bitmap-codecs-modern-zlib-counterexamples.json` retains the compressed-byte counterexamples. The final implementation uses one zlib 1.2.3 compressor with `Z_FILTERED`; there is no modern-compressor fallback. The first default-strategy experiment remains outside the shipped package under `artifacts/bitmap-zlib-compressor-investigation` as investigation material.

The 23,662-byte WebAssembly module imports no host functions. Its synchronous arena resets between calls; memory retains the largest prior allocation. JavaScript copies output bytes before that arena can be reused. Upstream source files and their license are unmodified and included in the package. The build uses official zlib sources, not Qt or Vector Magic code. LLVM rebuilds the same module hash.

## Simplification

The ce-simplify-code context script ran through the installed Git Bash command from PowerShell. All three rubrics were read and applied inline. There were zero independent reviewers, following the user's no-delegation instruction.

The common pixel boundary already avoided repeated validation. The Node CRC implementation avoids another CRC table. The local Paeth predictor remains small; extracting shared PNG math would add a module or couple image input to output without a clear benefit.

The earlier PNG review applied quality 1, efficiency 1, reuse 0: explicit TIFF directory sizing and reuse of PNG row/candidate buffers. This JPEG pass applied quality 0, efficiency 0, reuse 0. Existing validation is reused. Combining the separate codec arenas would widen the change into accepted build inputs without simplifying their independent memory ownership. No worthwhile behavior-preserving change or dead production code was identified. Upstream files remain unmodified and the linker keeps reachable WebAssembly code.

## Test ownership

The consolidate-test-suites skill placed the byte contract in the canonical `test/*.test.mjs` unit suite. Existing BMP/TIFF tests own import behavior, so a new encoder module warrants its own file in that suite. `test/bitmap-codecs.test.mjs` owns serialization, input ownership and explicit PNG RGB/RGBA storage choice. There are no browser or server tests for these same invariants.

Probe dispositions: PROMOTE the native codec fixture, the two larger reference captures and the modern-zlib counterexamples. MERGE the initial direct-proof tests into the compact fixture-driven codec suite. The archived compressor experiment is retained as unfinished investigation material outside product/package paths; it is not a runtime path or test dependency.

New JPEG observers and fixtures are PROMOTE reference evidence; the first two legacy JPEG comparisons are retained integration evidence. Their durable assertions are merged into the existing codec suite rather than a standalone test. The native JPEG decoder suite remains responsible for JPEG import, a separate invariant.

The installed-package check verifies exported module resolution, inclusion of all three WebAssembly files and upstream licenses/sources, and execution from a fresh offline dependency directory. It loads all 22 public imports and runs the actual packaged encoders and renderer against the complete current corpus.

## Verification and limits

- LLVM compressor build passed and reproduced module SHA-256 `16d064524bb68c63986e2faeed6cd78f24f170da7e6ed3d8137c24169873bd4f`.
- `node --test test/bitmap-codecs.test.mjs test/bitmap-raster.test.mjs`: 115 passed, 0 failed, 0 skipped.
- Python syntax compilation passed for the build and checkpoint scripts.
- The current local npm artifact includes the new compressor and its source/license files. It has not been published.
- The JPEG checkpoint reparses its native captures and scene references, verifies the package and compressor sources, checks the preceding 31 helpers plus new helper 32, and confirms installed application files/settings, 70 GUI files and the root preview index unchanged.

The root package defines no lint or typecheck scripts. The focused suite and installed artifact checks match this isolated addition; the full historical suites were not rerun. Their previously recorded processing-reset boundary mismatch and two core failures remain unresolved.

GUI bitmap commands, broader scene/flatten conformance, other-host runtime execution, publication and the Rust/GPUI port remain incomplete. These file cases do not prove full application parity.
