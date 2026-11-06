import assert from 'node:assert/strict';
import test from 'node:test';
import { exportEmf } from '../src/emf.mjs';
import { exportPdf } from '../src/pdf.mjs';
import { rectangle } from './helpers/authored-rectangle.mjs';

test('EMF description contains only OpenVector labels and zero padding', () => {
  const bytes = Buffer.from(exportEmf({ ...rectangle(), referenceDevice: {
    pixels: [1920, 1080], millimeters: [508, 286], micrometers: [508000, 286000],
  } }));
  assert.equal(bytes.readUInt32LE(0), 1);
  assert.equal(bytes.readUInt32LE(4), 236);
  assert.equal(bytes.toString('ascii', 40, 44), ' EMF');
  assert.equal(bytes.readUInt32LE(48), bytes.length);
  assert.equal(bytes.readUInt32LE(60), 64);
  assert.equal(bytes.readUInt32LE(64), 108);
  const expected = Buffer.alloc(128);
  expected.write('OpenVector\0OpenVector\0\0', 0, 'utf16le');
  assert.deepEqual(bytes.subarray(108, 236), expected);
});

test('PDF branding preserves valid object and cross-reference offsets', () => {
  const pdf = Buffer.from(exportPdf(rectangle())).toString('latin1');
  assert.match(pdf, /2 0 obj\r\n<<\/Producer\(OpenVector\) {5}\r\n>>endobj/);
  assert.match(pdf, /\/MediaBox \[0 0 4 3\]/);
  const start = /startxref\r\n(\d+)\r\n%%EOF/.exec(pdf);
  assert.ok(start, 'PDF has a startxref field');
  const rows = pdf.slice(Number(start[1])).split('\r\n');
  assert.equal(rows[0], 'xref');
  assert.equal(rows[1], '0 9');
  assert.equal(rows[2], '0000000000 65535 f');
  for (let object = 1; object <= 8; object++) {
    assert.match(rows[object + 2], /^\d{10} 00000 n$/);
    const offset = Number(rows[object + 2].slice(0, 10));
    assert.ok(pdf.startsWith(`${object} 0 obj\r\n`, offset), `object ${object} offset`);
  }
});
