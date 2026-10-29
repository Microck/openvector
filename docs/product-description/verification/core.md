# Core workflow verification

| ID | P | Needs | Claim | Setup | Steps | Expected | Result |
| --- | --- | --- | --- | --- | --- | --- | --- |
| OPEN-01 | P1 | Windows candidate | [Load](../single-image/open.md) | Fresh preferences | Open supported image | Mode choice, source visible | pending |
| OPEN-02 | P1 | Candidate | [Failed replacement](../single-image/open.md) | Computed result | Open corrupt file | Error; previous result retained | pending |
| OPEN-03 | P1 | Candidate | [Manual edits](../single-image/open.md) | Customized palette | Open another source, Cancel | Work retained | pending |
| OPEN-04 | P2 | Candidate | [Chooser cancel](../single-image/open.md) | Loaded source | Open, cancel chooser | Same document | pending |
| CONV-01 | P1 | Windows | [Automatic](../single-image/conversion.md) | Loaded image | Fully automatic | Review and exportable result | pass: Windows candidate ad58ddf6, automatic conversion reached Review and Quick Save wrote SVG |
| CONV-02 | P1 | Candidate | [Basic](../single-image/conversion.md) | Fresh image | Basic through result | Review and exportable result | pass: Windows candidate ad58ddf6, artwork with blended edges and flexible palette reached Review |
| CONV-03 | P1 | Candidate | [Advanced](../single-image/conversion.md) | Fresh image | Advanced through all stages | Exportable edited result | partial: Windows candidate ad58ddf6 traversed all Advanced pages to Export; edit/export visibility still pending |
| CONV-04 | P2 | Candidate | [Cancel](../single-image/conversion.md) | Processing image | Cancel, wait, retry | Responsive; no false completion | pending |
| KEY-01 | P1 | Candidate | [Shortcuts](../foundations/input.md) | Idle root focus | Ctrl+O, cancel; Ctrl+S with result | Correct dialogs | pending |
| KEY-02 | P2 | Candidate | [Modal focus](../foundations/input.md) | Custom form open | Tab, Shift+Tab, Escape | Focus stays in form; Escape closes | pending |
| SET-01 | P1 | Candidate | [Profile](../settings.md) | Advanced result | Name, save, apply | Same chosen settings | pending |
| SET-02 | P2 | Candidate | [Reset](../settings.md) | Populated history | Reset, Cancel | Lists and image preserved | pending |
| SET-03 | P2 | Candidate | [Restart](../settings.md) | Saved test preferences | Close/reopen QA app | Choices restored | pending |
| UI-01 | P2 | Candidate | [Reachability](../foundations/navigation.md) | Minimum window size | Visit every page and dialog | Controls visible or scrollable | pending |

Candidate `ad58ddf6e1fbfd7af808575736eeefd1f509ef2957eb3b9dc6b352b2f400b767`
ran in a separate Windows test directory with disposable preferences. Runtime receipts
are retained locally under `evidence/completion-native-reconciliation-20261028-r1/`
with the `gui-candidate-` prefix. These checks do not certify later source changes.
