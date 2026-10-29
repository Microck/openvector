# Backward navigation conformance review

Scope: additions to the guided recorder and harness, frozen verification script,
and browser projection reader. Product runtime, frontend source and npm archive
bytes remain unchanged. No implementation fix was needed for these workflows.

The ce-simplify-code context script and all three shipped review rubrics ran
inline. Delegation was not authorized. Independent reviewer contexts: 0.
Applied findings: reuse 0, quality 0, efficiency 0. Skipped findings: 0.
The recorder reuses the established guarded capture/parser/audit, and the harness
uses existing control predicates and field lists. Its page policy deliberately
mirrors the application while real browser checks cover the React controls.
Moving test policy into production would reduce that separation.

Seven added workflows cover default, fixed and flexible palettes, white-flattened
alpha, cached replay after returning to Mode, and dirty segmentation/smoothing
Back. The first dirty Back restores committed controls on the current page; the
next Back moves to the preceding page. No modified pixel/history case is claimed.

Eighteen paired recordings use 36 original processes and retain 142 documents.
Seventeen available workflows compare 135 complete documents and 13,500 typed
parameter records. The remaining case is the explicit hidden Play-on-Export
observation. Intermediate task records remain retained observations, not extra
independent comparisons. Previous eleven fixture rows remain an exact prefix.

An initial 75-action cached replay request hit the existing dispatcher's 64-action
limit before constructing MainModel. Its empty response, request and failure
report remain retained. The compact replacement retains all six Back presses,
Play and full forward completion while avoiding duplicate observation commands.
The recorder now checks this unchanged limit before capture. No native binary,
instruction, licensing check, input event or dialog answer changed.

Validation: focused 18/18, frozen GUI 287/287 and installed direct 254/254 with
actual main-process capability denials. Eight browser UI checks and seven native
state projections pass. The prior six HTTP checks and CLI checks apply to the
identical runtime bytes and were not rerun. No lint or typecheck is configured
for this JavaScript/JSX project. No new unreachable elements were identified.

Full parity remains unproven. Modified editing/background history and reset
paths, native modal behavior, broader desktop features and two known
native-memory-dependent mismatches remain unresolved.
