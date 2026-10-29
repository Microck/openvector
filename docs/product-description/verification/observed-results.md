# GUI QA observed results

These results cover the current GUI QA changes over baseline
`e794628c8ea3fbcfd4903e55c95ac29894846715`. They do not mark every manual
checklist row complete.

## Automated checks

| Environment | Check | Observed result |
| --- | --- | --- |
| Task-owned Linux Raft workspace | GUI library and integration suites | 100 passed, two ignored |
| Same workspace, Xvfb and Mesa Vulkan | Separately invoked headed smoke | Passed; opened a GPUI window and exported an SVG matching the retained Windows reference bytes |
| Linux | Batch queue and runner suites | Eight passed, including case-only names and a late-created output sentinel |
| Linux | Vector file suite | Five passed, covering serializer completion, cancellation, retries and open failures |
| Windows 10, isolated test directory | Batch queue and runner suites | Eight passed |
| Windows 10, isolated test directory | Vector file suite | Seven passed, including 348 native file-access probes |

The two ignored tests in the ordinary GUI suite are the locale check and the
headed smoke. The headed smoke was then run explicitly. The locale check remains
unverified. Export tests initially lacked their reference fixtures in Raft; after
copying the original fixtures, all five passed without changing their assertions.

The batch tests preserve the original captures and explicitly assert the approved
[safety differences](../../gui-usability-contract.md).

## Windows GUI observations

The candidate executable SHA-256 is
`4dd84e10bb449db012dc91c0066437e79f3b50832db261b4c003aed545d76c0f`.
Its preferences, sources and outputs are separate from the user's original app.

- Automatic conversion reached Review with Ready status.
- Basic conversion passed through quality and palette selection to Review.
- Advanced conversion traversed Palette, Segmentation, Editing, Smoothing and
  Background, then reached Export.
- Quick Save preserved intermediate dots in `artworkqa.multi.part.svg`; the
  resulting SVG contained 1,945 bytes.
- Fill enabled Undo, Undo enabled Redo, and Redo restored Undo.
- Pan Up moved the vertical position from 144 to 111; Down returned it to 144.
- Opening a recent image after leaving Editing showed a replacement confirmation.
  Cancel dismissed it without replacing the document.
- At a 760 by 500 client size, the mode actions and guidance fit. Lower toolstrip
  controls require scrolling; other pages have not all been checked at this size.

## Remaining coverage

Native chooser automation was inconclusive. Drag export, bitmap output dimensions,
comparison-key release across native focus changes, full keyboard navigation and
packaged batch interaction still need observation. Automated tests cover the
changed bitmap scale selection and focus-loss state, but do not replace those
native checks. Pencil Undo still records individual pointer samples.

The user may be testing the candidate. Further automation must use a separate
instance and must not send input to or close that window. The original manual-test
instance and historical reference captures remain untouched.

Detailed test logs and exact Windows executable hashes are retained locally under
`evidence/gui-qa-20261029/` and the `gui-core-tests.json` receipt in
`evidence/completion-native-reconciliation-20261028-r1/`.

## Prepared package

The final unpublished Windows QA package contains 17 files, including the app
license and third-party notices. Its executable SHA-256 is
`e29ff767b9df09ec117e140cac8271a586b708e923ffe054cd290e9fe0ebfe24`.
The ZIP SHA-256 is
`37c212c87eb1460ab873978f709a2604643fdc1be1b37932cc2d30242501403f`.
Every extracted member matched its recorded length and hash. Packaging used the
repository's GUI packager and locked dependencies. This final binary differs from
the observed candidate by the reviewed output-path reuse change; its focused batch
test passed. Native GUI checks of this packaged binary remain pending.

The user is using the Windows desktop and expects it to be available in roughly
five hours. Interactive checks are deferred; existing windows remain untouched.
