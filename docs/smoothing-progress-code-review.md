# Smoothing interruption

The independent smoother now publishes native phase-8 checkpoints from its
canonical initialization and optimizer. The document keeps the geometry, flags,
penalty counts and random draws produced before cancellation. Its direct and
task APIs return status 2, set progress 100, invalidate smoothing and fitting
keys, and keep the previous committed settings. Retry reconstructs raw contours
using the retained random stream.

The reference includes 388 cold cases, each recorded twice by the unchanged
protocol-3 helper. All 1,552 completed/first-chance state comparisons match the
previous protocol. A second fixture includes 16 warm cases after completed
fitting, followed by smoothing retry and refitting. Their 48 native processes
also repeat byte for byte. Native observations never supply numeric inputs to
the independent implementation.

The first warm hypothesis incorrectly expected cancellation to retain the old
fitting key after unwind. The failed assertion and raw captures remain in
`evidence/stage-cubics-smoothing-warm`. The actual first-chance snapshot retains
that key, while continuation 0x46e18b clears it before returning status 2. The
accepted warm captures use `stage-cubics-smoothing-warm-cleanup` and test this
ordering explicitly. No failed capture was normalized or overwritten.

## Ownership and ordering

- `contour-initialization.mjs` separates raw references and border classification
  from the antialias neighborhood-flag producer. The public preparation helper
  composes these same operations.
- `contour-smoothing.mjs` places both preparation callbacks before coordinate
  perturbation. The phase optimizer calls back after movement and repair, before
  convergence advances its iteration field. Phase endpoints advance the full
  budget even when convergence ends early. The two recovered progress tables
  retain their exact float32 values promoted to double.
  Read-only assembly recovery of 0x497c30 and 0x46f7f0 additionally establishes
  enabled-phase budgets and native int32 counter arithmetic. Disabled-mask
  progress and counter extremes still need direct progress captures. A focused
  test does verify that observing all eight phase masks preserves their native
  completed geometry, penalties and random state, without mutating inputs.
- `smoothing.mjs` keeps one optimization algorithm. Its internal iteration sink
  observes the live numeric state. Standalone calls without a sink avoid the
  per-iteration geometry copy.
- `palette.mjs` gives document smoothing the session's retained random stream.
  Standalone library smoothing keeps its existing transaction rollback behavior.
  Both use the same numeric implementation.
- `image-document.mjs` exposes owned public snapshots while retaining the live
  internal state. A host observer error propagates by identity, clears incomplete
  keys and permits retry. Its error name cannot impersonate cancellation.

## Simplification review

Applied the ce-simplify-code reuse, quality and efficiency rubrics inline. No
independent reviewers ran. The workspace has no VCS metadata and JJ is
unavailable, so the review scope is the changed files compared with the prior
document-fitting checkpoint.

Reuse: the preparation API composes the same raw-state and neighborhood
producers, and document and standalone smoothing share one optimizer. No further
reuse change was needed. Quality: kept the two ownership contracts explicit
instead of adding rollback flags or a second smoothing algorithm. Efficiency:
removed the newly added per-iteration geometry publication for standalone calls
that have no observer. Applied findings: reuse 0, quality 0, efficiency 1.

No source element became unreachable or unused. No configured root lint or
typecheck script exists; this scope uses JavaScript modules and Python evidence
tooling. Syntax checks and numerical/reference tests provide verification.

## Remaining scope

This establishes document-level behavior for the recorded inputs. Batch/worker
and GUI forwarding, browser cancellation/retry, installed package verification,
segmentation cancellation, wider original GUI/scheduler behavior and full
application parity remain unfinished. Existing known failing comparisons stay
visible. No GitHub/npm publication or Rust/GPUI completion is claimed.
