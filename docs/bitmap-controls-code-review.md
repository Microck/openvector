# Bitmap controls and persistence

The export screen now renders BMP, JPEG, PNG and TIFF choices, coupled width
and height fields, native small/large steps and percentage text. The worker's
existing locale parser and BitmapSize owner settle every command. The browser
queues blur commits before downloads and restores normalized text even when
the settled numeric value stays unchanged. Downloads use the canonical bitmap
exporter and its attachment filename. Non-PNG transparency requires explicit
confirmation; cancellation returns without a download. Focus returns to the
download button after the request finishes and the button becomes enabled.

GuiEngine owns separate basic/advanced bitmap choices and one shared scale.
The existing QuickSave filename/directory owner now accepts an explicit bitmap
kind with its four formats and PNG default. Vector admission remains unchanged.
Ordinary source events replace bitmap folders and suffixed filenames; blank
source events retain filenames and select the working directory's parent.

The canonical preference file adds bitmapSave with basic/advanced persistent
choices and a shared finite scale. Session filenames and presentation text are
not stored. Startup applies scale through the cold 150 by 250 size handlers.
The 21 immutable window workflows compare the resulting choices, scale and
folder lifetimes. Raw Qt seeds are translated only in fixture setup. Product
runtime accepts one current JSON schema, with explicit recovery for old files.
Native raw-registry reading and native writer execution are not claimed.

Tailwind now scans gui/src explicitly. Without a VCS ignore file, a later
build could otherwise scan its own generated output and grow the stylesheet.
Two consecutive isolated builds produce identical bytes after this change.

## Review and verification

The ce-simplify-code reuse, quality and efficiency rubrics ran inline with zero
independent reviewers. No additional behavior-preserving simplifications were
needed: reuse 0, quality 0, efficiency 0, skipped 0. The implementation already
shares the filename owner, size owner and renderer. A focus defect found during
browser verification was fixed as observable behavior before final checks.
No existing code became unreachable and no current-task probe code remains.

The owning invariant for pane choices, source/reset lifetime and persistence is
integration. Coverage was added to the existing bitmap-size and processing-
preferences suites. Existing vector tests exercise the shared filename owner's
unchanged format set. The HTTP bitmap suite still owns complete file transport;
its two scale paths compare 68 files. Browser evidence separately checks field
commit ordering, actual downloads and the confirmation/focus interaction. No
mocks, new standalone test files or replacement reference expectations were used.

The 125 scoped tests pass. The final isolated GUI suite passes 632 of 633 tests,
with zero skips. Its one failure remains the recorded batch backgroundMode 2
admission mismatch. The isolated build succeeds. No lint or typecheck script is
configured. Source identity, immutable reference guards and browser artifacts
are recorded in evidence/bitmap-controls-verification-checkpoint.json.

## Remaining work

Direct bitmap saving and rendered directory/filename controls remain pending.
View-scale and scale-preset producers, preprocessing/batch field lifetimes,
transient native signal ordering and native correction-cycle behavior still
need evidence or implementation. These checks do not establish full parity.
Existing previews, historical builds, the original installation/settings and
the packaged core/CLI remain unchanged. Publication and Rust/GPUI are unfinished.
