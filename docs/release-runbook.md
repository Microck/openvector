# Release runbook

OpenVector ships GitHub application releases and an npm CLI/Node library.
Publishing either requires a verified candidate and explicit release authorization.

The first release, `v0.0.0`, retains the version embedded in the verified desktop
builds. Its Windows x64 ZIP preserves all 17 files from the tested package; its
Linux ARM64 archive preserves the tested bytes. `build-provenance.json` records
binary hashes and source correspondence. Documentation and repository presentation
changes do not require rebuilding identical application source. Future product
changes need a new version and fresh build/runtime evidence.

## v0.1.0 candidate matrix

Every row needs a fresh build, archive extraction check and native application
workflow check before it is advertised as supported.

| Target | Downloads |
| --- | --- |
| macOS Apple Silicon | Developer ID-signed and notarized `.dmg` |
| macOS Intel | Developer ID-signed and notarized `.dmg` |
| Linux amd64 | `.tar.gz` with executable, desktop entry and notices |
| Linux arm64 | `.tar.gz` with executable, desktop entry and notices |
| Windows x64 | `Setup.exe` installer and portable `.zip` |

Test Windows 10 and Windows 11 separately. A Windows Server build or an x64
cross-build does not establish desktop support. Record the tested Linux
distribution, architecture and required GLIBC version rather than claiming
compatibility with every distribution. Portable means no installation is
required; preferences still use the normal per-user location.

Windows uninstall must handle busy files without reporting success or removing
the uninstall registration. Retry deletion for up to two seconds, then offer
Retry/Cancel. Silent uninstall must return a failure code if deletion stays
blocked. Check this with a real file lock before the installed application check,
then verify normal uninstall immediately after the application exits. Preserve
unrelated files in the installation directory.

Publish matching source and rebuild instructions alongside the binaries.
Preserve the Qt-derived module, its LGPL license and modification notices.
Verify that changing that module and rebuilding produces a working executable;
notices alone are not a substitute for source and relinking materials.

Developer ID signing requires a Developer ID Application certificate and its
private key. An Apple Development certificate is not a distribution identity.
Do not describe ad-hoc signed macOS packages as signed for public distribution.

## Prepare a version

1. Select an unused semantic version. Update `package.json`, `package-lock.json`,
   `rust/Cargo.toml` workspace version, and the workspace package entries in
   `rust/Cargo.lock` together. Update versioned examples and release notes.
2. Start from a clean source checkout and record the full commit ID. Never move a
   published tag or replace published assets.
3. Read [known limits](verification.md), the relevant component contracts, and the
   third-party distribution requirements in [building](building.md). Do not describe
   excluded comparisons or unexecuted targets as passing.
4. Restore and verify the [test corpus](test-fixtures.md). Use isolated state and
   matching native hosts for desktop tests. Heavy builds and full suites belong in
   a disposable build workspace with an explicit cleanup step.

## Validate the candidate

Use [GitHub Actions source CI](ci.md) for the lightweight repository checks.
It runs on GitHub-hosted Ubuntu runners. Source CI does not replace the native
and desktop checks below.

Use the locked dependencies and the target toolchain's `rust-docs` component:

```sh
npm ci
npm run test:smoke
npm run build
npm test
cargo test --manifest-path rust/Cargo.toml --locked -p openvector -p openvector-cli
cargo test --manifest-path rust/Cargo.toml --locked -p openvector-gui --lib
```

Record actual failures and skips. The known strict-suite exceptions remain visible;
these commands are not an assertion that every test currently passes. Stop if a
new failure appears, and investigate it before publication. A release decision must
name any accepted known limits explicitly.

Exercise the built application on each advertised platform: Open an image, convert
it, edit a palette, save and reopen the output, and run the changed workflow. Use
`OPENVECTOR_PREFERENCES` and disposable output directories. A cross-build alone is
not native execution evidence. Use a real graphical session for GUI checks.

## Package

The native CLI/addon and desktop app have separate packagers:

```sh
# CLI/addon for the build host. prepack performs the release build.
npm pack

# Desktop example. Set up this target's compiler and linker first.
mkdir -p dist
node tools/package-gui.mjs --target x86_64-pc-windows-gnu dist/openvector-windows-x64
```

For another CLI/addon target, use `CARGO_BUILD_TARGET` or
`npm run build -- --target TARGET`. Start each target from clean `native/` staging
so an archive cannot accidentally contain older binaries. See [building](building.md)
for supported notice collectors and the Windows Node-API linker requirements.

The GUI destination must be new. Archive the entire staged directory, including
`LICENSE` and `licenses/`, rather than only the executable. Keep the editable source
and exact build inputs available with the binary when required by its licenses.
`tools/package-windows.mjs` creates the Windows installer and portable ZIP from
a staged directory. Windows executables are not Authenticode-signed, and macOS
staging adds only an ad-hoc signature. Do not advertise public macOS distribution
signing until Developer ID signing and notarization have passed.

Before publishing:

- Inspect every archive member for credentials, local settings, original application
  files, caches, unintended targets, and missing notices.
- Calculate SHA-256 checksums from the actual archive bytes. Record the source
  commit, toolchain version, target triple, artifact digest, and native tests run.
- Extract each candidate into a fresh directory and run its CLI/addon or GUI there.
  Do not substitute a workspace binary for the packaged executable.
- Confirm the source archive includes both Cargo manifests/lockfile, vendored GPUI
  patches, bundled C sources/notices, required JSON tables, and the packaging tools.

## Publish an application release

Only after explicit release authorization, use the reviewed commit and notes.
Confirm `gh repo view Microck/openvector --json visibility` returns `PUBLIC`.
The application downloads and npm package have separate publication steps.

Create an annotated `vX.Y.Z` tag at the validated commit with author identity
`Microck <contact@micr.dev>`, push that tag, and create a normal GitHub release with
all selected archives and `SHA256SUMS.txt` attached. Use `--verify-tag`, an explicit
`--target` commit, `--notes-file`, and `--latest`. Never infer prerelease status from
an unreleased version number. There is no automatic publication workflow.

## Publish the npm package

Use `.github/workflows/npm-candidates.yml` on a `release/npm-` branch. It builds
Linux x64/arm64 and Windows GNU x64 binaries with their notices, then assembles
one `npm-package` artifact. The installed-package jobs must pass on all three
platforms with Node 22.19 and Node 24 before publication.

Download that exact tarball, inspect its members and record its SHA-512 integrity.
Publish the tarball with `npm publish <package.tgz> --access public`; do not run
`npm publish` from a host-only build directory. Never include npm credentials in
artifacts or workflow logs. Download the registry package after publication,
compare its integrity and run `npm run test:package -- <downloaded-package.tgz>`
on supported native targets. macOS remains excluded until its release QA passes.

## Verify and recover

Download the published assets into a fresh directory using `gh release download`.
Verify every checksum against the reviewed manifest, inspect the tag's commit, and
repeat the packaged installation/conversion smoke. Confirm repository visibility
is still public. An upload response alone is not completion evidence.

If publication partly fails, inspect the existing tag, release, and assets first.
Do not blindly retry, move a tag, delete a failed release, or overwrite an asset.
Complete only missing uploads whose reviewed bytes still match. A defective
published build needs a new version with a clear correction note.

## Reference corpus

Large reference captures stay local. Do not include them in source commits,
application assets, or Git LFS. Use `docs/fixtures-manifest.json` to identify and
verify the local files needed for a full differential run. Small checked-in
fixtures keep the source smoke checks self-contained.
