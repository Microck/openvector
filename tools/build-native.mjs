import { chmodSync, copyFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { cargoBuild, copyBundledNotices, root, run, rustNotices, toolchainNotices } from './native-notices.mjs';

try {
  const args = process.argv.slice(2);
  if (args.length !== 0 && (args.length !== 2 || args[0] !== '--target' || !args[1])) {
    throw new Error('Usage: npm run build -- [--target <Rust target triple>]');
  }
  const target = args[1] || process.env.CARGO_BUILD_TARGET ||
    run('rustc', ['-vV']).split('\n').find((line) => line.startsWith('host: '))?.slice(6).trim();
  const architectures = { x86_64: 'x64', aarch64: 'arm64' };
  const arch = architectures[target?.split('-')[0]];
  const platform = target?.endsWith('-unknown-linux-gnu') ? 'linux' :
    target?.endsWith('-apple-darwin') ? 'darwin' :
    /-pc-windows-(msvc|gnu)$/.test(target ?? '') ? 'win32' : undefined;
  if (!platform || !arch) {
    throw new Error(`Unsupported npm native target: ${target}. Use an x86_64 or aarch64 GNU/Linux, macOS or Windows target.`);
  }
  const toolchain = toolchainNotices(target);

  // An explicit target prevents a Cargo config from silently changing the
  // package platform. Cargo's artifact paths honor CARGO_TARGET_DIR and config.
  const artifacts = cargoBuild(target, ['--release', '-p', 'openvector-cli', '-p', 'openvector-node']);
  const executable = artifacts.find((artifact) =>
    artifact.target.name === 'openvector' && artifact.target.kind.includes('bin'))?.executable;
  const library = artifacts.find((artifact) =>
    artifact.target.name === 'openvector_node' && artifact.target.kind.includes('cdylib'))
    ?.filenames.find((filename) => /\.(so|dylib|dll)$/.test(filename));
  if (!executable || !library) {
    throw new Error('Cargo did not report both the openvector executable and openvector_node shared library.');
  }
  // Collected before copying so a missing license text leaves nothing half-packaged.
  const notices = rustNotices(target, artifacts);

  // Rust crate and toolchain licenses differ per target, so they sit beside that
  // platform's binaries.
  const destination = join(root, 'native', `${platform}-${arch}`);
  mkdirSync(destination, { recursive: true });
  const executableName = platform === 'win32' ? 'openvector.exe' : 'openvector';
  copyFileSync(library, join(destination, 'openvector-node.node'));
  copyFileSync(executable, join(destination, executableName));
  chmodSync(join(destination, executableName), 0o755);
  writeFileSync(join(destination, 'rust-licenses.txt'), notices);
  writeFileSync(join(destination, 'toolchain-licenses.txt'), toolchain.text);
  copyFileSync(toolchain.stdNotice, join(destination, 'rust-std-COPYRIGHT-library.html'));

  const licenses = join(root, 'native', 'licenses');
  mkdirSync(licenses, { recursive: true });
  copyBundledNotices(licenses);
  console.log(`Packaged native/${platform}-${arch}/${executableName} and openvector-node.node`);
} catch (error) {
  console.error(`${error.message}\nBuild from the source checkout or extracted npm package root with a current stable Rust toolchain and its rust-docs component, Cargo, Git, and a C compiler/linker for the selected target. Notice collection currently needs Debian-packaged GNU/Linux or MinGW runtimes and dpkg-query; see README.md. GPUI is not compiled by this command.`);
  process.exitCode = 1;
}
