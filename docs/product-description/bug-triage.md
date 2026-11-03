# GUI bug triage

| ID | Severity | Where | Status | Decision |
| --- | --- | --- | --- | --- |
| GUI-01 | high | Open/recent/drop and batch start | Page-independent confirmation implemented; 12 focused input tests pass; Windows warning and Cancel verified | fix |
| GUI-02 | medium | Viewer pan arrows | Reproduced on Windows; button/key parity test passes; Windows candidate moves 144 to 111 and back to 144 | fix |
| GUI-03 | medium | Quick Save dotted filenames | Candidate preserves dots; focused test passes; controller expectation updated | fix |
| GUI-04 | medium | Keyboard Open/Save | Shortcuts implemented; candidate test pending | fix |
| GUI-05 | low | Empty state, modes, layers and actions | Guidance, labeled icons and hover help implemented | fix |

## GUI-01: edited work can be replaced without warning

Open routes immediately to the worker, whereas Paste already asks. Batch also
reuses the document without asking. The replacement can discard palette,
segmentation or background edits. Confirmation now shares the manual-edit
predicate and retains all work when canceled. A failed accepted Open still uses
the existing preservation behavior. The Windows candidate showed the warning after leaving Editing, and Cancel preserved the current document.

## GUI-02: vertical pan controls disagree

From vertical position 123, the released Up button moves to 156 and Down returns
to 123. The focused-view Up key subtracts instead. Evidence: local
`gui-qa-pan-r2.json`. Both button signs are corrected to follow arrow-key semantics.

## GUI-03: filename truncation

`QuickSave::set_filename` uses the text before the first dot. Entering
`logo.final.svg` can become `logo.svg`, risking a wrong target and a surprising
replacement prompt. Preserve the full name before a supported final extension.

## GUI-04: expected file shortcuts are absent

The root handler supports Paste and wizard navigation but has no Ctrl/Cmd+O/S.
Add Open and vector export shortcuts behind the modal guard.

## GUI-05: weak first-run and comparison guidance

The empty viewport contains only a short drop hint. Mode choices do not explain
what they change, and B/S/V labels require prior knowledge. Add a visible first
action, brief mode guidance, labeled icons and named comparison layers.

## Further confirmed findings

- Bitmap export used the last edited mode's scale instead of the visible pane.
  Export now selects `pane_scales[mode]`; the controller test passes for both modes.
- Editor history reset and applied-state restoration both said Revert. Native QA
  showed both at once. They now read Undo all and Discard changes.
- Zap's uniform-region error exposed an internal implementation limitation. It now
  directs the user to Fill. Editing tools also have hover help.
- Leaving Editing could bypass the replacement warning. The guard now reads the
  document's `editing_changed` flag on every page; the focused test passes.
- Adding batch files replaced the chosen output folder. The GUI now preserves it;
  the focused test passes.
- Case-only batch output names collided, and targets appearing after preflight
  could be replaced. All eight Linux queue/runner tests pass, including preserved late-target bytes.
  All eight batch tests also pass on Windows; packaged batch interaction remains pending.

Pencil history still records pointer samples separately. Stroke-level undo has
not changed in this pass. Native picker automation, held comparison keys across
focus changes, drag handoff and interrupted writes remain runtime checks.
Static inspection is not a runtime pass.
