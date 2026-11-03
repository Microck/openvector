# The document

## Summary

One single-image document holds the source image, selected settings, segmentation,
edits and vector result. Source pixels and exported files are separate.

## The simple case

Load a source, compute a result and export it to a new file. Export does not turn
the raster source into a project file or overwrite it automatically.

## The interaction, event by event

A source load is committed on successful decode. Failed Open preserves the previous
document. A computed result enables export. Settings can be changed before the
result is recomputed, so a selected setting alone does not prove a fresh result.
Editing owns undo/redo for segmentation or background changes. The application
has no general-purpose document save/restore workflow in this UI.

## Cancel and interrupt

| Event | Before extended | While extended |
| --- | --- | --- |
| Explicit cancel | Keep the existing document if a chooser is canceled. | Wait for cancellation acknowledgement before assuming a result. |
| Another command | A new accepted source can replace the document. | Conflicting processing actions are blocked. |
| Focus or process loss | Focus loss does not save anything. | Process exit loses unsaved edits and result state. |
| Target changes outside the app | Source and output paths are independent. | A saved file can change independently of the in-memory document. |
| Input device or modifier changes | No independent document effect. | The active gesture may change editing behavior. |

## Open questions and verification

Replacement warnings and output-error preservation need runtime checks.

Source reviewed: `vector-magic-re` commit `e794628c8ea3fbcfd4903e55c95ac29894846715`. Runtime verification pending.
