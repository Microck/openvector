# Batch verification

All rows are pending runtime verification. Use an isolated Windows instance,
disposable source copies and output folders, as described in [the pass instructions](README.md).
Record binary hash, source commit and evidence with every result. Static source
inspection is draft evidence only. Never use original images or valuable outputs
for destructive checks. P1 suspected defects describe the source behavior to test;
a reproduced defect remains a fail for product QA even when this draft predicts it.

| ID | P | Needs | Claim | Setup | Steps | Expected | Result |
| --- | --- | --- | --- | --- | --- | --- | --- |
| B01 | P2 | Windows | [Workspace entry](../batch.md#summary) | Idle app | Choose Batch, then Single image, then Batch | Batch appears without requiring a document; queue remains between visits | pending |
| B02 | P2 | Native chooser | [Multiple addition](../batch.md#starting) | Two readable images | Add both through Add images | Both rows appear in chooser order with source paths and previews | pending |
| B03 | P2 | Native chooser | [Chooser cancellation](../batch.md#ending-at-once) | Nonempty queue and chosen folder | Cancel Add images; cancel Choose folder | Queue and folder stay unchanged | pending |
| B04 | P2 | Windows | [Duplicate addition](../batch.md#starting) | Image A queued | Add A and new B | A appears once, B enters, Already in queue appears | pending |
| B05 | P2 | Windows | [Selection and removal](../batch.md#starting) | Three rows | Select two; Remove selected; Clear all | Selected rows leave, then remaining rows leave; source files remain | pending |
| B06 | P1 | Windows | [Selected row Remove](../batch.md#starting) | Three rows, two selected | Press Remove on a selected row | Both selected rows leave; unselected row remains; assess whether label is misleading | pending |
| B07 | P1 | Windows | [Selection does not filter run](../batch.md#modifiers) | Three valid rows, one selected, fresh output folder | Start batch | All three process and selection clears | pending |
| B08 | P2 | Windows | [Empty queue](../batch.md#ending-at-once) | Empty queue | Inspect Start batch | Start batch is unavailable | pending |
| B09 | P1 | Disposable folders | [Folder replaced on add](../batch.md#open-questions-and-verification) | Source folder A and chosen output B | Choose B; add another source from A | Output folder remains B | pending |
| B10 | P2 | Valid sources | [Successful conversion](../batch.md#the-simple-case) | Two valid rows, fresh writable output folder | Select SVG; Start batch | Two written rows, two converted, valid SVG outputs and retained queue | pending |
| B11 | P2 | Existing output | [Overwrite decision](../batch.md#becoming-extended) | One existing sentinel output and one fresh output | Start; choose Overwrite | One dialog lists conflict; both files convert and sentinel changes | pending |
| B12 | P1 | Existing output | [Skip decision](../batch.md#becoming-extended) | One existing sentinel output and one fresh output | Start; choose Skip existing | Sentinel bytes remain; only fresh output converts; summary is one converted, one skipped | pending |
| B13 | P1 | Existing output | [Cancel decision](../batch.md#cancel-and-interrupt) | Existing sentinel and fresh target | Start; choose Cancel in overwrite dialog | No outputs change; queue unlocks; cancellation appears | pending |
| B14 | P2 | Existing outputs | [All skipped](../batch.md#becoming-extended) | All targets exist | Start; Skip existing | No conversion, all skipped, queue editable | pending |
| B15 | P2 | Duplicate stems | [Suffix planning](../batch.md#becoming-extended) | Two different folders with logo.png | Add both; choose new output; start | Distinct logo and logo_1 outputs | pending |
| B16 | P1 | Windows case-insensitive disk | [Case collision](../batch.md#edge-cases) | Separate source folders with Logo.png and logo.png; fresh output folder | Add both; convert | Both sources produce distinct planned outputs, including a numeric suffix | pending |
| B17 | P1 | Large images | [Cancel during conversion](../batch.md#while-extended) | Several images and fresh outputs | Cancel after first output exists | Prior output remains; partial report shows pending rows; queue unlocks | pending |
| B18 | P1 | Partial run | [Retry](../batch.md#finishing) | Canceled batch with one written output | Start again; Skip existing | Written output stays unchanged; pending files process | pending |
| B19 | P2 | Bad input | [Load failure and continuation](../batch.md#finishing) | Corrupt image before valid image | Start batch | Failed-load row has error; later valid image writes; summary counts both | pending |
| B20 | P1 | Disposable permission setup | [Save failure](../batch.md#finishing) | Unwritable or missing output folder | Start batch | Failed-save feedback; no fabricated success; missing directory stays absent | pending |
| B21 | P2 | Large image | [Busy guards](../batch.md#while-extended) | Long batch | Try remove, select, folder, settings and workspace controls | Conflicting commands unavailable; queue and captured settings stay fixed | pending |
| B22 | P2 | Large image | [Focus interrupt](../batch.md#cancel-and-interrupt) | Running batch | Move focus to another app, then return | Run continues without implicit cancel or save command | pending |
| B23 | P1 | Disposable process | [Process exit](../batch.md#cancel-and-interrupt) | Running batch with one completed output | Terminate isolated app; relaunch | Completed bytes remain; session queue/report are not restored | pending |
| B24 | P2 | Several images | [External input change](../batch.md#cancel-and-interrupt) | Later queued source not yet read | Delete later source during earlier conversion | Later row reports failed-load; run is usable afterward | pending |
| B25 | P1 | Disposable sentinel | [External output appearance](../batch.md#edge-cases) | Long run; later output absent at preflight | Create sentinel at later output path after processing starts | Sentinel bytes remain unchanged; that row reports failed-save; later rows continue | pending |
| B26 | P2 | Keyboard and pointer | [Input change](../batch.md#cancel-and-interrupt) | Queued images | Start by keyboard; use pointer; press modifiers during run | Accepted queue/settings stay fixed; controls remain reachable | pending |
| B27 | P2 | Profile saved | [Profile variant](../batch.md#modifiers) | Distinct saved processing profile | Use in batch; start; try profile controls while busy | Chosen settings apply; changes are blocked during run | pending |
| B28 | P2 | Both modes | [Basic/Advanced settings](../batch.md#modifiers) | Valid image and separate fresh outputs | Run Basic, then Advanced settings | Each run uses its selected settings | pending |
| B29 | P2 | Single-image EMF | [Format fallback](../batch.md#modifiers) | EMF selected in Single image | Enter Batch | Format becomes SVG | pending |
| B30 | P2 | Valid image | [Formats](../batch.md#becoming-extended) | Fresh outputs for each format | Run SVG, EPS, PDF, AI and DXF separately | Each supported format writes an inspectable output | pending |
| B31 | P2 | Corrupt then repaired file | [Preview refresh](../batch.md#interactions-with-other-systems) | Queue corrupt file and see preview error | Repair file bytes; run it | Successful read replaces error with preview; skipped rows keep prior previews | pending |
| B32 | P1 | Unsaved edited image | [Document replacement](../batch.md#open-questions-and-verification) | Single-image segmentation/background edits | Enter Batch; convert another source; return to Single image | Confirmation appears before starting; Cancel preserves edits; Continue permits document replacement even if the first batch source fails decoding | pending |
| B33 | P2 | Saved preferences | [Preference scope](../batch.md#interactions-with-other-systems) | Different single and batch settings, chosen folder | Switch workspaces, restart app | Processing settings stay separate; export format/options are shared; persisted folder reloads; session queue empties | pending |
| B34 | P2 | Finished run | [Folder and report](../batch.md#the-simple-case) | Completed mixed-result run | Inspect rows, counts, log; press Show Export Folder | Report matches disk; file manager opens run's output folder | pending |
| B35 | P2 | Windows minimum window | [Reachability](../batch.md#open-questions-and-verification) | Many rows and long Unicode paths | Resize, scroll, tab through controls and overwrite dialog | Paths and controls remain accessible; focus returns after dismissal | pending |
| B36 | P1 | Processing failure fixture | [Processing error](../batch.md#finishing) | Source/settings that fail a processing stage | Start batch | Error is visible; queue unlocks; record whether prior writes lack per-row report | pending |

Source reviewed: `vector-magic-re` commit `e794628c8ea3fbcfd4903e55c95ac29894846715`. No runtime checks executed by this author.

## Static review findings

The four findings from the initial source review have fixes:

| Finding | Current behavior | Automated evidence |
| --- | --- | --- |
| Destination changed when adding sources | GUI additions preserve the selected folder | GUI input test |
| Batch replaced manual edits without warning | Starting asks before replacing an edited single-image document | GUI input test |
| Case-only stems collided | Planner reserves case-insensitive stems and adds numeric suffixes | Batch queue reference comparison and real-filesystem runner test |
| Late targets were overwritten | Exclusive creation preserves targets without overwrite admission | Runner test checks sentinel bytes and failed-save result |

The Linux and Windows queue and runner suites each passed all eight tests. Historical captures
remain unchanged; the tests explicitly account for the deliberate safety changes
in the [current contract](../../gui-usability-contract.md). These checks do not
certify native chooser behavior or the packaged batch GUI.

## Runtime questions

All B01-B36 results remain pending. In particular, confirm Windows filesystem
case behavior, visible replacement feedback, edited-document state after failure,
native dialog focus, cancellation timing and permission-error recovery. Static
inspection does not establish their packaged-GUI outcomes.
