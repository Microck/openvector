# Bitmap renderer review

The bitmap-raster API provides two boundaries: rasterizePaths renders fills
and single line/cubic cosmetic strokes to premultiplied BGRA; renderBitmap
assembles completed fitted geometry and returns native premultiplied BGRA.
The canonical bitmap-export operation owns document storage policy and codec conversion. Neither requires installed Vector Magic or Qt code.

## Ground truth and corrections

Frozen helper 30 owns offscreen fills. Helper 31 adds owned QPen objects and
observes one-device-pixel square-capped, bevel-joined strokes. Neither drives
application widgets or dialogs. The preceding 30 executable hashes remain
unchanged. The corpus contains 26 fill cases and 41 stroke cases, each repeated
in two fresh processes with identical whole raw output. An archived allocator
diagnostic adds two more processes. The existing 3,200-byte raster control
quota and preservation of already painted spans across pool retries remain.

The stroke translation initially lost cap direction when a final control
repeated the endpoint. Native pixels and the upstream emitCubicTo logic
establish the previous distinct control as the tangent origin. The accepted
expected pixels were not changed.

Further cases expose different channel arithmetic for RGB32 and premultiplied
ARGB32. The former uses the scalar packed-channel correction; the latter uses
the MMX/SSE correction, which applies bias before the high-byte adjustment.
Source-color premultiplication remains scalar in both formats. The initial
opaque color-byte and translucent alpha-byte failures remain in evidence.

REA identifies scene construction at 0x430410 and rendering at 0x42df00.
Seam colors truncate adjacent-channel averages. Cubic IDs select slots and
retain the last region write; lines append in region traversal order. Opaque
output paints seams whose alpha is at least 250, then fills with holes.
Transparent output omits the prepass. File conversion truncates channel
unpremultiplication and clears RGB at alpha zero.

Eleven complete source-to-file cases match seventeen native files,
including two JPEG files, transparent PNG, BMP/TIFF padding, fractional scales, positive
half-integer dimensions and PNG scales 32 and 128. The old opaque diagnostic's
15 differing pixels are resolved by the seam prepass.

## Source and license boundary

The FreeType scan converter and its license remain under
src/vendor/freetype-raster. Its WebAssembly bytes are unchanged. The separately
imported, editable src/vendor/qt-bezier/square-stroke.mjs derives from LGPL-2.1
Qt 4.5.3. The complete source, license, upstream exception and modification
notice are packaged. The fetched 4.5.3 curve-offset block is byte-identical to
the inspected 4.4.3 block. Source URLs and hashes remain in evidence. The
translation specializes one line/cubic seam edge. General polyline strokes
and other pen styles are not advertised. Bounded subdivision and threshold
relaxation remain intact.

## Review and verification

The ce-simplify-code context and all three rubrics were applied inline, with
zero independent reviewers. The Bash wrapper failed because the host routed
it to unavailable WSL; the Node context script then ran directly. The user's
no-delegation instruction controls this review.

Reuse review found no replacement for the seam traversal, whose ordering and
multiplicity differ from fills. It already reuses fitted-graph validation,
hole traversal and cubic orientation. Quality and efficiency review found no
worthwhile behavior-preserving cleanup. Applied findings: reuse 0, quality 0,
efficiency 0. Correctness fixes above are not counted as simplifications.

The consolidated raster suite owns exact primitive pixels and input ownership.
The codec suite owns encoding supplied pixels and now also the distinct
complete decode/fit/scene/file integration boundary. Native observers, source
identities and failure logs are promoted reference evidence. The earlier
assembly diagnostic remains historical evidence outside the package. No
temporary alternate implementation or redundant standalone test remains.

The focused command node --test test/bitmap-raster.test.mjs
test/bitmap-codecs.test.mjs passes 115 core tests, with zero failures/skips. JavaScript
and Python syntax checks pass. No lint or typecheck script is configured. Full
historical suites were not rerun; their recorded failures remain unresolved.

The current offline local npm install verifies the canonical bitmap export,
all public imports and the current native corpus. All three
modules import no host functions. The current JPEG checkpoint checks all accepted
helpers, original files/settings, 70 GUI files and the root preview index.
Nothing was published.

## Remaining scope

Broader color/geometry conformance, hidden-region scenes, flatten choices,
extreme-coordinate clipping, scale/widget limits, broader JPEG coverage, bitmap
GUI commands, GitHub publication and the Rust/GPUI port remain incomplete.
These cases establish the stated renderer behavior, not full parity.
