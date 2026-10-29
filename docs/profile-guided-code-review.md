# Guided replay conformance review

Scope: the guided native recorder, native-driven engine/service harness and
browser projection checks. No product implementation changed. The package
differs from the preceding archive only in its README coverage note.

The `ce-simplify-code` context script and three shipped rubrics ran inline.
Delegation was not authorized. Independent contexts: 0. Applied findings:
reuse 0, quality 0, efficiency 1; skipped 0. The harness now takes one complete
engine snapshot per Next decision instead of cloning the document twice.
No new unreachable elements were identified.

The harness uses real engine, profile service, timer and document operations.
Its orchestration encodes the current page policy; it does not execute React.
The browser separately exercises the actual controls and timers through a full
forward flow and cached replay. These are complementary scopes, not independent
implementations of the numerical algorithms.

Ten available workflows cover default/fixed/flexible palettes, source alpha
flattened to white, cached replay, dirty segmentation/smoothing Update presses,
and Start/Reset from unmodified editing/background pages. The engine/service
checks compare 62 complete documents and 6,200 typed parameter records, together
with selected settings, palette runtime, names and page state. Twenty-two native
processes record eleven cases. The extra case invokes Play directly while Export
is current; the native slot delegates to current-page Next and remains on Export.
That hidden-control observation is kept separate from the ten GUI workflows.
No tests are skipped.

The native corpus retains 69 documents and 44 joined task observations, including
4,400 task parameter records. Intermediate worker records are retained evidence;
they are not all independently compared by the GUI harness. Complete document
comparisons occur after task groups.

Validation: frozen GUI 280/280, installed direct tests 247/247, separate HTTP
workers 6/6, actual main-process capability denials, 15 browser UI checks and ten
browser-driven state projection comparisons. The browser projection reader first
mistook Float32 scores for Float64; the failure report and corrected reader remain
retained. No product assertion or native expectation was weakened. The frontend
source, dependency locks and built files match the preceding verified build, so
its bundle is reused. No rebuild or unchanged CLI-suite rerun was needed. There
is no configured lint/typecheck command in this JavaScript/JSX project.

The prior two native-memory-dependent root failures remain unresolved. Backward
navigation, confirmations, resets after changed pixels/history, broader control
combinations and full native modal behavior remain outside this verified scope.
The evidence establishes progress toward parity, not completion.
