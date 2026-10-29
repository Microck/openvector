# Export verification

Results distinguish native observations from source and controller tests.
Controller tests do not constitute a Windows GUI pass. Use the
isolated preferences and disposable files described in [Verification](README.md).
Record binary hash, commit, screenshots and output paths with each result.

| ID | P | Needs | Claim | Setup | Steps | Expected | Result |
| --- | --- | --- | --- | --- | --- | --- | --- |
| EX01 | P1 | GUI | [Export eligibility](../single-image/export.md#summary) | Loaded source only | Try save; convert and revisit | No save before vectors; save after conversion | Pending |
| EX02 | P1 | GUI, SVG reader | [Simple save](../single-image/export.md#the-simple-case) | Computed result | Select SVG; Save vector file to new path; open output | Readable vectors; document remains open | Pending |
| EX03 | P1 | Disposable source aliases | [Source protection](../single-image/export.md#ending-at-once) | Source with vector suffix and hard link | Save to source and alias paths | Error; unchanged source bytes | Pending |
| EX04 | P1 | Existing output | [Replace confirmation](../single-image/export.md#becoming-extended) | Known destination bytes | Save there; cancel; repeat and Continue | Cancel preserves bytes; Continue replaces output | Pending |
| EX05 | P1 | Transparent fixture | [Flattening](../single-image/export.md#becoming-extended) | Transparent result; no background | Save JPG without confirmation, then confirm; change format and retry | Explicit consent required; new format clears it; PNG preserves alpha | Pending |
| EX06 | P1 | GUI | [Dotted names](../single-image/export.md#edge-cases) | Quick Save form | Enter logo.final.svg; save | Preserve logo.final.svg | pass: candidate ad58ddf6 saved artworkqa.multi.part.svg with every dot preserved; exact-name controller test passes |
| EX07 | P1 | Bitmap reader | [Separate mode sizes](../single-image/export.md#edge-cases) | Same source; Basic and Advanced result | Set Basic 200%; Advanced 400%; return to Basic; save | Pixels agree with displayed Basic dimensions | controller test passes for both modes; native bitmap dimensions pending |
| EX08 | P1 | File manager, vector receiver | [Drag handoff](../single-image/export.md#finishing) | Ready drag file | Drag to folder; then vector app | Correct selected format; readable current result; drop feedback | Pending |
| EX09 | P1 | GUI | [Cancellation](../single-image/export.md#cancel-and-interrupt) | Large output; disposable existing target | Start save; Cancel; inspect bytes and result | Acknowledged cancellation; record actual partial-write behavior | Pending |
| EX10 | P2 | GUI | [Chooser cancel](../single-image/export.md#ending-at-once) | Result | Open Save vector file; cancel | No output or document change | Pending |
| EX11 | P2 | GUI | [Configure retention](../single-image/export.md#ending-at-once) | Quick Save form | Change folder/name/format; close; reopen | Choices retained; no output | Pending |
| EX12 | P2 | Disposable folders | [Create directory](../single-image/export.md#becoming-extended) | Missing output directory | Save; cancel; retry Continue | Cancel creates nothing; Continue creates directory and output | Pending |
| EX13 | P2 | GUI | [Empty filename](../single-image/export.md#ending-at-once) | Quick Save | Clear filename and save | Clear error; no file | Pending |
| EX14 | P2 | All vector readers available | [Format choices](../single-image/export.md#starting) | Computed result | Save AI/DXF/EMF/EPS/PDF/SVG | Each selected output reads correctly; unavailable readers recorded | Pending |
| EX15 | P2 | GUI, vector reader | [Vector options](../single-image/export.md#starting) | Overlapping shapes | Save each shape mode; DXF curve mode; Thin outline; Compact EPS | Options produce intended geometry/output; controls match format | Pending |
| EX16 | P2 | Bitmap reader | [Bitmap sizing](../single-image/export.md#starting) | Known aspect ratio | Apply width; height; percent; preset; Use view scale; save | Linked fields and actual pixels agree | Pending |
| EX17 | P2 | GUI | [Keyboard sizing](../single-image/export.md#modifiers) | Bitmap Configure | Enter in dimension field; then save | First Enter applies size; save writes expected dimensions | Pending |
| EX18 | P2 | Existing output | [Keyboard confirmation](../single-image/export.md#interactions-with-other-systems) | Replace dialog | Press Enter; reopen; click Continue | Enter dismisses without write; Continue replaces | Pending |
| EX19 | P2 | GUI | [Drag preparation](../single-image/export.md#starting) | Export page | Wait ready; change drag format and vector option | New prepared file matches current choices | Pending |
| EX20 | P2 | GUI | [Drag retry](../single-image/export.md#starting) | Unwritable task-owned drag directory | Trigger preparation failure; restore permission; Retry | Error visible; retry reaches ready | Pending |
| EX21 | P2 | GUI | [Drag cancel and minimize](../single-image/export.md#while-extended) | Ready drag; Minimize enabled | Start drag; cancel; restore app | App minimizes on system start; cancellation feedback; controls recover | Pending |
| EX22 | P2 | GUI | [Busy and modal guards](../single-image/export.md#cancel-and-interrupt) | Save/drag in progress | Try competing commands and modal background | No conflicting document replacement or second write | Pending |
| EX23 | P2 | Disposable permissions | [External target changes](../single-image/export.md#cancel-and-interrupt) | Accepted output | Change permission/target during workflow | Error or documented result; source remains intact | Pending |
| EX24 | P2 | GUI | [Focus and process loss](../single-image/export.md#cancel-and-interrupt) | Disposable outputs | Alt-tab during chooser/write/drag; separately terminate instance | Focus alone accepts nothing; inspect interrupted outputs and retained drag paths | Pending |
| EX25 | P2 | GUI | [Input changes](../single-image/export.md#cancel-and-interrupt) | Native chooser and drag | Start with pointer; finish/cancel using keyboard | Platform outcome matches feedback; no stuck drag | Pending |
| EX26 | P2 | GUI restart | [Preferences](../single-image/export.md#interactions-with-other-systems) | Separate mode choices | Restart; inspect; load a different source | Formats persist; new source sets its directory/name | Pending |
| EX27 | P2 | GUI | [Independent formats](../single-image/export.md#modifiers) | Different main/Quick Save/drag formats | Use each path | Output follows that path's selected format | Pending |
| EX28 | P2 | GUI | [Native extension](../single-image/export.md#edge-cases) | Main Save SVG selected | Accept .pdf then no suffix | Record actual output and errors; never silently mislabel bytes | Pending |
| EX29 | P2 | GUI, bitmap reader | [Bitmap formats](../single-image/export.md#starting) | Opaque result | Export BMP/JPG/PNG/TIF | Readable files with displayed dimensions | Pending |
| EX30 | P2 | GUI | [Reachability](../single-image/export.md#open-questions-and-verification) | Minimum window size | Reach every export section with pointer and keyboard | All controls visible or scrollable and operable | Pending |

## Resolved source findings

EX06 used to strip text after the first dot. Quick Save now preserves intermediate
dots and recognizes only a supported final output suffix. The unit test and all
eight controller tests pass. A native Quick Save produced the dotted name recorded
above; its exact candidate hash is in the core verification record.

EX07 used a shared last-edited scale despite displaying separate Basic and Advanced
sizes. Export now uses `pane_scales[mode]`. The focused real-controller test switches
between modes and verifies the admitted export scale while preserving an existing
output. Native pixel-dimension verification remains pending.

## Runtime questions

Native chooser focus, drag handoff, bitmap output dimensions and interrupted writes
remain open. The original source review used commit
`e794628c8ea3fbcfd4903e55c95ac29894846715`; current-source fixes do not imply a pass
for the remaining native rows.
