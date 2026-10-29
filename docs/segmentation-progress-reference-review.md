# Segmentation interruption reference

The segmentation reference now has 688 cancellation/retry cases: 441 cold or
same-sized retained-label cases, 22 after completed fitting, and 225 on a new
owned 9x9 gradient. The unchanged protocol-3 observer ran 1,424 fresh native
processes. Its cold responses match 1,324 state comparisons against protocol 2;
eight completed gradient baselines also match between protocols.

The production engine has not changed in this phase. Before adding callbacks,
the new gradient's four distinct settings/image combinations pass exact
completed document comparisons in the workspace and an installed package.
The installed check actually denies original/application-source reads and
child creation. The prior package remains byte-identical to the current source.

## Facts that determine the implementation

The first segmentation callback precedes label initialization. New allocations
are unavailable, while initialized same-sized storage retains its prior map.
The native producer 0x4891a0 writes one pixel index per entry; return 0x4aab3b
establishes availability. The reference records this producer rather than
inspecting values or zeroing allocator residue.

Label storage, the live palette, scratch optimizer regions and exported region
records have separate lifetimes. Cold exported counts/color IDs remain empty
through most progress callbacks, even though the optimizer already has regions.
The palette can change before phase-6 region transfer. Warm segmentation can
keep the previous exported records through earlier callbacks.

After fitting, the observer reports retained node/cubic allocation counts.
However, its unchanged action-1 gate omits contour serialization. Counts do not
prove a valid graph or initialized contents. No retained-coordinate or stale-edge
claim follows from those counts, and this phase does not remove that gate.

The read-only document continuation assembly at 0x46e04c shows dependent-key
cleanup after normal or cancelled stage unwinding. Segmentation-edit, smoothing
and fitting keys clear before the cancellation check. Cancellation sets progress
100, clears the segmentation key and returns 2 without committing settings.
First-chance snapshots still expose the preceding dependent keys.

The existing RegionOptimizerState snapshot calls labels(), which resolves and
compresses parent links. An observer must not use that mutating accessor as a
substitute for the raw label writer. Nor can scratch optimizer counts substitute
for the document's exported region records. These constraints require a retained
owner with explicit producer boundaries before adding document progress hooks.

## Review and test ownership

Applied ce-simplify-code's reuse, quality and efficiency rubrics inline. No
independent reviewers or subagents ran. The one added test belongs to the existing
document-segmentation integration suite and reuses its captured-settings reader.
It supplies declared pixel/settings inputs to the independent engine; native
outputs supply assertions only. The quality pass changed deduplication to include
dimensions and pixels as well as settings. Reuse 0, quality 1, efficiency 0.

The consolidate-test-suites disposition is PROMOTE for the three frozen native
fixtures and repeatable recorder/auditor, and MERGE for the new baseline test.
The installed harness is retained as package/independence verification evidence.
No production probe or unreachable source element was added. No deletion is
pending. JJ remains unavailable; no VCS metadata, commits or pushes were created.

Syntax and scoped tests cover the test/evidence change. No root lint/typecheck
script is configured. A second full numerical or GUI run is unnecessary because
every production/package byte remains unchanged.

Segmentation callbacks and cancellation are still unimplemented. Palette-analysis
handoffs, resized retained storage, editing/shadow cases, old graph availability,
broader original GUI/scheduler behavior, the three known failures and full parity
remain open. Publication and Rust/GPUI work remain pending.
