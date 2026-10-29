# Native zoom arithmetic in the viewer

The viewer now retains displayed scale and integer slider position separately.
Fit and 100 percent set the displayed scale directly, then synchronize the
slider with the native inverse conversion. Slider actions use their separate
producer calculation. Bitmap Use view scale reads the getter for the current
slider. It does not read the displayed matrix scale.

The three native read-only observations establish this difference directly.
Cold state displays 1 with slider 100 and getter 0.9864848297321882. A 16 by 12
image fits the captured 133 by 133 available area at 8.3125, with slider 233 and
getter 8.238739353414875. The tall image likewise has distinct matrix and getter
values. The same observations retain native center offsets caused by integer
scrollbars; this change does not yet reproduce those offsets.

## Independent runtime values

The source-written x87 formulas are the initial direct implementation. They
preserve the getter's binary64 spill, the slider producer's retained logarithm,
and inverse truncation. A separate scalar oracle replays only the original
numeric spans in private allocations. Every arithmetic instruction is audited;
only receiver/state bookkeeping, argument/return wrappers and numeric constant
addresses differ. No Qt or application initialization occurs in this scalar
process. All earlier 39 helpers remain unchanged.

The independent formulas agree at all 301 slider positions, 300 inverse
transitions and 4,099 sampled scales. Getter and slider producer results differ
at 34 positions. Two reference processes are repeat-identical. The generator
also runs with '-' instead of an application filename: that branch never reads
the original executable. Runtime arrays come exclusively from this generator,
not from expected fixtures or copied instruction bytes.

The runtime uses finite-domain arrays for the two slider functions and binary
search over the inverse transition boundaries. This preserves the verified
numeric behavior in portable JavaScript without shipping native code or invoking
a native process. The original application and Qt DLLs are not dependencies.
The explicit generate command writes the source. Audit regenerates in an owned
temporary directory and compares bytes without rewriting product source.

## Browser and test evidence

The Chromium wheel producer supplies wheelDeltaY as an integer angle. It does
not estimate notches from pixel deltaY. Missing native angle input reports an
explicit diagnostic. A real Chromium vertical wheel event advances ten slider
positions. Home, End, Page Up and the zoom button admit the expected positions.
At displayed 100 percent, Use view scale sends 0.9864848297321882 to the worker;
the downloaded PNG matches the native view-scale-getter-cold file byte for byte.
No browser errors or warnings were observed.

The existing viewer-input unit suite owns numeric admission, native scale
conformance, transition neighbors, measured fit scale and angle-to-slider steps.
It now has 12 passing tests. It reads the owned input images and measured view
geometry independently of expected scales. Existing HTTP suites retain ownership
of worker serialization and native bitmap bytes. The full isolated GUI suite
passes 653 of 654 tests with no skips. Its single failure is the unchanged
processing-batch-controls backgroundMode 2 comparison. The GUI builds successfully.
No lint or typecheck script is configured.

The ce-simplify-code reuse, quality and efficiency passes ran inline, with zero
independent reviewers. Reuse 0, quality 1, efficiency 0, skipped 0. Audit no longer
rewrites generated product source. The zero-area fit correction is a separate
behavior fix from the native ratio contract, not a behavior-preserving cleanup.
All runtime exports have current callers. No existing code became unreachable.

The consolidate-test-suites disposition is PROMOTE for the metric and numeric
corpora, their recorders and the independent generator. Coverage extends the
existing canonical viewer-input suite. No mock, new standalone product test or
temporary product instrumentation was added. Generated lookup values remain
outside the hand-written simplification scope. Existing native helpers stay
frozen as reference evidence.

## Remaining parity work

Native center retention, integer scrollbar placement, scene bounds and clipping,
exact x87 wheel-anchor arithmetic, horizontal and non-Chromium wheel input,
preprocessing, resize and batch lifetimes remain unfinished or unverified.
The current pan geometry is retained and full viewer parity is not claimed.
Full application parity, publication and Rust/GPUI are not complete. Existing
previews, the root GUI build, core package and CLI remain unchanged.

The checkpoint is evidence/viewer-zoom-runtime-verification-checkpoint.json.
