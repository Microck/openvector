// Builds openvector-gui for one Rust target and stages it in a new directory with
// the app license and every third-party notice its binary needs: the bundled C
// sources, the embedded Geist fonts and Lucide icons, the Rust crates, and the Rust
// standard library and C runtime from the toolchain.
import { copyFileSync, mkdirSync, writeFileSync } from 'node:fs';
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
  const licenses = join(destination, 'licenses');
  mkdirSync(destination);
  mkdirSync(licenses);
  copyFileSync(executable, join(destination, basename(executable)));
  copyFileSync(join(root, 'LICENSE'), join(destination, 'LICENSE'));
  copyBundledNotices(licenses);
  const assets = join(workspace, 'crates', 'openvector-gui', 'assets');
  copyFileSync(join(assets, 'fonts', 'OFL.txt'), join(licenses, 'geist-fonts-OFL.txt'));
  copyFileSync(join(assets, 'icons', 'LICENSE'), join(licenses, 'lucide-icons-LICENSE.txt'));
  writeFileSync(join(licenses, 'rust-licenses.txt'), notices);
  writeFileSync(join(licenses, 'toolchain-licenses.txt'), toolchain.text);
  copyFileSync(toolchain.stdNotice, join(licenses, 'rust-std-COPYRIGHT-library.html'));
  console.log(`Packaged ${basename(executable)} in ${destination}`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
