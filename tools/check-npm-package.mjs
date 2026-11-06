import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

if (process.argv.length !== 3 || !process.env.npm_execpath) {
  throw new Error('Usage: npm run test:package -- <package.tgz>');
}
const archive = resolve(process.argv[2]);
const directory = mkdtempSync(join(tmpdir(), 'openvector-package-'));
const npm = (args) => execFileSync(process.execPath, [process.env.npm_execpath, ...args], {
  cwd: directory, stdio: 'inherit',
});
// Authored 8x8 BMP: a blue 4x4 square inside a red border. Expected SVG was
// independently checked with the native release packages on Linux and Windows.
const bitmap = 'Qk32AAAAAAAAADYAAAAoAAAACAAAAAgAAAABABgAAAAAAMAAAADEDgAAxA4AAAAAAAAAAAAAAAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD//wAA/wAA/wAA/wAAAAD/AAD/AAD/AAD//wAA/wAA/wAA/wAAAAD/AAD/AAD/AAD//wAA/wAA/wAA/wAAAAD/AAD/AAD/AAD//wAA/wAA/wAA/wAAAAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/AAD/';
const expected = 'e762312fc6cb03bca38f557bc5b2c724653a3ea774d722a5ab2f9e3d34d2f952';
try {
  npm(['install', '--ignore-scripts', '--no-audit', '--no-fund', archive]);
  writeFileSync(join(directory, 'input.bmp'), Buffer.from(bitmap, 'base64'));
  execFileSync(process.execPath, ['--input-type=module', '--eval', `
    import assert from 'node:assert/strict';
    import { readFileSync } from 'node:fs';
    import { CoreSession } from 'openvector';
    const document = new CoreSession().createDocument();
    document.setSource(readFileSync('input.bmp'), 'input.bmp');
    document.runAutomatic({ extension: '.bmp' });
    const saved = document.saveVector('api.svg', { layout: 'grouped-cutouts' });
    assert.equal(saved.status, 0, JSON.stringify(saved));
  `], { cwd: directory, stdio: 'inherit' });
  npm(['exec', '--offline', '--', 'openvector', 'vectorize', 'input.bmp', '--output', 'cli.svg']);
  for (const filename of ['api.svg', 'cli.svg']) {
    assert.equal(createHash('sha256').update(readFileSync(join(directory, filename))).digest('hex'), expected, filename);
  }
  writeFileSync(join(directory, 'protected.svg'), 'preserve existing file');
  const overwrite = spawnSync(process.execPath, [process.env.npm_execpath, 'exec', '--offline', '--',
    'openvector', 'vectorize', 'input.bmp', '--output', 'protected.svg'], { cwd: directory, encoding: 'utf8' });
  if (overwrite.error) throw overwrite.error;
  assert.notEqual(overwrite.status, 0, 'CLI must reject an existing output without --force');
  assert.equal(readFileSync(join(directory, 'protected.svg'), 'utf8'), 'preserve existing file');
  console.log(`Installed Node API and CLI passed on ${process.platform}-${process.arch} / ${process.version}`);
} finally {
  rmSync(directory, { recursive: true, force: true });
}
