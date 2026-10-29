# Export file state and worker exception boundary

This phase adds 57 direct document save cases, repeated in two fresh processes
per group, using the existing OVSX/3 helper. No new native helper, production
source, existing suite, package or browser host changed. The live contract is
`export-interruption-contract.json`; the preceding checkpoint retains its earlier
contract hash and all original observations.

## Same-target retries

Each of six formats completes a baseline, cancels at its first callback, retries
that filename, then cancels at its last callback and retries that filename. The
input is the same explicit 64x64 opaque grid, stacked layout and thin stroke.
All retry method calls return 0. That result does not prove output validity.

SVG, PDF, EPS, AI and DXF expose complete bytes after successful retry and retain
baseline bytes after process exit. For EMF, both cancelled targets and their
successful retry calls remain unreadable through the observer, with CRT errno
13. After exit, the first target contains zero bytes and the late target contains
the earlier 16,376-byte unfinished EMF. Neither is the 18,220-byte baseline.
Unreadable bytes at return remain unknown. The evidence does not assign them an
empty payload or claim a specific Win32 sharing error.

The post-exit map contains one file per target. It is not duplicated as if every
step had independently produced those final bytes. Per-step observations come
from the native helper before the next save. The fixture preserves that ordering.

## File-open and extension results

For each format, a missing parent directory, a directory target and an existing
read-only file exercise failed opens, followed by a valid save. SVG, PDF, EPS and
AI return 4 without traversal or exceptions. DXF and EMF return 0, traverse all
16 callbacks and end at progress 91. The absent files stay absent, directory
targets stay directories and read-only marker bytes remain intact. Every valid
save afterwards succeeds. Source, working dimensions, PRNG, keys, settings bits,
exposed palette bits and reserved geometry container counts remain unchanged.

Unsupported suffix, no suffix and uppercase .SVG each return 1 without callbacks
or output through this method. A GUI picker admitting such an extension has not
been established. OpenVector's supported-format admission should not be widened
from a direct filename observation alone.

Twenty-two readable completed snapshots at actual return match bytes computed
from the independent engine: six baselines, ten text-format retries and six valid
saves after errors. The comparison excludes unreadable EMF retries and native
success returns that have no output. This is evidence about completed bytes, not
an assertion that the independent filesystem path already matches.

The current BatchRunner treats every open failure as result 4 and stops before
traversal. DXF therefore has a concrete direct-method mismatch. Applying a broad
"success on error" rule would be wrong for the other formats and could conceal
unverified write/close errors. The canonical file writer needs format-specific
native return behavior and real write ordering, with the GUI/controller boundary
verified separately. No production workaround was introduced here.

## Worker EH boundary

The retained REA dossier reports SaveFileThread::run at 0x436c80 with its body
ending at 0x436cff. LLVM disassembly finds a handler at 0x436d00, outside that
reported body, which writes worker result 3. The original x86 MSVC 7 FuncInfo
and handler tables identify its catch as std::bad_alloc. Cancellation's throw
table contains CancelTaskException and exception, neither matching that catch.
Result 3 is not established as a cancellation outcome.

The initial pseudocode read suggested no local handler. Assembly then exposed
the handler, and typed EH metadata narrowed its scope. Both revisions are kept
explicit rather than changing an expected cancellation status to fit a guess.
The recorder reads the seven-word MSVC 7 FuncInfo; later adjacent bytes must not
be interpreted as fields of a newer compiler's structure. Catch matching uses
the recorded type names rather than assuming descriptor addresses are unique.

The worker calls original admission 0x43a620 before the document method. That
check is preserved. No readiness/license state was patched, no worker started
and no GUI exception outcome inferred. The current REA process was not running;
the retained REA dossier supplied the entry and boundary, and read-only LLVM
disassembly plus physical original bytes supply the new EH evidence.

## Review, coverage and verification

The invariant is the relationship between method result, retained document state
and observable file state across interruption, failures and retry. Its primary
owning layer is integration. PROMOTE the two native traces, fixture, physical EH
reader, completed-byte comparator and auditor as reference evidence. Eventual
independent behavior belongs in the existing batch-cancellation/vector-output
suites, not a new standalone smoke suite. No newly unreachable production code
or unpromoted temporary runtime probe remains.

The ce-simplify-code rubrics ran inline under the user's no-delegation rule.
Reuse applied 0: existing encoders, parsers, clock validation and protection
utilities already serve the new scripts. Quality applied 1: match catch types by
their actual names. Efficiency applied 0, skipped 0, independent reviewers 0.
The two traces deliberately preserve duplicate-target ordering instead of
collapsing it into a map of inferred per-step outputs.

The auditor reparses requests and responses, validates physical input words,
result/exception states, successful retry bytes, failed-path observations,
post-exit target files and the compressed fixture. It re-reads original EH tables
and imports, checks assembly hashes, retains the prior reference ledger and
verifies unchanged runtime/source and test hashes. Python parsing and Node syntax
checks are fresh. No root lint/typecheck exists. Full runtime tests and package
verification are explicitly reused from the preceding unchanged-source phase;
no new full-suite result is claimed and the three known failures remain open.

There are four fresh native helper processes and 114 save calls in this phase.
Original installation, saved settings, desktop, accepted helpers, prior captures,
root gui/dist and user preview channels remain protected. Post-open errors,
actual worker cancellation, native cleanup, arbitrary-input parity, publication
and the Rust/GPUI port remain unfinished.
