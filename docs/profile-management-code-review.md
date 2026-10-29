# Profile management review

Scope: the new `SavedAdvancedProfiles.rename`/`remove` methods, profile CLI
commands, worker service branches, advanced Export controls and their focused
tests. Native helper composition was also checked against retained REA and LLVM
evidence. Existing numerical kernels, original installation and settings,
previous artifacts and running user previews were outside the mutation scope.

`ce-simplify-code` ran its Windows Node context script and all three shipped
review rubrics in this context. Delegation was not authorized. There were three
review passes and zero independent reviewer contexts. No behavior-preserving
simplification was needed: reuse 0, quality 0, efficiency 0, skipped 0.

The list remains the only owner of names, order and profiles. Rename mutates
that entry directly rather than calling save, which would reorder entries and
replace settings under the built-in label. The GUI keeps its existing command
queue and shared settings-name draft. Dialog state represents the pending user
decision; DOM refs restore focus and do not mirror application state. Validation
stays at the public boundary, with no processing-loop checks or additional
numerical passes. No new unreachable elements were identified. The previously
approved unused compiler smoke test remains absent.

Browser verification found a separate accessibility defect: cancelling returned
focus to BODY. The final build restores the clicked control on cancel and the
stable Saved settings heading after mutation. The failing observation and
provisional build remain retained. Final browser checks verify the repair.

Validation: final frozen GUI 269/269; installed direct tests 235/235; separate
HTTP worker tests 6/6; installed profile CLI 8/8; source CLI suite 165/165;
installed browser 13 checks, no console warnings/errors. The project has no
configured lint or typecheck command; it uses JavaScript/JSX. The prior complete
root run still has two unresolved native-memory-dependent mismatches. This work
changes no numerical kernel, fixture expectations or those failing tests.

Reference limit: six cases, twelve serial native processes, twelve complete
document snapshots and 1,200 typed parameter records compare original
post-dialog list primitives with prepared decisions. Static full handlers
establish the branch order. This does not verify execution of native dialogs,
their keyboard focus/default buttons, or complete application parity.
