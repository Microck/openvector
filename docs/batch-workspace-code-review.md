# Batch workspace code review

The settled scope is `src/batch-runner.mjs`, the GUI engine integration,
`gui/batch-controller.mjs`, `gui/server.mjs`, `gui/engine-worker.mjs`,
`gui/native-paths.mjs`, `gui/native-picker.ps1`, and the new batch React workspace
and shared request/control seams. Historical reference probes are outside scope.

The ce-simplify-code context script completed during this work. Its three actual
reuse, quality and efficiency rubric files were read. All three passes ran inline
because delegation is prohibited for this task. This is a single-operator review,
not three independent reviews. The graph index was ready, but returned no nodes
for the new batch symbols; direct reads of the known changed files supplied the
missing source.

## Reuse

No additional finding. The GUI uses the existing BatchQueue, BatchRunner,
ImageDocument, configuration reader, decoders, writers and wizard controls. Both
workspaces share one HTTP request helper and one engine document. The Windows
helper represents a separate host boundary; its complete COM method declarations
are required vtable slots, including declarations that are never called.

## Quality

One applied correction: derive the batch workspace's busy state as a boolean.
Previously an outstanding command name could reach aria-busy as an invalid value.
Control disabling and command order remain the same. Preserve named controls,
page-heading focus, default Cancel focus and all trust-boundary checks.

Separate feature correctness fixes made before final verification copy the
returned settings and retained geometry, join result labels by exact filename,
and use the completed job's export folder. Those fixes are not represented as
behavior-preserving simplifications.

## Efficiency

One applied improvement: publish thumbnail pixels once when files are added.
Progress snapshots carry metadata and content IDs. The host serves immutable
pixel assets separately and evicts removed IDs. Both producer and consumer are
inside this new, unshipped workspace; no old protocol adapter remains. Queue and
processing results are unchanged, and real HTTP/controller checks verify pixel
delivery and that progress events contain no pixel payloads.

No additional worthwhile simplification was identified. Serial engine work
preserves the document/random-state contract. No dead code was identified.

## Verification

- 171 GUI tests pass in the isolated build copy; Vite production build passes.
- 97 related tests pass: 79 shared source tests and the 18 GUI batch tests.
- Twelve retained paired native workflows are re-audited, including their raw
  responses, outputs and original-installation/settings guards.
- Browser actions verify overwrite Cancel, retry, two successful SVG writes,
  one invalid input, summary and restoration after reload. SVG bytes match the
  retained native reference.
- No lint or typecheck script is configured. This is JavaScript/JSX.
- The earlier full root suite's Adam7 and edited-alpha failures remain visible.
- Native picker/reveal UI interactions remain unverified. COM creation, inherited
  vtable calls and Unicode filesystem-folder roundtrips pass without opening UI.

Applied counts: reuse 0, quality 1, efficiency 1. No further findings were skipped.
See `evidence/gui-batch-workspace-verification-checkpoint.json` for hashes and scope.
