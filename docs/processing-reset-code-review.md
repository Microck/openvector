# Processing reset

The original source/model reset differs from Start Over. It initializes selected
scalar controls before clearing source state, commits zero RGBA background to
the blank document, then resets selected palette/transparency metadata. It keeps
the application PRNG and palette revert copy. Batch reset preserves advanced
controls, count and supplied palette while resetting basic/background fields.

The independent engine now establishes those scalar defaults and calls its
existing source reset. The batch owner clears queue records and processing
index, then changes only the observed basic/background fields. Neither operation
replaces the session, tracer or export path. Full window coordination and its
confirmation remain unfinished.

## Review

The ce-simplify-code reuse, quality and efficiency rubrics ran inline, with zero
independent review contexts. No additional simplification changes were applied.
There is no independent corroboration from these three passes. The existing
code graph still points to an older engine method; current known files supply
the implementation evidence.

- Reuse: the engine reuses canonical source reset and selected-profile defaults.
  Batch reset uses its existing queue, command admission and notification path.
- Quality: no new compatibility path, mode flag, fallback or persisted shape.
  Advanced batch fields remain owned by their existing profile. No existing
  production code became unreachable. The new engine operation awaits the
  unfinished global action; the batch command is exercised through its owner.
- Efficiency: no new decode, allocation of a CoreSession, startup work, scan or
  timer. Resets preserve the existing bounded state owners.

## Native evidence and unresolved comparison

Seven workflows repeat raw response bytes across 28 fresh processes. The corpus
captures 49 profile frames and ten documents. Phase-one preference restoration
remains evidence for an unfinished persistence owner. The current implementation
compares phase zero, with nine complete documents and 900 parameter records
matching across six workflows.

The seventh case invokes a batch model setter with transparency value 2. The
public processing contract rejects that value. Retained native menu evidence
shows values 0, 1 and 3, without 2. The comparison remains failing at admission;
it has not been skipped, clamped, passed through a second setter channel or
rewritten. Its current-state question is recorded in the processing-reset
contract. This turn does not claim complete model conformance.

A completed two-file batch verifies resetting a nonzero cursor and clearing
thumbnails without removing exported files. Its initial test setup used the
wrong status spelling and a one-file job, whose last zero-based cursor is zero.
Those failed logs remain retained. The corrected setup uses the actual complete
status and two files so the nonzero-cursor assertion is meaningful.

The native probe executable is unchanged. The recorder now accepts an explicit
case specification so both reset corpora use the same capture and guards.
Original files, settings and input desktop remain unchanged; the private
namespace is removed after each paired recording. No native UI input, warning
answer, instruction patch or post-confirmation window handler is executed.

No lint/typecheck scripts are configured. The isolated build succeeds and its
wider suite passes 342 of 343 tests, with only the setter-only transparency
comparison failing. All nine new checks run; none is skipped. Scoped results
pass eight of nine. Wider results and immutable verification identities are
recorded in the processing-reset checkpoint. The npm package's 64 file bytes
are unchanged because this implementation changes GUI owners only.
Publication, the Rust/GPUI port and complete parity remain unfinished.
