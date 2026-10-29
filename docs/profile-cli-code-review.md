# Profile CLI and retained-document review

Scope: the new profile CLI handler/help and tests; the retained-profile document
oracle, shared decoder extension and differential tests. GUI product source and
numerical engine implementation files are unchanged.

The create-cli skill and its complete CLI guidelines were applied. The existing
Node parseArgs, JSON/stdin conventions, exit codes and package manager remain
canonical. No prompting or implicit settings path was added.

The ce-simplify-code context script completed with CE_CONTEXT_END. Its three
actual rubric assets were read. Review ran inline because delegation is
prohibited. This is one operator's reuse, quality and efficiency review, without
independent reviewers.

## Reuse

No additional finding. CLI operations use SavedAdvancedProfiles and the existing
atomic storage writer. The CLI only validates its own syntax and overwrite
decision; the shared component validates settings. Native document workflows use
the established batch/document wire parser and one extracted profile-frame
decoder, preserving strict original-header and full-record checks.

## Quality

No additional finding. Help resolution explicitly checks own dictionary keys,
including unknown prototype-shaped names. JSON channels serialize as arrays and
can feed save input. Read-only commands do not write files. Errors emit no partial
JSON, and overwrites require an explicit flag. The separate oracle executable
preserves the accepted profile-storage binary; its new include file has a distinct
ownership/build purpose despite its small size.

The native tests keep selected GUI settings separate from committed document
settings and assert the original commit ordering. They do not claim that an
unwired GUI control is implemented.

## Efficiency

No additional finding. One owned list snapshot serves each CLI command. The list
is bounded at 200 entries; there is no daemon, polling, lockfile or second storage
format. The native extra-frame callback runs only when an extension reader is
explicitly supplied. All old records retain their original strict decoding.

Applied findings: reuse 0, quality 0, efficiency 0. No findings were skipped and
no new dead code was identified. Moved decoder code has one canonical definition.
No lint or typecheck script is configured for this JavaScript project.

Six real CLI tests pass, alongside the existing CLI help/error checks selected
by name. Four native document comparisons pass through ten complete states and
1,000 typed parameter records. See the verification checkpoint for fresh-package
checks, exact test scope and remaining application gaps.
