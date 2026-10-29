import assert from 'node:assert/strict';
import { test } from 'node:test';
import reference from './fixtures/pixel-map-reference.json' with { type: 'json' };
import { PixelMap } from '../src/pixel-map.mjs';

test('flood fill matches all 5574 original host-CPU map mutations', () => {
  assert.equal(reference.cases.length, 5574);
  for (const fixture of reference.cases) {
    const map = new PixelMap(fixture);
    assert.equal(map.floodFill(fixture.seed, fixture.replacement), undefined);
    assert.deepEqual(map.toJSON(), {
      width: fixture.width, height: fixture.height, segments: fixture.expected,
    }, `fill ${fixture.width}x${fixture.height} at ${fixture.seed}`);
  }
});

test('map storage and exported snapshots belong to separate owners', () => {
  const segments = new Int32Array([0, 1, 1, 0]);
  const map = new PixelMap({ width: 2, height: 2, segments });
  segments[0] = 99;
  const snapshot = map.toJSON();
  snapshot.segments[0] = 100;
  map.floodFill(0, 7);
  assert.deepEqual(map.toJSON().segments, [7, 1, 1, 0]);
});

test('invalid map or edit inputs fail before any mutation', () => {
  for (const input of [null, {}, { width: 0, height: 1, segments: [] },
    { width: 2, height: 1, segments: [1] },
    { width: 1, height: 1, segments: [2147483648] },
    { width: 1, height: 1, segments: [1.5] },
    { width: 1, height: 1, segments: new Array(1) }]) {
    assert.throws(() => new PixelMap(input));
  }
  const map = new PixelMap({ width: 2, height: 1, segments: [0, 1] });
  for (const [seed, replacement] of [[-1, 2], [2, 2], [0.5, 2], [0, 0], [0, 2147483648]]) {
    assert.throws(() => map.floodFill(seed, replacement));
    assert.deepEqual(map.toJSON().segments, [0, 1]);
  }
});
