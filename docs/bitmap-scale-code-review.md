# Bitmap scale producers

The bitmap export pane now exposes the native x1, x2 and x4 buttons, eight
More scales, and Use view scale. Presets and view scale enter the existing
BitmapSize owner as absolute doubles. They do not round-trip through percentage
text. The chosen pane refreshes while the other pane retains its field text.

Viewer updates one shared view reference synchronously at each existing zoom or
pan action. The export button reads that reference at the click. React still owns
the viewer presentation; panning does not render the whole wizard. Preference
validation moved unchanged into processing-preferences, its sole consumer, so the
browser can import the scale catalog without importing the filesystem owner.

## Reference evidence

REA analysis resolves the constructor, preset registration, menu text parsing and
From View handler. At 0x41d280 the native handler calls 0x445a60, stores the double
and calls the existing scale update. Helper 37 reads existing button lists,
QAction text, slider bounds and view getters. Six processes reproduce three
states. The first QAction is the menu's own More title; the eight remaining
actions are x5, x6, x8, x10, x12, x16, x20 and x24.

The three getter doubles retain their raw bits. Six further processes pass those
values to the already accepted model bitmap-scale and bitmap-save operations.
They produce three repeat-identical PNG files. This establishes those model
inputs and file results. It does not execute the native From View widget slot.
All earlier 36 native helpers remain byte-identical. No widget setter, native
input, dialog answer or original saved setting was used or changed.

## Review and coverage

The ce-simplify-code reuse, quality and efficiency rubrics ran inline, with zero
independent reviewers. The settled implementation needed no further changes:
reuse 0, quality 0, efficiency 0, skipped 0. No existing code became unreachable.
There are no configured lint or typecheck scripts.

The consolidate-test-suites invariant is absolute scale admission and transport
without percentage conversion or peer-field refresh. The existing bitmap-size
unit suite owns catalog, raw-double, normalization and atomic rejection rules.
The existing export-preferences integration suite owns HTTP/worker serialization,
workspace locks and three complete native PNG comparisons. These cover distinct
failure modes. Both native corpora and their recorders are promoted as durable
reference evidence. There are no mocks, standalone product test files or
temporary product instrumentation to remove.

The 87 scoped tests pass. The isolated GUI build passes; its full suite passes
648 of 649 tests with zero skips. The remaining processing-batch-controls failure
rejects backgroundMode 2 and is unchanged. Browser checks confirm x4, x24, all
eight menu choices, current-view scale at 1 and 1.25, and an unrounded fit value
of 47.666666666666664. The x2 browser download matches the native 32 by 24 PNG
byte for byte. The saved panel screenshot shows the controls in the clean build.

## Remaining parity work

Native getter 0x445a60 maps slider positions 0 through 300 logarithmically from
0.2 to 24. Native wheel handler 0x4474a0 moves the slider by wheel delta divided
by 12 and adjusts the anchor. The current browser controller has a different
range, mapping, wheel step and fit behavior. The new binding is correct for the
current renderer; native viewer interaction parity remains incomplete.

Exceptional numeric conversions, correction cycles, preprocessing and batch
bitmap-field lifetimes, error notification timing and full application parity
remain incomplete or unverified. The core package, CLI, historical captures,
root GUI build and existing previews remain unchanged. Publication and Rust/GPUI
work are not complete.

The checkpoint is evidence/bitmap-scale-verification-checkpoint.json.
