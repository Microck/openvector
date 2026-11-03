# Glossary

**Source image.** The loaded raster input. Processing does not rewrite this file.

**Document.** The current source image, chosen settings, intermediate segmentation,
edits and vector result held by the app.

**Result.** Computed vector geometry available for inspection and export.

**Selected.** The currently chosen option, tool, layer or queue entry.

**Changed.** A setting or editing transaction differs from its last committed state.
A changed setting does not itself mean a new result has been computed.

**Saved.** Output bytes have been written to a selected path. This is separate from
saved preferences and does not imply an editable project file exists.

**Running.** The worker is processing a command; the footer shows progress and Cancel.

**Done.** The worker acknowledged completion. A result can be done without being saved.

**Cancel.** Abandon a dialog or request cancellation of running work. It does not
promise to reverse already completed operations or previously written batch files.

**Interrupt.** A change during an interaction, including another command, focus loss,
process loss, an external file change or a different input device.

**Page.** One step of the single-image wizard displayed beside the viewer.

**Layer.** The bitmap, segmentation or vector representation shown in the viewer.

**Palette.** The colors available to tracing or editing.

**Segmentation.** The source divided into colored regions before vector fitting.

**Profile.** Named processing settings reused for another conversion.

**Batch.** A queue of source files converted to a chosen output folder.
