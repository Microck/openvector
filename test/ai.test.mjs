import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { exportAi } from '../src/ai.mjs';

const reference = JSON.parse(gunzipSync(readFileSync(new URL('./fixtures/ai-reference.json.gz', import.meta.url))));
const layouts = ['stacked', 'cutouts', 'grouped-cutouts'];
function inputFor(fixture) {
  const shape = reference.geometries[fixture.geometry];
  const hiddenRegions = shape.geometry.regions.flatMap((_, region) =>
    region < fixture.hiddenSize && (fixture.hiddenWords[region >>> 5] & (1 << (region & 31))) ? [region] : []);
  return { ...shape, layout:layouts[fixture.shapeMode], thinStroke:fixture.thinStroke,
    outputWidth:fixture.outputWidth, outputHeight:fixture.outputHeight, hiddenRegions };
}

test('AI matches all 22014 original files including headers, compounds, colors, strokes and scaling', () => {
  assert.equal(reference.cases.length, 22014);
  assert.equal(reference.geometries.length, 3264);
  for (const [index, fixture] of reference.cases.entries()) {
    assert.equal(exportAi(inputFor(fixture)), fixture.expected, `original AI ${index}`);
  }
});

test('AI defaults and duplicate hidden IDs preserve caller geometry', () => {
  const input = structuredClone(inputFor(reference.cases[0]));
  for (const key of ['layout', 'thinStroke', 'outputWidth', 'outputHeight', 'hiddenRegions']) delete input[key];
  const before = structuredClone(input);
  assert.equal(exportAi(input), reference.cases[0].expected);
  assert.deepEqual(input, before);
  assert.equal(exportAi({ ...input, hiddenRegions:[0, 0] }), exportAi({ ...input, hiddenRegions:[0] }));
});

test('AI rejects invalid dimensions, options and fitted references before returning text', () => {
  const valid = inputFor(reference.cases[0]);
  for (const patch of [{ width:0 }, { height:2 ** 31 }, { outputWidth:undefined }, { layout:'unknown' },
    { thinStroke:1 }, { hiddenRegions:[-1] }, { hiddenRegions:[valid.geometry.regions.length] }, { geometry:null }]) {
    assert.throws(() => exportAi({ ...valid, ...patch }), /AI/);
  }
  const geometry = structuredClone(valid.geometry); geometry.nodes[0].x = NaN;
  assert.throws(() => exportAi({ ...valid, geometry }), /AI/);
});
