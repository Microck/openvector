# Profile service review

Scope: `gui/profile-controller.mjs`, its worker and HTTP integration, and
`gui/test/profile-service.test.mjs`.

The ce-simplify-code reuse, quality and efficiency reviews ran inline under the
standing restriction on delegation. They are three review passes, not independent
corroboration. The context script completed with `CE_CONTEXT_END`; its Bash
launcher was adapted to the installed Windows Node runtime.

The service reuses SavedAdvancedProfiles for storage and ordering, GuiEngine for
single-image application, and BatchController for its native advanced-only copy.
It owns no alternate processing path or second profile list. Existing host
serialization, loopback binding, Host/Origin checks and worker error propagation
remain the transport boundary.

No worthwhile behavior-preserving simplification was found. The list is bounded
at 200 entries and is read on explicit commands. Snapshot copies preserve buffer
ownership. Dependent writes remain serialized; startup/listing creates no files.
No new unused product elements were identified.

Applied findings: reuse 0, quality 0, efficiency 0. Skipped findings: 0.

The fresh GUI build passes 236 tests. A fresh npm install passes 22 direct owner
tests with actual main-process original/workspace-read and child-process denials,
plus three separate HTTP/worker tests. The original engine archive remains
unchanged. No worker permission inheritance is claimed.

An earlier source-tree GUI run passed 233 of 234 tests. Its one failure expected
OpenVector in the deliberately preserved old `gui/dist` build. The fresh build
passes that same check. No assertion or test was weakened. Two harness print
steps then hit Windows CP1252 encoding errors after writing complete UTF-8 passing
logs; the separate installed checks and checkpoint verifier exit successfully.

There are no configured lint or typecheck scripts for this JavaScript/JSX project.
The full-root Adam7 and edited-alpha failures from earlier work remain unresolved.
Visible profile controls and native replay scheduling remain unfinished.
