# Release runbook

This procedure publishes desktop applications on GitHub and the CLI/Node library
on npm. These are separate publications. Neither candidate workflow publishes.
Obtain explicit authorization before pushing, merging, tagging, publishing, or
triggering a workflow. Documentation edits alone do not authorize those actions.

Use [CHANGELOG.md](../CHANGELOG.md) as the canonical release history. GitHub release
bodies use its versioned entries, followed by download instructions and the actual
verification results and coverage limits. Do not maintain a second release-history file.

## 1. Prepare the version and release record

Start from the default branch in a clean, task-owned checkout. Preserve unrelated
work. Use Jujutsu for workspace history and pushes; the corresponding-source
packager needs a Git checkout at the build commit, which CI supplies.

1. Choose an unused semantic version. Update `package.json`, `package-lock.json`,
   the workspace version in `rust/Cargo.toml`, and workspace package versions in
   `rust/Cargo.lock` together.
2. Add user-visible changes under `[Unreleased]` in the changelog. Use `Added`,
   `Changed`, `Deprecated`, `Removed`, `Fixed`, or `Security`; omit empty categories.
   Do not turn test, CI, or housekeeping commits into product changes.
3. Before the candidate build, promote those entries to `## [X.Y.Z] - YYYY-MM-DD`,
   using the planned UTC publication date. Leave an empty `[Unreleased]` above it
   and update the comparison links. Correct the date if publication is delayed.
4. Update the README badge, download table, archive directory examples, and release
   coverage link. Confirm every advertised filename matches the planned assets.
5. Record the version, candidate branch, full build commit, CI run IDs, intended
   targets, authorization, and accepted limits in a durable task record outside
   the distributable payload. Add actual commands, results, artifact hashes,
   signing receipts, publication state, and cleanup as the work proceeds.

The commands below use Bash. Set these in each host's release session, after
updating the version. Each host needs its own disposable work directory:

```bash
set -euo pipefail
REPO=Microck/openvector
VERSION=$(node -p 'JSON.parse(require("fs").readFileSync("package.json")).version')
TAG="v$VERSION"
WORKDIR=$(mktemp -d "${TMPDIR:-/tmp}/openvector-release.XXXXXX")
```

Inspect remote versions before building or publishing:

```bash
gh api --paginate "repos/$REPO/tags" --jq '.[].name'
gh release list --repo "$REPO" --limit 100
npm view openvector versions dist-tags --json \
  --registry=https://registry.npmjs.org --userconfig=/dev/null
```

These inventory requests must succeed. An authentication, TLS, or transport failure
does not prove absence. If the selected version already exists, inspect its tag,
release, assets, or registry entry before continuing. Never move a published tag,
overwrite assets, or republish an immutable npm version.

## 2. Build and validate one candidate

Restore and verify the [private test corpus](test-fixtures.md) only inside an
isolated build workspace. Keep original-application captures out of commits,
Git LFS, npm packages, and public source archives. Inspect
[component contracts and known limits](verification.md); dated historical
failures are not exemptions for a new release.

Run full builds and suites in a task-owned build workspace with sufficient disk,
memory, and lifetime. Record the host and cleanup plan. Use locked dependencies,
Rust 1.99.1 with `rust-docs`, and Node.js 22.19 or later:

```bash
npm ci --ignore-scripts
npm run test:smoke
npm run build
npm test
cargo test --manifest-path rust/Cargo.toml --locked -p openvector -p openvector-cli
cargo test --manifest-path rust/Cargo.toml --locked -p openvector-gui --lib
```

Use `--offline` for Cargo checks from the vendored corresponding-source archive.
Record failures and skips explicitly. Run locale-dependent ignored tests in an
isolated locale when applicable; an ignore is not a passing test. Fix new failures
without weakening comparisons. After a source change, rebuild affected artifacts
and rerun affected checks. Never reuse stale binaries.

The [source workflow](../.github/workflows/source.yml) runs
`bash tools/check-source.sh`. Its smoke and inventory checks do not replace full
suites, native execution, or manual desktop QA.

### Select the correct candidate workflows

| Workflow | Trigger | Required artifacts and checks |
| --- | --- | --- |
| [Desktop candidates](../.github/workflows/desktop-candidates.yml) | A PR whose head branch starts with `release/`, excluding `release/npm-` | `corresponding-source`, all selected `desktop-*` jobs, and Windows installation checks |
| [npm candidates](../.github/workflows/npm-candidates.yml) | A `release/npm-` PR, or manual dispatch at a selected ref | Three `native-*` builds, `npm-package`, and all six installed-package checks |

For a combined release PR such as `release/X.Y.Z`, set `CANDIDATE_REF` to that frozen
remote branch. Explicitly dispatch npm candidates there after authorization:

