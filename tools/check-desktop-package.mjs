// Exercise the packaged GUI, not a workspace substitute. Each invocation owns
// its input, output, preferences and child process; no operator state is used.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { encodeBmp } from '../src/bitmap-codecs.mjs';

if (process.argv.length !== 4) throw new Error('Usage: node tools/check-desktop-package.mjs <packaged GUI executable> <matching CLI executable>');
const gui = resolve(process.argv[2]), cli = resolve(process.argv[3]);
const directory = mkdtempSync(join(tmpdir(), 'openvector-package-check-'));
try {
  const bgra = new Uint8Array(8 * 8 * 4);
  for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) {
    bgra.set(x >= 2 && x < 6 && y >= 2 && y < 6 ? [255,0,0,255] : [0,0,255,255], (y * 8 + x) * 4);
  }
  const input = join(directory, 'two colors.bmp'), output = join(directory, 'gui.svg');
  writeFileSync(input, encodeBmp({ width:8, height:8, bgra }));
  const env = { ...process.env, OPENVECTOR_PREFERENCES:join(directory, 'preferences.json') };
  const expected = spawnSync(cli, ['vectorize', input, '--format', 'svg'], { env, timeout:180000, killSignal:'SIGKILL' });
  assert.equal(expected.status, 0, expected.error?.message ?? expected.stderr.toString());
  assert.match(expected.stdout.toString(), /<svg\b/);
  assert.match(expected.stdout.toString(), /<path\b/);
  const converted = spawnSync(gui, ['--smoke', input, output], { env, timeout:180000, killSignal:'SIGKILL' });
  assert.equal(converted.status, 0, converted.error?.message ?? converted.stderr.toString());
  assert.deepEqual(readFileSync(output), expected.stdout, 'Packaged GUI conversion must match the native CLI.');
  console.log('Packaged GUI opened the authored bitmap, converted it and saved the exact expected SVG.');
} finally {
  rmSync(directory, { recursive:true, force:true });
}
