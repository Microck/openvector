# GUI bitmap download review

The GUI engine and existing download endpoint now call the canonical
openvector/bitmap exporter. The worker preserves the exporter's error code,
so a client can distinguish a required flatten decision from a failed
conversion. Rejected downloads return JSON and current lightweight state,
without an attachment header or file bytes.

The endpoint accepts numeric scale and boolean flatten values. Vector-only
options are rejected for bitmap output, and bitmap-only options are rejected
for vector output. Per-download choices do not change processing preferences,
the source image, fitted geometry or background history. Existing loopback,
origin, batch and operation serialization checks still apply.

Native filename captures establish image.bmp -> image_vectorized,
tall.bmp -> tall_vectorized and multi.part.bmp -> multi on both bitmap
widgets. The bitmap download branch applies that measured filename rule;
vector names retain their existing behavior. Broader filename cases still
need reference coverage.

## Review and coverage

The consolidate-test-suites skill places transport coverage in the existing
gui/test/export-preferences.test.mjs integration suite. It exercises the real
host and worker with source bytes, automatic processing and declared
background actions. Native fitted geometry is never used as computation
input. Eighteen recorded workflows compare thirty-four whole native files.
The core suites separately own raster pixels, storage and codec behavior.
Filename cases reuse the immutable control capture fixture. No temporary
test probes, duplicate test files or mocks were added.

All three ce-simplify-code rubrics ran inline, with zero independent reviewers.
The scope was the three changed GUI runtime modules and their export test
changes. Reuse, quality and efficiency each had zero applied findings and
zero skipped findings. The shared exporter already owns the substantial
operation; no new state owner, wrapper or compatibility path was needed.
No existing code became unreachable. The JavaScript project has no lint or
typecheck script.

The export suite passes 33 tests. Core bitmap suites pass 115 tests. The
separate GUI build succeeds; its full suite passes 506 of 507 tests, with no
skips. The remaining failure is the previously recorded processing-batch-controls
backgroundMode 2 admission mismatch. The workspace run passes 505 of 507:
its additional failure checks the intentionally preserved historical preview
title. The fresh build passes that title check. No test was skipped or
weakened to hide either result.

evidence/bitmap-gui-backend-verification-checkpoint.json records source
identity, native helper and fixture audits, build results and exact test
totals. All 123 packaged core/CLI files remain unchanged, and the isolated
copy matches all 70 checked GUI files plus those 123 package files.

## Remaining scope

This change supplies the bitmap download backend. It does not complete the
bitmap widget, custom width/height/percentage editor parser, separate basic
and advanced persisted choices, or direct bitmap Quick Save transactions.
The size fields are custom text controls, so spin-box assumptions cannot
define their admission rules. Native flatten-dialog acknowledgement and
broader flatten cases remain unverified.

The existing preview build and running previews remain preserved. Verification
uses a separate owned copy and build. The original application and accepted
reference helpers are unchanged. Publication, full application parity and
the Rust/GPUI port remain pending.