```bash
gh workflow run npm-candidates.yml --repo "$REPO" --ref "$CANDIDATE_REF"
```

Desktop candidates have no manual-dispatch trigger. Set `DESKTOP_RUN` and `NPM_RUN`
to the recorded run IDs and confirm both use the intended full build commit.
Branch names alone do not identify a build. Inspect and download each recorded run:

```bash
gh run view "$DESKTOP_RUN" --repo "$REPO" --json headSha,conclusion,jobs,url
gh run view "$NPM_RUN" --repo "$REPO" --json headSha,conclusion,jobs,url
gh run download "$DESKTOP_RUN" --repo "$REPO" --dir "$WORKDIR/desktop"
gh run download "$NPM_RUN" --repo "$REPO" --name npm-package --dir "$WORKDIR/npm"
```

Require successful relevant jobs, not merely downloadable artifacts: desktop
jobs upload diagnostics even when they fail. The npm matrix checks Linux x64,
Linux ARM64, and Windows x64 on Node 22.19 and 24.

## 3. Verify the packaged desktop applications

| Target | Public download |
| --- | --- |
| macOS Apple Silicon | Developer ID-signed, notarized, stapled `.dmg` |
| macOS Intel | Developer ID-signed, notarized, stapled `.dmg` |
| Linux amd64 | Complete `.tar.gz` |
| Linux ARM64 | Complete `.tar.gz` |
| Windows x64 | `Setup.exe` and complete portable `.zip` |

Extract each artifact into a fresh directory. Exercise that packaged executable,
not a workspace binary. Open an authored image, convert it, edit a palette, save,
reopen in a native reader, and exercise the changed workflow. Set
`OPENVECTOR_PREFERENCES` to a task-only JSON file and use disposable outputs.
Keep license files and the complete directory layout with the executable.
This isolates application settings, not OS-managed dialog history. Use a disposable
OS account or session when that history must remain untouched.

Record OS version, CPU, execution mode, input/output hashes, and the checks
actually performed. Label Rosetta, Windows x64 emulation, and WSL explicitly.
Test Windows 10 and Windows 11 separately. Record the tested Linux distribution
and GLIBC requirements. Wayland and full screen-reader support need their own
evidence; an X11 conversion or text-field accessibility check does not prove them.

The CI conversion checker is
`node tools/check-desktop-package.mjs <packaged-GUI> <matching-CLI>`.
It compares actual GUI and CLI output in isolated state, but does not perform
manual palette editing or native-reader reopening.

Windows installation tests need a disposable host with no existing OpenVector
uninstall registration. Verify custom-path installation, installed/portable byte
identity, real locked-file uninstall failure with installation and registration
preserved, then normal uninstall preserving unrelated files. Do not run lifecycle
tests against a daily installation. Windows downloads are not Authenticode-signed.

If a required check cannot run, stop or obtain explicit approval for a documented
coverage limit. Do not silently turn missing QA into a supported-platform claim.

### Sign and notarize macOS downloads

CI produces ad-hoc-signed candidate apps, not distribution-ready DMGs. Use those
CI apps on a matching Mac; do not rebuild while signing. A Developer ID Application
certificate with its private key and a configured Notary keychain profile are
required. Apple Development and ad-hoc identities are insufficient.

Work on copies in a task-owned directory. Set `APP` to the extracted app,
`DEVELOPER_ID` to the distribution identity, `NOTARY_PROFILE` to an existing
credential profile, and `ARCH` to `arm64` or `x64`. Keep secrets out of command
arguments and logs. Restore any keychain search-list changes and relock a
task-unlocked signing keychain afterward. Do not grant privacy permissions
programmatically; request the required capture/accessibility permission for QA.

Sign the app, archive it for notarization, and require an `Accepted` result:

```bash
codesign --force --options runtime --timestamp --sign "$DEVELOPER_ID" "$APP"
codesign --verify --deep --strict "$APP"
codesign --display --verbose=4 "$APP"
ditto -c -k --keepParent "$APP" "$WORKDIR/OpenVector-$ARCH.zip"
xcrun notarytool submit "$WORKDIR/OpenVector-$ARCH.zip" \
  --keychain-profile "$NOTARY_PROFILE" --wait --timeout 30m --output-format json \
  > "$WORKDIR/app-$ARCH-notary.json"
node -e 'if(require(process.argv[1]).status !== "Accepted") process.exit(1)' \
  "$WORKDIR/app-$ARCH-notary.json"
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
spctl --assess --type execute --verbose=2 "$APP"
```

Check the Developer ID authority chain, secure timestamp, and hardened runtime in
the signature display. Sign any nested code inside-out before signing the bundle;
do not use `--deep` as a substitute for signing each code object.

