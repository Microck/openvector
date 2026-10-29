# GUI QA work record

Source: `/home/ubuntu/workspace/openvector/vector-magic-re`, baseline `e794628c8ea3fbcfd4903e55c95ac29894846715`.
Scope: inspect and fix the native GUI so users can finish open, convert, inspect,
edit and export, with useful icons and guidance. Source edits are explicitly
requested by the user and override the description skill's read-only default.
No paid resources or new release publication is authorized by this pass.
Use a separate Windows instance, preferences and disposable output directory.
Do not change the user's manual-test instance or original-application settings.

## Reading order

README.md, glossary.md, single-image/open.md, foundations/input.md,
foundations/document.md, foundations/navigation.md, single-image/conversion.md.
Then read the relevant complete source functions and tests in the source map.

## Writing rules

Describe the experience, not implementation. Technical details belong only in
Technical note block quotes when they change user expectations. Use sentence case
headings and direct prose. State surprising behavior and suspected bugs plainly.
Link to foundations instead of duplicating their definitions. Use glossary terms;
propose missing terms to the parent so parallel writers do not edit shared files.
Do not guess or mark runtime verification from code inspection. Follow README's
fixed skeleton, interrupt rows and cross-cutting paragraph order. Cite the source
commit in each document footer, distinguishing draft evidence from runtime proof.

## Established facts

- A successful load opens the mode page. Opening does not convert automatically.
- Fully automatic processing leads to Review unless Skip review is selected.
- Advanced processing leads through palette, segmentation, editing, smoothing,
  background and Export. Navigation waits for worker completion where required.
- A failed Open preserves the previous source and its result.
- Most editing commands are disabled while processing; Cancel requests worker cancellation.
- Export needs computed vectors; opening an image alone does not enable it.
- Preferences are separate from image/output files. Test preferences use OPENVECTOR_PREFERENCES.
- Shared thresholds belong to foundations/input.md, document state to
  foundations/document.md, page movement to foundations/navigation.md.
- Conversion owns processing and result completion. Editing owns edit transactions;
  export owns disk writes; inspection owns view-only changes; batch owns its queue.

## Work order

Pilot and foundations drafted by the parent first. Parallel reviewers own only
assigned feature documents/checklists. Parent owns glossary, coverage, triage,
implementation, runtime checks and final reconciliation.
