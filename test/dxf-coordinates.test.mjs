import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createReadStream } from 'node:fs';
import { createGunzip } from 'node:zlib';
import { createInterface } from 'node:readline';
import { transformDxfPoint, formatDxfCoordinate } from '../src/vector-output.mjs';
import { fixedDecimal } from '../src/decimal-format.mjs';

test('DXF coordinates, double bits and emitted numeric fields match all 126984 original pairs', async () => {
  const path = new URL('./fixtures/dxf-coordinates-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  const bytes = Buffer.alloc(8);
  let count = 0, overlongFields = 0, nonfiniteFields = 0, summary;
  for await (const line of rows) {
    const fixture = JSON.parse(line);
    if (fixture.type === 'metadata') {
      assert.equal(fixture.sourceSha256, 'c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1');
    } else if (fixture.type === 'summary') {
      summary = fixture;
    } else {
      assert.equal(fixture.type, 'case');
      const coordinates = transformDxfPoint(fixture, ...fixture.point);
      for (const [axis, value] of coordinates.entries()) {
        bytes.writeDoubleLE(value);
        assert.equal(bytes.toString('hex'), fixture.expected.bits[axis], `original DXF pair ${count}, axis ${axis}`);
        assert.equal(fixedDecimal(value, 16), fixture.expected.fields[axis], `original DXF raw field ${count}, axis ${axis}`);
        if (fixture.expected.emitted[axis] === null) {
          assert.throws(() => formatDxfCoordinate(value), /DXF.*256-byte/);
          overlongFields++;
        } else {
          assert.equal(formatDxfCoordinate(value), fixture.expected.emitted[axis], `original DXF emitted field ${count}, axis ${axis}`);
        }
        if (!Number.isFinite(value)) nonfiniteFields++;
      }
      count++;
    }
  }
  assert.equal(count, 126984);
  assert.deepEqual(summary, { type:'summary', cases:count, overlongFields, nonfiniteFields });
  assert.equal(overlongFields, 98); assert.equal(nonfiniteFields, 109);
});

test('DXF transform preserves all 30 original infinity and NaN input pairs', async () => {
  const path = new URL('./fixtures/dxf-special-coordinates-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  const bytes = Buffer.alloc(8);
  let count = 0, summary;
  for await (const line of rows) {
    const fixture = JSON.parse(line);
    if (fixture.type === 'summary') summary = fixture;
    else if (fixture.type === 'case') {
      const point = fixture.pointBits.map(bits => Buffer.from(bits, 'hex').readDoubleLE());
      const coordinates = transformDxfPoint(fixture, ...point);
      for (const [axis, value] of coordinates.entries()) {
        bytes.writeDoubleLE(value);
        assert.equal(bytes.toString('hex'), fixture.expected.bits[axis], `special DXF pair ${count}, axis ${axis}`);
        assert.equal(fixedDecimal(value, 16), fixture.expected.fields[axis]);
        assert.equal(formatDxfCoordinate(value), fixture.expected.emitted[axis]);
      }
      count++;
    } else assert.equal(fixture.type, 'metadata');
  }
  assert.equal(count, 30);
  assert.deepEqual(summary, { type:'summary', cases:30, overlongFields:0, nonfiniteFields:60 });
});
