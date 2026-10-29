# Document segmentation interruption

OpenVector now runs segmentation through the same numerical kernels for pure
library calls and document tasks. The document owns a private live view of raw
labels and palette state. Exported region counts and color IDs have a separate
lifetime. New label storage is explicitly unavailable until its first producer;
same-sized initialized labels remain visible at the opening checkpoint.

The owner publishes actual producers in tiling, connected rebuilding, axis and
diagonal optimization, subpixel statistics, color selection, palette transfer
and final region export. It deduplicates phase/substep/counter tuples and uses
the recovered float32 table endpoints in exact double arithmetic. Observing
progress never calls the optimizer accessor that compresses parent links.

The diagonal branch also reproduces the temporary live minimum-pixel parameter
write. Cancellation retains that write until profile loading on retry. Document
unwinding clears dependent keys, returns status 2 with progress 100, and keeps
the interrupted storage without committing requested settings. Host observer
errors keep their identity. Incomplete fitting cannot reach either vector or
bitmap export.

## Review and test ownership

The existing document-cancellation integration suite owns interruption state,
cleanup and retry. It now covers the 688 frozen segmentation cases through both
the direct method and runTask, alongside its existing smoothing/fitting cases.
Native outputs supply assertions only. Declared pixels and controls supply all
independent numerical inputs. The observer-error case belongs to this same
suite. No standalone production test or duplicate fixture was added.

Applied ce-simplify-code's three complete review rubrics inline, with zero
independent reviewers. Reuse: 0 findings. Quality: 1 applied change, expressing
subpixel progress offsets relative to a defined observer context. Efficiency:
4 applied changes, avoiding unused connected-state construction, observer-view
allocation, palette progress traversal and per-pixel progress arithmetic when
no observer is attached. No arithmetic kernel or validation was simplified.
No additional finding was skipped.

Consolidate-test-suites disposition: MERGE for the new assertions in the
existing document integration suite; PROMOTE for the repeatable byte-table,
source/snapshot/archive/installation auditor. Installed copies test package
resolution and real Node permission denials, a separate boundary from document
semantics. Failed diagnostic logs remain evidence. No source became unreachable
and no deletion is pending. No root lint or typecheck script is configured.

## Verification boundary

The implementation checkpoint records exact results after the simplifications,
including the full core run, fresh isolated GUI build/tests, and installed API
run. The package comparison covers every archived file. The installed run
actually attempts denied reads of the original executable and workspace source,
and denied child creation. It makes no worker permission-inheritance claim.
The auditor checks all 145 segmentation progress rows against original source
bytes, including shadow rows not reached by the frozen interruption fixtures.

The three existing failures remain visible: 16-bit Adam7 native uninitialized
rows, alpha-premultiplied editing with an invalid native live palette ID, and a
setter-only batch background value with no established valid producer. No test
was weakened or skipped to accommodate them.

Segmentation forwarding through batch, workers and GUI is unfinished. Shadow
interruption hooks have static source evidence and completed numerical tests,
but lack full native cancellation captures. Palette-analysis handoffs, resized
retained storage, editing contexts, old contour validity and broader original
GUI/scheduler behavior remain open. Full parity, publication and Rust/GPUI
porting are pending. The original installation, accepted helpers, root gui/dist
and existing previews are preserved.
