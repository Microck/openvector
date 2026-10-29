# Viewer zoom reference recovery

The owned scalar probe copies the original getter's 88 instruction bytes. It
replaces the receiver's three-byte slider-pointer read with NOPs and its six-byte
imported slider-value call with an owned integer argument read. Three absolute
constant addresses point to owned copies of the original binary64 constants.
Every arithmetic instruction remains unchanged. The original image is read-only.
The process initializes neither Qt nor the application.

Two scalar processes record all 301 slider positions under control word 0x027f.
Their 3,628-byte outputs match exactly. The auditor compares normalized executed
instruction bytes against the original function and verifies every substitution.
The values at positions 100, 233 and 192 match the separately captured native
widget getters bit for bit in both basic and advanced export panes. The original
37 reference helpers remain unchanged. The accepted scalar executable is frozen;
the recorder rejects rebuilding it and audits its source and executable hashes.

Plain JavaScript exp/log interpolation differs at 164 of 301 positions, by up to
eight binary64 representable steps. A geometric Math.pow expression differs at
209 positions, by up to seven steps. These results rule out claiming exact native
arithmetic from either convenient expression. No product controller or expected
test value was changed to hide those differences.

The ce-simplify-code rubrics ran inline, with zero independent reviewers. Reuse 0,
quality 1, efficiency 0, skipped 0. Unused imports were removed from the checkpoint
auditors. The scalar loader is deliberately separate
from the full reference application loader: it initializes no application and
executes only an owned numeric span. The recorder reuses the existing 37-helper
guard. No existing code became unreachable. The C compiler passes Wall, Wextra
and Werror; both raw outputs and all instruction substitutions pass audit.
There are no configured lint or typecheck scripts.

The consolidate-test-suites invariant is exact scalar reference execution over
the closed integer domain. The corpus and its auditor are promoted as durable
reference evidence, following existing native fixture conventions. No product
test duplicates that invariant yet because the runtime replacement is unfinished.
No temporary product instrumentation or standalone regression suite was added.

REA additionally recovers fit, 100 percent, scale application, image availability,
batch transitions, scene bounds, painting and the slider producer. The inverse
conversion truncates toward zero. Direct scale application and the integer slider
remain separate state. The bitmap From View handler uses the slider-derived
getter, which can differ from the displayed matrix scale after fit or 100 percent.
The slider producer also has a separate arithmetic span with a retained x87
intermediate. Its bits must be compared before reusing the getter calculation.

The new saved analysis contains 31 workflows and 109 evidence records. Closing
REA acknowledges a 2,127,987-byte snapshot; its process terminates normally. The
earlier bitmap analysis remains unchanged. The methods at 0x464dd0 through
0x464e30 were initially misclassified as cursor helpers. Import and meta-call
inspection now identifies them as horizontal/vertical scrollbar step actions.
The corrected disassembly and dispatch map are retained in
`evidence/viewer-pan-actions-assembly.txt` and `evidence/viewer-pan-dispatch.json`.

Dynamic inverse and slider-producer conformance, slider subclass plus/minus
behavior, viewport geometry, direct matrix measurements and lifecycle comparisons
remain pending. Product runtime implementation and differential verification are
unfinished. The native arithmetic corpus proves the bounded getter calculation;
it does not establish full viewer or application parity.
