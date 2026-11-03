# Desktop usability contract

This pass improves the native GUI's open → convert → inspect → export workflow.
It intentionally improves interaction behavior rather than preserving confusing
labels or destructive replacement behavior from the reference application.

- Open, recent-file and drop replacement must ask before discarding manual palette,
  segmentation or background edits. Cancel preserves the document. Accepted Open
  still preserves the previous document if decoding fails.
- Starting a batch must ask before replacing an edited single-image document.
- Adding files to the desktop batch queue must preserve the displayed export folder.
- Ctrl/Cmd+O opens an image and Ctrl/Cmd+S opens vector export when a result exists.
  These shortcuts do not fire through a custom modal or consume ordinary text input.
- Up and Down pan buttons follow the corresponding focused-view arrow keys.
- Quick Save preserves dots in the filename. Only a supported final output suffix
  selects a format; the source suffix is removed once from suggested names.
- Bitmap export uses the scale displayed for the current Basic or Advanced mode.
- Primary actions keep text labels alongside icons. Icon-only controls have names
  and hover help. Bitmap, segmentation and vector layers have visible names.
- The first-run page explains supported input and the next conversion step.
  Mode choices explain their difference. Start is named Start over; Review's Next
  names its destination, Export. No decorative animation is introduced.

The detailed behavior and verification results live in [product description](product-description/README.md).

Editing names its two recovery actions separately: Undo all resets tool history;
Discard changes restores the last applied settings and edits. Tool hover help
explains Fill, Pencil and Zap. Errors tell the user how to continue.

Batch output names must remain distinct on case-insensitive Windows filesystems.
A file appearing after overwrite preflight must not be silently replaced. Existing
files may be replaced only when covered by the user’s overwrite decision.

Deactivating the window releases temporary B/V comparison layers and Shift-to-pan,
so keys released in another application cannot leave the viewer stuck.

## Batch safety decision

OpenVector intentionally differs from the original app where reproducing its
behavior could replace an unapproved file. Batch names reserve case-insensitive
stems, and unapproved outputs use atomic exclusive creation. Existing reference
captures remain unchanged as historical evidence; executable expectations must
check the safer behavior and retain comparisons for unaffected behavior.

This follows [Microsoft's file naming rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file),
the documented [CREATE_NEW behavior](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
and Rust's [atomic create_new guarantee](https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create_new).
Sources checked on 2026-10-29. The user delegated this choice based on research.