Create and sign a new DMG containing the stapled app and an Applications link.
Then notarize and staple the DMG itself:

```bash
DMG_ROOT="$WORKDIR/dmg-root-$ARCH"
DMG="$WORKDIR/openvector-$VERSION-macos-$ARCH.dmg"
mkdir "$DMG_ROOT"
ditto "$APP" "$DMG_ROOT/OpenVector.app"
ln -s /Applications "$DMG_ROOT/Applications"
hdiutil create -volname OpenVector -srcfolder "$DMG_ROOT" -format UDZO "$DMG"
codesign --force --timestamp --sign "$DEVELOPER_ID" "$DMG"
xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" \
  --wait --timeout 30m --output-format json > "$WORKDIR/dmg-$ARCH-notary.json"
node -e 'if(require(process.argv[1]).status !== "Accepted") process.exit(1)' \
  "$WORKDIR/dmg-$ARCH-notary.json"
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"
codesign --verify --strict "$DMG"
hdiutil verify "$DMG"
spctl --assess --type open --context context:primary-signature --verbose=2 "$DMG"
```

Mount each final DMG read-only at a task-owned mountpoint. Verify its app signature
and Gatekeeper assessment, compare it with the signed staging app, and repeat
conversion and manual save/reopen checks from the mounted or extracted app.
Detach only the task-owned mount. Retain app and DMG submission IDs and receipts.
Hash the final stapled DMGs, not the earlier candidates.

## 4. Verify corresponding source and assemble the payload

Inspect every archive for unsafe paths, credentials, preferences, private captures,
unintended targets, caches, and missing notices. The desktop corresponding-source
archive must record the build commit in `SOURCE-COMMIT`, include its tracked files
and locked vendored dependencies, and contain working `REBUILD.txt` instructions.
See [building and third-party distribution requirements](building.md).

In a disposable extracted source tree, modify the Qt-derived
`rust/crates/openvector/src/bitmap_raster/square_stroke.rs` component. Rebuild the
whole GUI and CLI with locked offline dependencies, execute the modified component,
and exercise the rebuilt application. Retain proof, then discard the proof
binaries. Never publish them in place of the reviewed candidate.

Create `BUILD-PROVENANCE.json` with the version, build commit/tree, CI run IDs,
toolchains, target triples, unsigned and final signed executable hashes, package
digests, notarization IDs, native QA results, skips, and accepted limits. GitHub
and npm artifacts must correspond to the same reviewed application source.

Set `PAYLOAD_DIR` to a new directory containing only the selected public downloads,
corresponding source, and provenance. Keep diagnostic CLIs, candidate Mac tarballs,
notarization ZIPs, logs, screenshots, and npm tarballs outside it. For all desktop
targets, expect six binary downloads, one source archive, provenance, and checksums.

## 5. Merge and publish GitHub downloads

Require review and relevant checks on the exact candidate head before merging.
Record the resulting full `RELEASE_COMMIT`. Compare its tree with the tested build
tree; do not assume a squash merge is identical.

Documentation-only differences may retain identical application binaries. Record
both commits/trees and the exact documentation delta in provenance. If application
source, dependencies, build scripts, or packaging inputs change, build and verify
a new candidate. Do not relabel old artifacts as a new build.

Generate the manifest after signing and stapling are complete:

```bash
(
  cd "$PAYLOAD_DIR"
  test ! -e SHA256SUMS.txt
  sha256sum -- openvector-* BUILD-PROVENANCE.json > SHA256SUMS.txt
  sha256sum --check SHA256SUMS.txt
)
```

`PAYLOAD_DIR` must be a fresh, reviewed directory without an existing checksum
file; the manifest lists every other payload file, including provenance. Use a
Linux signing-output collection host with `sha256sum`, not a partial Mac staging
directory. Preserve the reviewed manifest for public-download comparison.

Confirm the versioned changelog and README match the selected payload. Set
`RELEASE_BODY` to a reviewed Markdown file containing the changelog entries plus
actual downloads, evidence, limits, and source correspondence. Check the repository
and workspace before running any publishing commands:

```bash
gh repo view "$REPO" --json visibility
jj status
```

Stop if the workspace is dirty or visibility is not `PUBLIC`. Create an annotated
tag with the required identity at the explicit release commit:

```bash
GIT_DIR=$(jj git root)
git --git-dir="$GIT_DIR" -c user.name=Microck -c user.email=contact@micr.dev \
  tag -a "$TAG" "$RELEASE_COMMIT" -F "$RELEASE_BODY"
git --git-dir="$GIT_DIR" push origin "refs/tags/$TAG"
gh release create "$TAG" "$PAYLOAD_DIR"/* --repo "$REPO" \
  --verify-tag --target "$RELEASE_COMMIT" --title "OpenVector $VERSION" \
  --notes-file "$RELEASE_BODY" --latest
```

