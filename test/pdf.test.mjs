import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { exportPdf } from '../src/pdf.mjs';
import { publicVectorFixture } from './helpers/native-vector-output.mjs';

const reference = JSON.parse(gunzipSync(readFileSync(new URL('./fixtures/pdf-reference.json.gz', import.meta.url))));
for (const fixture of reference.cases) {
  fixture.expected = publicVectorFixture(Buffer.from(fixture.expected, 'latin1')).toString('latin1');
}
const layouts = ['stacked', 'cutouts', 'grouped-cutouts'];
function inputFor(fixture) {
  const shape = reference.geometries[fixture.geometry];
  const hiddenRegions = shape.geometry.regions.flatMap((_, region) =>
    region < fixture.hiddenSize && (fixture.hiddenWords[region >>> 5] & (1 << (region & 31))) ? [region] : []);
  return { ...shape, layout:layouts[fixture.shapeMode], thinStroke:fixture.thinStroke,
    outputWidth:fixture.outputWidth, outputHeight:fixture.outputHeight, hiddenRegions };
}

test('PDF matches all 22014 reference files with OpenVector metadata, opacity, stream lengths and xref bytes', () => {
  assert.equal(reference.cases.length, 22014);
  assert.equal(reference.geometries.length, 3264);
  const resources = new Set();
  for (const [index, fixture] of reference.cases.entries()) {
    const actual = exportPdf(inputFor(fixture));
    assert.ok(actual instanceof Uint8Array);
    assert.equal(Buffer.from(actual).toString('latin1'), fixture.expected, `original PDF ${index}`);
    for (const match of fixture.expected.matchAll(/\/Vmt(\d+) <<\/CA/g)) resources.add(Number(match[1]));
  }
  assert.deepEqual([...resources].sort((a, b) => a - b), Array.from({ length:245 }, (_, index) => index + 11));
});

test('PDF defaults return owned bytes and preserve caller geometry', () => {
  const input = structuredClone(inputFor(reference.cases[0]));
  for (const key of ['layout', 'thinStroke', 'outputWidth', 'outputHeight', 'hiddenRegions']) delete input[key];
  const before = structuredClone(input), first = exportPdf(input), second = exportPdf(input);
  assert.equal(Buffer.from(first).toString('latin1'), reference.cases[0].expected);
  assert.deepEqual(input, before);
  first.fill(0);
  assert.equal(Buffer.from(second).toString('latin1'), reference.cases[0].expected);
  assert.equal(Buffer.from(exportPdf({ ...input, hiddenRegions:[0, 0] })).toString('latin1'),
    Buffer.from(exportPdf({ ...input, hiddenRegions:[0] })).toString('latin1'));
});

test('PDF object offsets address physical bytes and declared content length leaves the native LF delimiter', () => {
  const selected = [reference.cases[0], reference.cases.find(frame => frame.expected.includes('/Vmt128')),
    reference.cases.find(frame => frame.expected.includes('#J'))];
  for (const fixture of selected) {
    const pdf = Buffer.from(exportPdf(inputFor(fixture))).toString('latin1');
    const xref = Number(/startxref\r\n(\d+)\r\n%%EOF/.exec(pdf)[1]);
    assert.equal(pdf.slice(xref, xref + 6), 'xref\r\n');
    const rows = pdf.slice(xref).split('\r\n');
    assert.equal(rows[1], '0 9');
    assert.equal(rows[2], '0000000000 65535 f');
    for (let number = 1; number <= 8; number++) {
      assert.match(rows[number + 2], /^\d{10} 00000 n$/);
      const offset = Number(rows[number + 2].slice(0, 10));
      assert.ok(pdf.startsWith(`${number} 0 obj\r\n`, offset));
    }
    const contentStart = pdf.indexOf('stream\r\n') + 8;
    const length = Number(/6 0 obj\r\n(\d+)\r\nendobj/.exec(pdf)[1]);
    assert.ok(pdf.startsWith('\nendstream\r\n', contentStart + length));
    const source = reference.geometries[fixture.geometry];
    assert.ok(pdf.includes(`/MediaBox [0 0 ${source.width} ${source.height}]`));
  }
});

test('PDF rejects invalid dimensions, options and fitted references before returning bytes', () => {
  const valid = inputFor(reference.cases[0]);
  for (const patch of [{ width:0 }, { height:2 ** 31 }, { outputWidth:undefined }, { layout:'unknown' },
    { thinStroke:1 }, { hiddenRegions:[-1] }, { hiddenRegions:[valid.geometry.regions.length] }, { geometry:null }]) {
    assert.throws(() => exportPdf({ ...valid, ...patch }), /PDF/);
  }
  const geometry = structuredClone(valid.geometry); geometry.nodes[0].x = NaN;
  assert.throws(() => exportPdf({ ...valid, geometry }), /PDF/);
});
