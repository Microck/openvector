import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { exportEps } from '../src/eps.mjs';

const reference = JSON.parse(gunzipSync(readFileSync(new URL('./fixtures/eps-reference.json.gz', import.meta.url))));
const layouts = ['stacked', 'cutouts', 'grouped-cutouts'];
function inputFor(fixture) {
  const shape = reference.geometries[fixture.geometry];
  const hiddenRegions = shape.geometry.regions.flatMap((_, region) =>
    region < fixture.hiddenSize && (fixture.hiddenWords[region >>> 5] & (1 << (region & 31))) ? [region] : []);
  return { ...shape, layout:layouts[fixture.shapeMode], thinStroke:fixture.thinStroke,
    outputWidth:fixture.outputWidth, outputHeight:fixture.outputHeight, hiddenRegions,
    compact:fixture.postscriptStyle !== 1, creationDate:fixture.creationDate };
}

test('EPS matches every original layout, stroke, style, visibility, color and scaling byte', () => {
  assert.equal(reference.cases.length, 41010);
  assert.equal(reference.geometries.length, 3005);
  for (const [index, fixture] of reference.cases.entries()) {
    assert.equal(exportEps(inputFor(fixture)), fixture.expected, `original EPS ${index}`);
  }
});

test('EPS defaults, duplicate hidden IDs and caller ownership use the common fitted graph contract', () => {
  const fixture = reference.cases[1], input = structuredClone(inputFor(fixture));
  for (const key of ['layout', 'thinStroke', 'compact', 'outputWidth', 'outputHeight', 'hiddenRegions']) delete input[key];
  const before = structuredClone(input);
  assert.equal(exportEps(input), fixture.expected);
  assert.deepEqual(input, before);
  assert.equal(exportEps({ ...input, hiddenRegions:[0, 0] }), exportEps({ ...input, hiddenRegions:[0] }));
  assert.ok(reference.cases.some(frame => frame.expected.includes('0.00392')));
  assert.ok(reference.cases.some(frame => frame.expected.includes('#J')));
});

test('default EPS creation metadata records the actual local clock without a mock', () => {
  const input = inputFor(reference.cases[1]); delete input.creationDate;
  const before = Date.now(), eps = exportEps(input), after = Date.now();
  const creationDate = /^%%CreationDate: ([^\r\n]*)\r$/m.exec(eps)?.[1];
  assert.match(creationDate, /^[A-Z][a-z]{2} [A-Z][a-z]{2} [ \d]\d \d{2}:\d{2}:\d{2} \d{4}$/);
  const timestamp = Date.parse(creationDate);
  assert.ok(timestamp >= Math.floor(before / 1000) * 1000 && timestamp <= Math.floor(after / 1000) * 1000);
  assert.equal(eps, exportEps({ ...input, creationDate }));
});

test('EPS rejects invalid options, metadata and graph references before returning bytes', () => {
  const valid = inputFor(reference.cases[1]);
  for (const patch of [{ width:0 }, { height:2 ** 31 }, { outputHeight:undefined },
    { layout:'unknown' }, { thinStroke:1 }, { compact:'yes' }, { hiddenRegions:[-1] },
    { creationDate:null }, { creationDate:'' }, { creationDate:'x'.repeat(256) },
    { creationDate:'line\nbreak' }, { creationDate:'line\rbreak' }, { creationDate:'é' }, { geometry:null }]) {
    assert.throws(() => exportEps({ ...valid, ...patch }), /EPS/);
  }
  const geometry = structuredClone(valid.geometry); geometry.nodes[0].x = Infinity;
  assert.throws(() => exportEps({ ...valid, geometry }), /EPS/);
});
