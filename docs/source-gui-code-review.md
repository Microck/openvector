# Recent-file controls

The Load Image page now shows three cached recent-file links. Each row contains
its thumbnail, basename and native size label. The full path remains in its
tooltip and accessible name. A native disclosure element owns expansion and
keyboard behavior. The modernized presentation starts expanded; the original
widget's default and retained expansion state remain unverified.

Open, Browse, the empty viewer's Open Image action and Ctrl-O use the source
host's OS picker. Browser drops keep their byte-only path. The replaced hidden
browser file input and its ref were removed as part of this control replacement.
No old picker fallback remains. The existing upload handler still serves drops.

The original Load Image help and `recent_images_open.png` supplied the visual
reference. Octen's design and general searches both returned connection timeouts.
No beta-access or missing-key claim was inferred from those network errors.

## Behavior checks

The client adopts a newly opened source through one helper shared by disk opens
and byte drops. The source host explicitly reports whether an image loaded before
a history-save failure. That flag lets the client start the new wizard and show
the actual image while preserving the error. Failed reads do not report a loaded
source and preserve the current page and image.

Browser checks exposed a global shortcut handler consuming Enter on disclosure
headings. Adding `summary` to the existing interactive-control exclusion restores
the native HTML disclosure's keyboard handling. The final browser run covers
keyboard collapse, expansion and reopening; filename promotion; missing-file
preservation; history-write failure; restart; and a Batch workspace round trip.

The read-only browser DOM does not expose canvas `getContext`. Browser evidence
therefore checks thumbnail dimensions and appearance, not canvas pixel buffers.
Exact cached RGBA bytes remain covered by the preceding native storage corpus
and source-controller tests. This distinction is explicit in the checkpoint.

## Simplification review

The ce-simplify-code reuse, quality and efficiency rubrics ran inline. Independent
contexts: zero. The review does not provide independent corroboration.

- Reuse: no further change. The existing Bitmap renderer serves both the viewer
  and recent rows. Its label can be omitted for decorative thumbnails.
- Quality: no further change. The worker remains the source-history owner; the
  React parent retains its projection across workspace changes. The loaded flag
  describes an operation outcome rather than comparing image contents to infer
  whether loading happened.
- Efficiency: no further change. Failure replies carry history and the loaded
  flag beside the existing document state, avoiding a second full bitmap and an
  extra history request. Thumbnail pixels remain cached at the decode boundary.
- Newly unused production elements: none found. Removed picker elements were
  replaced controls, not unrelated cleanup.

The final isolated build passes all 321 GUI tests. No lint or typecheck script is
configured. The served workspace build and the user's preview hosts were left
intact. Both temporary preview hosts and their tabs were closed.

Full parity remains unproven. Native picker interaction, native file-error dialog
outcomes, clipboard acquisition, recent directories, global Reset Everything,
complete selected-settings persistence and other application gaps remain. Opening
a picker or replacing a source while another wizard page owns pending work still
needs native and browser coverage. No GitHub or npm publication occurred.
