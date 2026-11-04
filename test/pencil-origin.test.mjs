import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const require = createRequire(import.meta.url);
const native = process.env.OPENVECTOR_NODE_BINARY
  ? require(process.env.OPENVECTOR_NODE_BINARY)
  : await import('../npm/index.mjs');

test('short Pencil crossing stays away from origin through SVG export', () => {
  const rows = gunzipSync(readFileSync(new URL('./fixtures/bitmap-scenes-reference.ndjson.gz', import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const scene = rows.find(row => row.id === 'opaque-four-plugins');
  const document = new native.CoreSession().createDocument();
  document.setSource(Buffer.from(scene.sourceBytes, 'base64'), scene.sourceFilename);
  const { settings } = document.runAutomatic({ extension: '.bmp' });
  document.beginEditing();
  const before = document.editing();
  const x = Math.floor(before.width / 2), y = Math.floor(before.height / 2);
  assert.ok(x >= 2 && y >= 2 && x + 3 < before.width);
  const color = [0.8, 0.1, 0.9, 1];
  document.edit({ tool: 'pencil', from: [x - 0.2, y + 0.1], to: [x + 0.1, y + 0.1], color });
  const edited = document.edit({ tool: 'pencil', from: [x + 0.1, y + 0.1], to: [x + 3.1, y + 0.1], color });
  assert.deepEqual(Array.from(edited.bgra.subarray(0, 4)), Array.from(before.bgra.subarray(0, 4)));
  const offset = (y * before.width + x) * 4;
  const painted = Array.from(edited.bgra.subarray(offset, offset + 4));
  assert.notDeepEqual(painted, Array.from(before.bgra.subarray(offset, offset + 4)));
  document.finishEditing(settings);
  const directory = mkdtempSync(join(tmpdir(), 'openvector-pencil-'));
  try {
    const filename = join(directory, 'result.svg');
    const result = document.saveVector(filename, { layout: 'stacked' });
    assert.equal(result.status, 0);
    const svg = readFileSync(filename, 'utf8');
    const hex = [painted[2], painted[1], painted[0]].map(value => value.toString(16).padStart(2, '0')).join('');
    const paths = [...svg.matchAll(new RegExp('<path fill="#' + hex + '"[^>]*>', 'gi'))];
    assert.ok(paths.length > 0, 'edited color should survive export');
    for (const [path] of paths) assert.doesNotMatch(path, /M\s+0(?:\.0+)?\s+0(?:\.0+)?(?:\s|$)/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