Verify the remote tag's annotated type, target commit, and tagger. Publish a normal
release. Use a prerelease or draft only when explicitly authorized.

## 6. Publish the exact npm candidate

The npm target set is Linux x64/ARM64 and Windows x64. Desktop macOS QA does not
add macOS Node binaries; the npm workflow must explicitly build and test a target
before it is included. Download the successful run's single `npm-package` artifact
and set `NPM_TARBALL` to its absolute `.tgz` path. Do not rebuild or run `npm pack`
in a host-only directory to replace it.

Inspect its package name/version, members, notices, three target directories, and
SHA-512 integrity. Retain the audited tarball and integrity value. All six CI
installed-consumer jobs must have passed for those exact bytes.

Authenticate with a securely supplied token file. Agents use a secure secret-input
tool, not a chat request for a plaintext token. Never put credentials in artifacts,
source, command arguments, shell tracing, or a global npm configuration. For a
temporary `NPM_TOKEN_FILE` provided through that secure channel:

```bash
(
  set -euo pipefail
  set +x
  : "${NPM_TOKEN_FILE:?Secure npm token file required}"
  umask 077
  AUTH_DIR=$(mktemp -d)
  trap 'rm -f "$AUTH_DIR/npmrc"; rmdir "$AUTH_DIR"' EXIT
  printf '%s\n' 'registry=https://registry.npmjs.org/' \
    '//registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}' \
    'ignore-scripts=true' > "$AUTH_DIR/npmrc"
  export NPM_CONFIG_USERCONFIG="$AUTH_DIR/npmrc"
  export NODE_AUTH_TOKEN="$(cat "$NPM_TOKEN_FILE")"
  npm whoami
  npm publish "$NPM_TARBALL" --access public --tag latest --ignore-scripts
)
```

A rejected token is an authentication blocker, not a reason to change the package
or persist a token globally. A successful publish can report asynchronous
processing before downloads become available. Do not publish again because an
immediate lookup or tarball download returns 404.

## 7. Verify public downloads and close the release

GitHub verification must use fresh public downloads, not the staging files:

```bash
gh release view "$TAG" --repo "$REPO" --json isDraft,isPrerelease,assets,url
gh release download "$TAG" --repo "$REPO" --dir "$WORKDIR/public-desktop"
(cd "$WORKDIR/public-desktop" && sha256sum --check SHA256SUMS.txt)
```

Compare the downloaded manifest with the retained reviewed manifest and match the
exact asset inventory. Confirm `latest`, repository visibility, the tag target,
and provenance correspondence. Repeat a packaged installation/conversion smoke.
Signature checks and checksums do not replace the earlier native workflow QA.

Verify npm without publisher credentials:

```bash
npm view "openvector@$VERSION" version dist --json \
  --registry=https://registry.npmjs.org --userconfig=/dev/null
npm view openvector dist-tags --json --prefer-online \
  --registry=https://registry.npmjs.org --userconfig=/dev/null
```

Require `latest` to match the intended version. Download the canonical registry
tarball from `dist.tarball` into a new directory and compare its SHA-512 integrity
with the audited CI tarball. Set `PUBLIC_NPM_TARBALL` to its absolute path and run:

```bash
NPM_CONFIG_USERCONFIG=/dev/null npm run test:package -- "$PUBLIC_NPM_TARBALL"
```

`PUBLIC_NPM_TARBALL` must be the actual registry download. The checker creates and
removes an isolated project, exercises the installed Node API and CLI, compares
SVG bytes, and verifies existing-output protection. Keep that temporary project
boundary; npm otherwise can modify an ancestor project's manifests and dependencies.
The prior native CI results apply only when the downloaded bytes match.

If publication is interrupted or a response is uncertain, inspect remote state
before retrying any mutation:

- GitHub: inspect the tag, release, and existing asset names and hashes. Complete
  only missing uploads with the reviewed bytes. Never use `--clobber`.
- npm: inspect version metadata, `dist-tags`, integrity, and the actual tarball.
  Record accepted-but-processing separately from publicly verified. Inspect
  `Age` and `Cache-Control` on transient 404 responses and allow the observed
  cache lifetime to expire before another read. Do not repeat `npm publish`.
- Notarization: recover the existing submission ID and inspect its status/log
  before submitting another copy.
- A defective published build requires a new version and a correction entry in
  the changelog, not replacement of immutable published bytes.

Complete only when each authorized channel is publicly verified, or report its
specific pending state or blocker separately. Close task-owned processes and
mounts, collect evidence, remove temporary credential configuration, and destroy
disposable build resources. Preserve daily installations, signing identities,
operator preferences, and historical releases.
