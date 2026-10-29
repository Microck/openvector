import assert from 'node:assert/strict';
import { test } from 'node:test';
import reference from './fixtures/contour-extraction-reference.json' with { type: 'json' };
import pipelineReference from './fixtures/pipeline-contour-reference.json' with { type: 'json' };
import { extractContours } from '../src/contours.mjs';
import { segment } from '../src/segmentation.mjs';

test('independent segmentation and contours match all 15 original complete-pipeline stages', () => {
  assert.equal(pipelineReference.cases.length, 15);
  for (const fixture of pipelineReference.cases) {
    const segmented = segment(fixture);
    const geometry = extractContours({ ...segmented, width:fixture.width, height:fixture.height,
      antiAlias:fixture.parameters['Shared::is_anti_aliased'] === 1 });
    assert.deepEqual(geometry, fixture.expected, fixture.id);
  }
});

test('contour extraction matches all 2584 original ordered geometry states', () => {
  assert.equal(reference.cases.length, 2584);
  let exhaustive = 0, staircase = 0, nested = 0;
  for (const [index, fixture] of reference.cases.entries()) {
    assert.deepEqual(extractContours(fixture), fixture.expected, `contour case ${index}`);
    if (fixture.purpose?.startsWith('Exhaustive')) exhaustive++;
    if (fixture.purpose?.startsWith('Long')) staircase++;
    if (fixture.purpose?.startsWith('Nested')) nested++;
  }
  assert.equal(exhaustive, 1312);
  assert.equal(staircase, 112);
  assert.equal(nested, 4);
});

test('native geometry corpus exercises staircase reductions and nested containment', () => {
  const staircases = reference.cases.filter(fixture => fixture.purpose?.startsWith('Long'));
  let reduced = 0;
  for (let index = 0; index < staircases.length; index += 2) {
    const plain = staircases[index], blended = staircases[index + 1];
    assert.deepEqual(plain.indices, blended.indices);
    assert.equal(plain.antiAlias, false);
    assert.equal(blended.antiAlias, true);
    if (plain.expected.nodes.length < blended.expected.nodes.length) reduced++;
  }
  assert.ok(reduced > 0, 'Plain-image constructor patterns must actually remove samples.');
  for (const fixture of reference.cases.filter(sample => sample.purpose?.startsWith('Nested'))) {
    assert.deepEqual(fixture.expected.regions.map(region => region.parent), [-1, 0, 1, 2]);
  }
});

test('contours own all output storage and preserve typed input arrays', () => {
  const input = structuredClone(reference.cases.find(fixture => fixture.purpose?.startsWith('Nested')));
  for (const key of ['indices', 'counts', 'colorIndices']) input[key] = Int32Array.from(input[key]);
  input.palette = Float32Array.from(input.palette);
  const before = structuredClone(input);
  const geometry = extractContours(input);
  for (const node of geometry.nodes) { node.x = 7; node.flags = 7; }
  for (const region of geometry.regions) {
    region.contour.fill(7);
    region.neighbors.fill(7);
    region.bgra.fill(7);
    for (const edge of region.boundary) edge.fill(7);
  }
  assert.deepEqual(input, before);
  assert.deepEqual(extractContours(input), input.expected);
});

test('contour input validation rejects inconsistent or disconnected region maps', () => {
  const input = structuredClone(reference.cases[0]);
  const before = structuredClone(input);
  const disconnected = { ...input, indices:[0, 1, 1, 0], counts:[2, 2], colorIndices:[0, 0] };
  for (const invalid of [null, {}, { ...input, width:1 }, { ...input, height:2.5 },
    { ...input, antiAlias:undefined }, { ...input, antiAlias:1 }, { ...input, indices:[0] },
    { ...input, indices:[0, 0, 0, -1] }, { ...input, indices:[1, 1, 1, 1] },
    { ...input, counts:[3] }, { ...input, counts:new Array(1) }, { ...input, counts:[4, 0], colorIndices:[0, 0] },
    { ...input, colorIndices:[1] }, { ...input, palette:[] }, { ...input, palette:[NaN, 0, 0, 1] },
    { ...disconnected, indices:[1, 0, 0, 1] }, disconnected]) {
    assert.throws(() => extractContours(invalid));
  }
  assert.deepEqual(input, before);
});
