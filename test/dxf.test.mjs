import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createReadStream, readFileSync } from 'node:fs';
import { createGunzip } from 'node:zlib';
import { createInterface } from 'node:readline';
import { exportDxf } from '../src/dxf.mjs';
import cliReference from './fixtures/dxf-cli-reference.json' with { type:'json' };
import overlongReference from './fixtures/dxf-overlong-reference.json' with { type:'json' };

const directory = new URL('./fixtures/dxf-reference/', import.meta.url);
const index = JSON.parse(readFileSync(new URL('index.json', directory), 'utf8'));
const layouts = ['stacked', 'cutouts', 'grouped-cutouts'], modes = ['splines', 'sparse-lines', 'dense-lines'];

test('DXF matches all 65934 original files across layers, handles, curves, visibility and dimensions', async () => {
  const geometries = [];
  let count = 0, summary;
  for (const part of index.parts) {
    const rows = createInterface({ input:createReadStream(new URL(part.file, directory)).pipe(createGunzip()), crlfDelay:Infinity });
    for await (const line of rows) {
      const fixture = JSON.parse(line);
      if (fixture.type === 'geometry') {
        assert.equal(fixture.index, geometries.length); geometries.push(fixture.value);
      } else if (fixture.type === 'summary') {
        summary = fixture;
      } else if (fixture.type === 'case') {
        const shape = geometries[fixture.geometry];
        const hiddenRegions = shape.geometry.regions.flatMap((_, region) =>
          region < fixture.hiddenSize && (fixture.hiddenWords[region >>> 5] & (1 << (region & 31))) ? [region] : []);
        const actual = exportDxf({ ...shape, hiddenRegions, layout:layouts[fixture.shapeMode], thinStroke:fixture.thinStroke,
          outputWidth:fixture.outputWidth, outputHeight:fixture.outputHeight, curveMode:modes[fixture.curveMode] });
        if (actual !== fixture.expected) {
          let offset = 0;
          while (offset < actual.length && offset < fixture.expected.length && actual[offset] === fixture.expected[offset]) offset++;
          assert.fail(`DXF case ${count}, ${shape.id}, mode ${fixture.curveMode}, layout ${fixture.shapeMode}, byte ${offset}: ` +
            `expected ${JSON.stringify(fixture.expected.slice(offset, offset + 120))}, actual ${JSON.stringify(actual.slice(offset, offset + 120))}`);
        }
        count++;
      } else assert.equal(fixture.type, 'metadata');
    }
  }
  assert.equal(count, 65934); assert.equal(geometries.length, 3264);
  assert.deepEqual(summary, { type:'summary', cases:count, geometries:geometries.length });
});

test('DXF defaults, typed palettes and duplicate hidden IDs preserve caller storage', () => {
  const fixture = cliReference.cases[cliReference.defaultCaseIndex];
  assert.equal(fixture.input.curveMode, 'dense-lines'); assert.equal(fixture.input.layout, 'stacked');
  assert.equal(fixture.input.outputWidth, fixture.input.width); assert.equal(fixture.input.outputHeight, fixture.input.height);
  const input = structuredClone(fixture.input);
  for (const key of ['curveMode', 'layout', 'thinStroke', 'outputWidth', 'outputHeight', 'hiddenRegions']) delete input[key];
  input.palette = new Float32Array(input.palette);
  const before = structuredClone(input);
  assert.equal(exportDxf(input), fixture.expected); assert.deepEqual(input, before);
  assert.equal(exportDxf({ ...input, hiddenRegions:[0, 0] }), exportDxf({ ...input, hiddenRegions:[0] }));
});

test('DXF retains unused palette entries and exports hidden extreme geometry', () => {
  const unused = cliReference.cases[cliReference.unusedPaletteCaseIndex];
  assert.ok(unused.input.palette.length / 4 > new Set(unused.input.geometry.regions.map(region => region.bgra.join(','))).size);
  assert.equal(exportDxf(unused.input), unused.expected);
  const hidden = cliReference.cases[cliReference.hiddenExtremeCaseIndex];
  assert.ok(hidden.input.geometry.nodes.some(node => Math.abs(node.x) > 1e300));
  assert.equal(exportDxf(hidden.input), hidden.expected);
});

test('DXF reports all 144 visible overlong-field diagnostics without truncating them', () => {
  assert.equal(overlongReference.cases.length, 144);
  for (const fixture of overlongReference.cases) assert.throws(() => exportDxf(fixture.input), /DXF.*256-byte/);
});

test('DXF rejects invalid graph, dimensions, mode and palette before returning output', () => {
  const input = cliReference.cases[0].input;
  for (const patch of [{ width:0 }, { outputWidth:undefined }, { hiddenRegions:[-1] }, { layout:'unknown' },
    { thinStroke:1 }, { curveMode:'unknown' }, { curveMode:null }, { curveMode:0 }, { palette:undefined }, { palette:[] }, { palette:[0, 0, 0] },
    { palette:[0, 0, NaN, 1] }, { palette:[0, -1, 0, 1] }, { palette:[0, 0, 0, 2] },
    { palette:[0, 0, '0', 1] }, { geometry:null }]) {
    assert.throws(() => exportDxf({ ...input, ...patch }), /DXF/);
  }
  const geometry = structuredClone(input.geometry); geometry.nodes[0].x = Infinity;
  assert.throws(() => exportDxf({ ...input, geometry }), /DXF/);
});
