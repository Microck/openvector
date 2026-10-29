# Test fixtures

The native differential corpus is about 2 GB. The source checkout includes small
fixtures and the inputs needed by `npm run test:smoke`, including the compile-time
GUI artwork fixture. Ninety-one large capture files are excluded from Git; some
exceed GitHub's per-file limit. Their exact paths, sizes and SHA-256 digests are in
[the fixture manifest](fixtures-manifest.json).

The complete corpus stays in the owner's local investigation workspace. It is not
uploaded to Git, Git LFS, or GitHub Releases. Full differential suites require that
local corpus. Missing fixtures are errors, not skipped tests or substituted
expectations. `npm run test:smoke` does not need
those omitted files.

Copy the exact large captures into `test/fixtures/` before running the complete
Node suite. Rust differential suites can also use `OPENVECTOR_FIXTURES` to name a
complete fixture directory. The GUI's compile-time artwork input stays checked in
at its original path and does not use that environment variable.

No original application executable or installed profile is needed to replay these
fixtures. Never regenerate an expected result merely to make a mismatch pass.
