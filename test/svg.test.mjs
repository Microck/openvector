import assert from 'node:assert/strict';
import { test } from 'node:test';
import reference from './fixtures/svg-reference.json' with { type:'json' };
import { exportSvg } from '../src/svg.mjs';

const layouts = ['stacked', 'cutouts', 'grouped-cutouts'];
function inputFor(fixture) {
  const shape = reference.geometries[fixture.geometry];
  const hiddenRegions = shape.geometry.regions.flatMap((_, region) =>
    region < fixture.hiddenSize && (fixture.hiddenWords[region >>> 5] & (1 << (region & 31))) ? [region] : []);
  return { ...shape, layout:layouts[fixture.shapeMode], thinStroke:fixture.thinStroke,
    outputWidth:fixture.outputWidth, outputHeight:fixture.outputHeight, hiddenRegions };
}

test('SVG serialization matches all 17436 original layout, stroke, visibility and formatting results byte for byte', () => {
  assert.equal(reference.geometries.length, 2743);
  assert.equal(reference.cases.length, 17436);
  for (const [index, fixture] of reference.cases.entries()) {
    assert.equal(exportSvg(inputFor(fixture)), fixture.expected, `original SVG ${index}`);
  }
});

test('SVG defaults and hidden-region IDs preserve caller geometry and export dimensions', () => {
  const input = structuredClone(inputFor(reference.cases[0])), before = structuredClone(input);
  delete input.layout; delete input.thinStroke; delete input.outputWidth; delete input.outputHeight;
  assert.equal(exportSvg(input), reference.cases[0].expected);
  assert.deepEqual(input.geometry, before.geometry);
  input.hiddenRegions = [0, 0];
  assert.equal(exportSvg(input), exportSvg({ ...input, hiddenRegions:[0] }));
});

test('SVG boundary rejects invalid options and incomplete fitted graph references', () => {
  const valid = inputFor(reference.cases[0]);
  for (const patch of [{ width:0 }, { height:2 ** 31 }, { layout:'unknown' }, { thinStroke:1 },
    { outputWidth:undefined }, { outputHeight:0 }, { hiddenRegions:[-1] }, { hiddenRegions:[valid.geometry.regions.length] },
    { hiddenRegions:[0.5] }, { hiddenRegions:null }, { geometry:null }]) {
    assert.throws(() => exportSvg({ ...valid, ...patch }), /SVG/);
  }
  for (const damage of [g => { g.nodes[0].x = Infinity; }, g => { g.regions[0].bgra[0] = 256; },
    g => { g.regions[0].parent = g.regions.length; }, g => { g.regions[0].contour[0] = -1; },
    g => { g.regions[0].edges[0][1] = 7; }, g => { g.regions[0].edges[0][3] = -1; },
    g => { g.regions[0].edges[0][0] = g.cubicStorageSlots; g.regions[0].edges[0][1] = 1; }]) {
    const geometry = structuredClone(valid.geometry); damage(geometry);
    assert.throws(() => exportSvg({ ...valid, geometry }), /SVG/);
  }
  const curved = inputFor(reference.cases.find(frame => reference.geometries[frame.geometry].geometry.cubics.length > 0));
  for (const damage of [g => { g.cubics[0].points[0] = NaN; }, g => { g.cubics.push(g.cubics[0]); },
    g => { g.cubics = []; }]) {
    const geometry = structuredClone(curved.geometry); damage(geometry);
    assert.throws(() => exportSvg({ ...curved, geometry }), /SVG/);
  }
});
