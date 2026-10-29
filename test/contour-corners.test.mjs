import assert from 'node:assert/strict';
import { test } from 'node:test';
import decisions from './fixtures/corner-decision-reference.json' with { type:'json' };
import puncturing from './fixtures/contour-puncturing-reference.json' with { type:'json' };
import { classifyCornerFeatures } from '../src/contour-corners.mjs';
import { punctureContourCorners } from '../src/smoothing.mjs';

test('corner decisions match all 3504 original threshold, arbitrary and nonfinite feature vectors', () => {
  assert.equal(decisions.cases.length, 3504);
  for (const [index, fixture] of decisions.cases.entries()) {
    assert.equal(classifyCornerFeatures(fixture.features.map(Number)), fixture.expected, `corner decision ${index}`);
  }
});

test('complete corner puncturing matches all 3680 original raw, smoothed and transformed geometries', () => {
  assert.equal(puncturing.cases.length, 3680);
  for (const [index, fixture] of puncturing.cases.entries()) {
    assert.deepEqual(punctureContourCorners(fixture), fixture.expected, `corner puncturing ${index}`);
  }
});

test('corner puncturing owns geometry and rejects malformed inputs before producing a partial result', () => {
  const fixture = puncturing.cases.find(candidate => candidate.geometry.regions.some(region => region.contour.length > 6));
  const input = structuredClone(fixture), before = structuredClone(input);
  const actual = punctureContourCorners(input);
  actual.nodes[0].kind = 255;
  actual.regions[0].boundary[0][0] = 255;
  assert.deepEqual(input, before);
  const sparse = structuredClone(input.geometry);
  delete sparse.nodes[0];
  const invalidNode = structuredClone(input.geometry);
  invalidNode.nodes[0].x = Infinity;
  const invalidBoundary = structuredClone(input.geometry);
  invalidBoundary.regions[0].boundary[0][0] = 256;
  const invalidContour = structuredClone(input.geometry);
  invalidContour.regions[0].contour[0] = invalidContour.nodes.length;
  for (const geometry of [null, {}, sparse, invalidNode, invalidBoundary, invalidContour]) {
    assert.throws(() => punctureContourCorners({ geometry }));
  }
  assert.deepEqual(input, before);
});
