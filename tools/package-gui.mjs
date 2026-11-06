// Builds openvector-gui for one Rust target and stages it in a new directory with
// the app license and every third-party notice its binary needs: the bundled C
// sources, the embedded Geist fonts and Lucide icons, the Rust crates, and the Rust
// standard library and C runtime from the toolchain.
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { basename, join } from 'node:path';
import { parseArgs } from 'node:util';
import { cargoBuild, copyBundledNotices, root, rustNotices, toolchainNotices, workspace } from './native-notices.mjs';

try {
  const { values, positionals } = parseArgs({
    options: { target: { type: 'string' }, profile: { type: 'string', default: 'release' } },
    allowPositionals: true,
  });
  if (!values.target || positionals.length !== 1) {
    throw new Error('Usage: node tools/package-gui.mjs --target <Rust target triple> [--profile <Cargo profile>] <new directory>');
  }
  const toolchain = toolchainNotices(values.target);
  const artifacts = cargoBuild(values.target, ['--profile', values.profile, '-p', 'openvector-gui']);
  const executable = artifacts.find((artifact) =>
    artifact.target.name === 'openvector-gui' && artifact.target.kind.includes('bin'))?.executable;
  if (!executable) throw new Error('Cargo did not report the openvector-gui executable.');
  const notices = rustNotices(values.target, artifacts);

  // A fresh directory guarantees no file from an earlier build or target ships.
  const destination = positionals[0];
  const darwin = values.target.endsWith('-apple-darwin');
  const bundle = join(destination, 'OpenVector.app');
  // Licenses must travel with the app when a macOS user drags it out of a DMG.
  const packageRoot = darwin ? join(bundle, 'Contents', 'Resources') : destination;
  const licenses = join(packageRoot, 'licenses');
  mkdirSync(destination);
  mkdirSync(licenses, { recursive: true });
  // macOS shows an app's Dock and Finder icon only for a bundle, so Darwin
  // targets get OpenVector.app with the executable under Contents/MacOS.
  const binaryDirectory = darwin ? join(bundle, 'Contents', 'MacOS') : destination;
  mkdirSync(binaryDirectory, { recursive: true });
  copyFileSync(executable, join(binaryDirectory, basename(executable)));
  copyFileSync(join(root, 'LICENSE'), join(packageRoot, 'LICENSE'));
  copyBundledNotices(licenses);
  const assets = join(workspace, 'crates', 'openvector-gui', 'assets');
  copyFileSync(join(assets, 'fonts', 'OFL.txt'), join(licenses, 'geist-fonts-OFL.txt'));
  copyFileSync(join(assets, 'icons', 'LICENSE'), join(licenses, 'lucide-icons-LICENSE.txt'));
  // Windows embeds its icon in the exe (crates/openvector-gui/build.rs). Linux
  // desktops find it through a desktop entry whose name matches the window's
  // app_id, laid out as an XDG prefix: copy share/ into ~/.local/share.
  if (values.target.includes('-linux-')) {
    const applications = join(destination, 'share', 'applications');
    const icons = join(destination, 'share', 'icons', 'hicolor', '256x256', 'apps');
    mkdirSync(applications, { recursive: true });
    mkdirSync(icons, { recursive: true });
    copyFileSync(join(assets, 'openvector.desktop'), join(applications, 'openvector.desktop'));
    copyFileSync(join(assets, 'app-icon-256.png'), join(icons, 'openvector.png'));
  }
  if (darwin) {
    const resources = join(bundle, 'Contents', 'Resources');
    mkdirSync(resources, { recursive: true });
    // actool output from assets/OpenVector.icon (see crates/openvector-gui/build.rs).
    // macOS 26 reads Assets.car through CFBundleIconName; a bare .icns is shown on
    // a grey plate. OpenVector.icns is actool's fallback for older macOS.
    for (const file of ['Assets.car', 'OpenVector.icns']) copyFileSync(join(assets, 'macos', file), join(resources, file));
    const { version } = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
    writeFileSync(join(bundle, 'Contents', 'Info.plist'), infoPlist(basename(executable), version));
  }
  writeFileSync(join(licenses, 'rust-licenses.txt'), notices);
  writeFileSync(join(licenses, 'toolchain-licenses.txt'), toolchain.text);
  copyFileSync(toolchain.stdNotice, join(licenses, 'rust-std-COPYRIGHT-library.html'));
  // Seal only after every bundled notice is written. This local-use signature
  // is replaced by Developer ID signing during public macOS release packaging.
  if (darwin && process.platform === 'darwin') {
    execFileSync('codesign', ['--force', '--sign', '-', bundle], { stdio: 'inherit' });
  }
  console.log(`Packaged ${basename(executable)} in ${destination}`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}

function infoPlist(executable, version) {
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>OpenVector</string>
  <key>CFBundleDisplayName</key><string>OpenVector</string>
  <key>CFBundleIdentifier</key><string>dev.micr.openvector</string>
  <key>CFBundleExecutable</key><string>${executable}</string>
  <key>CFBundleIconFile</key><string>OpenVector</string>
  <key>CFBundleIconName</key><string>OpenVector</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
`;
}
