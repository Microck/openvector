# Release runbook

OpenVector is private and unpublished. Creating this repository does not publish
an npm package or an application release. Keep `package.json` private and the
GitHub repository private unless a separate request explicitly changes that scope.

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
The current packager does not create installers, sign Windows executables, or
notarize macOS applications. Do not advertise those properties.

Before publishing:

- Inspect every archive member for credentials, local settings, original application
  files, caches, unintended targets, and missing notices.
- Calculate SHA-256 checksums from the actual archive bytes. Record the source
  commit, toolchain version, target triple, artifact digest, and native tests run.
- Extract each candidate into a fresh directory and run its CLI/addon or GUI there.
  Do not substitute a workspace binary for the packaged executable.
- Confirm the source archive includes both Cargo manifests/lockfile, vendored GPUI
  patches, bundled C sources/notices, required JSON tables, and the packaging tools.

## Publish a private application release

Only after explicit release authorization, use the reviewed commit and notes.
Confirm `gh repo view Microck/openvector --json visibility` returns `PRIVATE`.
Keep the package's npm `private` flag; this workflow uses GitHub assets, not npm.

Create an annotated `vX.Y.Z` tag at the validated commit with author identity
`Microck <contact@micr.dev>`, push that tag, and create a normal GitHub release with
all selected archives and `SHA256SUMS.txt` attached. Use `--verify-tag`, an explicit
`--target` commit, `--notes-file`, and `--latest`. Never infer prerelease status from
an unreleased version number. There is no automatic publication workflow.

## Verify and recover

Download the published assets into a fresh directory using `gh release download`.
Verify every checksum against the reviewed manifest, inspect the tag's commit, and
repeat the packaged installation/conversion smoke. Confirm repository visibility
is still private. An upload response alone is not completion evidence.

If publication partly fails, inspect the existing tag, release, and assets first.
Do not blindly retry, move a tag, delete a failed release, or overwrite an asset.
Complete only missing uploads whose reviewed bytes still match. A defective
published build needs a new version with a clear correction note.

## Reference corpus

Large reference captures stay local. Do not include them in source commits,
application assets, or Git LFS. Use `docs/fixtures-manifest.json` to identify and
verify the local files needed for a full differential run. Small checked-in
fixtures keep the source smoke checks self-contained.
