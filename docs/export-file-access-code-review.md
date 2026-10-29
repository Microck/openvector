# Export file access code review

Scope: Windows share modes for vector file saves, EMF file ownership, and the
missing `test/fixtures/export-file-access-reference.ndjson.gz`.

## Problem

Node's `fs` opens every Windows file with read, write and delete sharing. The
original exporters do not. Native probes after each save show:

- Cancelled text saves (SVG, PDF, EPS, AI, DXF) allow generic read and write
  opens but deny DELETE with error 32.
- Cancelled EMF saves deny read, write and delete with error 32. Attribute
  opens succeed. A same-target EMF retry returns 0, writes nothing and leaves
  the retained handle and size unchanged.
- Retained handles have granted access `0x120196`.

The earlier Node writer allowed all four probes
(`evidence/export-independent-file-access-verification.json`).

## Change

- `tools/file-win32.c`: a Node-API module that opens with `CreateFileW`,
  `CREATE_ALWAYS`, `GENERIC_WRITE | FILE_READ_ATTRIBUTES` and a caller share
  mode, then writes with `WriteFile` at the handle pointer or at an explicit
  offset, and closes. No C runtime: it imports only `KERNEL32.dll` and
  `node.exe`. Win32 errors map to libuv-style codes (`ENOENT`, `EPERM`,
  `EBUSY`, `ENOSPC`, `ENAMETOOLONG`, otherwise `EIO`) with `winError` kept.
- `tools/build-file-win32.py`: cross-builds `src/native/file-win32-x64.node`
  on Linux with mingw-w64, asserts the import list and records
  `evidence/file-win32-build-verification.json`. Node-API headers are vendored
  unmodified in `tools/node-api` from node-v24.12.0.
- `src/vector-file.mjs`: `writeVectorTextFile` became `writeVectorFile`. One
  `openOutput(filename, share)` platform layer feeds the existing text buffer
  and a forwarding EMF stream. Text shares read/write; EMF shares nothing.
  EMF uses the existing `serializeEmf` positioned writes. EMF, like DXF,
  traverses and returns 0 when its open fails. Interrupted EMF handles are
  retained without an exit flush; interrupted text streams still drain at exit.
- `evidence/build-export-file-access-reference.py`: derives the fixture from
  the three native captures, dropping only per-process handle values, and
  refuses to write if the two fresh repeats differ otherwise.
- `test/vector-output.test.mjs`: the ownership test now includes EMF across
  the interruption (390 complete, 270 cancelled), resource, same-target and
  file-error corpora. A Windows-only test runs the original probe script from
  a separate PowerShell process after each of 87 saves and compares 348 probe
  results. The child helper gained the probe and EMF error fixtures.

## Alternatives rejected

- libuv `UV_FS_O_EXLOCK` gives share mode 0 only. It fits EMF but cannot give
  text files read/write without delete, so it would split the owner in two.
- Converting a native handle to a Node descriptor with `_open_osfhandle`
  fails because `node.exe` links its C runtime statically; the module's
  descriptor table would not be Node's.
- A PowerShell or helper-process file server, or emulated access errors,
  would fake the measurement.

## Verification

- Windows, Node 24.12.0: full root suite 936/938. The two failures are the
  documented pre-existing ones (Adam7 scratch, `alpha-premultiplied`).
  Logs: `evidence/export-file-access-windows-full-core-tests.txt`,
  `evidence/export-file-access-windows-focused-tests.txt`,
  `evidence/export-file-access-windows-cli-emf-tests.txt`.
- Installed package on Windows: `evidence/export-file-access-package.json`
  lists the module. `evidence/export-file-access-installed-consumer.txt`
  shows the packed CLI batch save through it.
- Linux (aarch64 Raft box, non-root): the ownership test passes and the
  sharing test skips. As root, read-only fixtures cannot fail to open, which
  is an environment property, not a product one.

## Known limits

- Only win32-x64 is built. Windows ARM64 fails at module load with a clear
  error.
- EMF write failures after a successful open return 4 by analogy with text.
  The original behavior there is unmeasured.
- Retained-handle access masks are not re-read from the independent process.
- `evidence/observe-independent-export-file-access.mjs` still imports the old
  `writeVectorTextFile` name. It is frozen evidence for the earlier writer.

## Linux portability pass

The root suite had only ever run on Windows. On Linux aarch64 it failed 119
tests for two pre-existing reasons, both now fixed at the root:

- `src/batch-queue.mjs` derived the batch export directory with host path
  rules. Native `QFileInfo::absolutePath` applies Windows rules everywhere, so
  it now uses `path.win32`. POSIX absolute paths keep working because they
  contain no drive or backslash. `test/helpers/native-batch.mjs` maps captured
  Windows separators to host ones and probes whether the temporary filesystem
  is case-sensitive, since NTFS case aliasing is a filesystem property.
- x86 invalid operations produce the indefinite NaN with the sign bit set;
  AArch64 clears it. `src/automatic-colors.mjs` (palette feature ratios) and
  `src/image-classification.mjs` (edge profile scaling) now emit the x86
  pattern for generated NaNs. On x86 this is the value already produced.

Results: Linux aarch64 non-root 935/938 with the Windows sharing test skipped;
Windows 936/938. The two failures on both are the documented pre-existing ones.
