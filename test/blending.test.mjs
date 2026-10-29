import assert from 'node:assert/strict';
import { test } from 'node:test';
import reference from './fixtures/blend-sample-reference.json' with { type: 'json' };
import scoreReference from './fixtures/weighted-score-reference.json' with { type: 'json' };
import { fitColorBlend, scoreWeightedPalette } from '../src/blending.mjs';

test('local color blends match all 1080 original solver calls exactly', () => {
  assert.equal(reference.cases.length, 1080);
  for (const [index, fixture] of reference.cases.entries()) {
    const blended = fitColorBlend(fixture);
    assert.deepEqual(Array.from(blended.color), fixture.expected.color, `case ${index} color`);
    assert.deepEqual(Array.from(blended.weights), fixture.expected.weights, `case ${index} weights`);
    assert.equal(blended.selfWeight, fixture.expected.selfWeight, `case ${index} self weight`);
  }
});

test('blend boundaries preserve inputs and reject invalid samples and colors', () => {
  const input = { bgra: new Uint8Array([0, 127, 255, 255]), colors: new Float32Array([0, 0, 0, 1]) };
  const before = structuredClone(input);
  const blended = fitColorBlend(input);
  blended.color.fill(7);
  blended.weights.fill(7);
  assert.deepEqual(input, before);
  for (const invalid of [null, {}, { ...input, bgra: [0] }, { ...input, bgra: new Array(4) },
    { ...input, bgra: [0, 0, 0, 256] }, { ...input, colors: [] }, { ...input, colors: new Array(4) },
    { ...input, colors: new Float32Array(40) }, { ...input, colors: [NaN, 0, 0, 1] },
    { ...input, colors: [0, 0, 0, 2] }]) assert.throws(() => fitColorBlend(invalid));
});

test('weighted palette scores match all 440 original scorer calls exactly', () => {
  assert.equal(scoreReference.cases.length, 440);
  for (const [index, fixture] of scoreReference.cases.entries()) {
    assert.deepEqual(scoreWeightedPalette(fixture), fixture.expected, `case ${index}`);
  }
});

test('weighted scoring validates boundaries and preserves supplied assignments', () => {
  const input = structuredClone(scoreReference.cases.find(fixture => fixture.width === 3 && fixture.colorCount === 3));
  const before = structuredClone(input);
  scoreWeightedPalette(input);
  assert.deepEqual(input, before);
  for (const invalid of [null, {}, { ...input, width: -1 }, { ...input, height: 1.5 },
    { ...input, bgra: [0] }, { ...input, indices: [] }, { ...input, indices: new Array(input.indices.length) },
    { ...input, indices: input.indices.map(() => input.colorCount) }, { ...input, costPerColor: Infinity }]) {
    assert.throws(() => scoreWeightedPalette(invalid));
  }
});
