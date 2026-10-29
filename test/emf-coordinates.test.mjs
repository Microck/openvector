import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createReadStream } from 'node:fs';
import { createGunzip } from 'node:zlib';
import { createInterface } from 'node:readline';
import { transformEmfPoint } from '../src/vector-output.mjs';

test('EMF integer coordinates match all 277909 original storage-boundary pairs', async () => {
  const path = new URL('./fixtures/emf-coordinates-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let count = 0, summary;
  for await (const line of rows) {
    const fixture = JSON.parse(line);
    if (fixture.type === 'metadata') {
      assert.equal(fixture.sourceSha256, 'c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1');
      assert.equal(fixture.wholeNativeResponseRepeatIdentical, true);
    } else if (fixture.type === 'summary') summary = fixture;
    else {
      assert.equal(fixture.type, 'case');
      const point = fixture.pointBits.map(bits => Buffer.from(bits, 'hex').readDoubleLE());
      assert.deepEqual(transformEmfPoint(fixture, ...point), fixture.expected, `original EMF pair ${count}`);
      count++;
    }
  }
  assert.equal(count, 277909);
  assert.deepEqual(summary, { type:'summary', cases:count });
});
