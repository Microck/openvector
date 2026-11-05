// Wrap the existing staged Windows GUI package without rebuilding or dropping
// its license files. NSIS and zip run on the Linux cross-build host.
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative, resolve } from 'node:path';
import { root } from './native-notices.mjs';

try {
  if (process.argv.length !== 4) throw new Error('Usage: node tools/package-windows.mjs <staged Windows directory> <new output directory>');
  const stage = resolve(process.argv[2]), output = resolve(process.argv[3]);
  const { version } = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
  for (const file of ['openvector-gui.exe', 'LICENSE']) {
    if (!statSync(join(stage, file)).isFile()) throw new Error(`Missing package file: ${file}`);
  }
  if (!statSync(join(stage, 'licenses')).isDirectory()) throw new Error('Missing package licenses directory.');
  const entries = readdirSync(stage, { recursive: true, withFileTypes: true });
  if (entries.some(entry => !entry.isFile() && !entry.isDirectory())) throw new Error('Package entries must be regular files or directories.');
  const packagePath = entry => relative(stage, join(entry.parentPath, entry.name));
  const escape = value => value.replaceAll('$', '$$').replaceAll('"', '$\\"');
  const uninstall = [
    ...entries.filter(entry => entry.isFile()).map(entry => `Delete "$INSTDIR\\${escape(packagePath(entry).replaceAll('/', '\\'))}"`),
    ...entries.filter(entry => entry.isDirectory()).map(packagePath).sort((a, b) => b.length - a.length)
      .map(directory => `RMDir "$INSTDIR\\${escape(directory.replaceAll('/', '\\'))}"`),
  ].join('\n');
  const temporary = mkdtempSync(join(tmpdir(), 'openvector-installer-'));
  try {
    const include = join(temporary, 'uninstall.nsh');
    writeFileSync(include, `${uninstall}\n`);
    mkdirSync(output);
    const prefix = `openvector-${version}-windows-x64`;
    execFileSync('zip', ['-q', '-r', join(output, `${prefix}-portable.zip`), '.'], { cwd: stage, stdio: 'inherit' });
    execFileSync('makensis', ['-V3', `-DVERSION=${version}`, `-DSTAGE=${stage}`,
      `-DOUTPUT=${join(output, `${prefix}-Setup.exe`)}`, `-DUNINSTALL_INCLUDE=${include}`,
      `-DAPP_ICON=${join(root, 'rust/crates/openvector-gui/assets/app-icon.ico')}`,
      join(root, 'tools/windows-installer.nsi')], { stdio: 'inherit' });
    console.log(`Packaged Windows installer and portable ZIP in ${output}`);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
