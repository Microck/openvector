import assert from 'node:assert/strict';
import { createReadStream, readFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { createGunzip } from 'node:zlib';
import test from 'node:test';
import { decodeImage, resolveImageFilename } from '../src/image-input.mjs';
import { prepareImageState } from '../src/image-pipeline.mjs';

test('independent encoded-image decoding and source preparation match original BMP/PNG bytes', async () => {
  const rows = createInterface({ input:createReadStream(new URL('./fixtures/image-input-reference.ndjson.gz', import.meta.url))
    .pipe(createGunzip()),crlfDelay:Infinity });
  let cases = 0, opaque = 0, transparent = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical,true); continue; }
    const encoded = Uint8Array.from(Buffer.from(frame.encoded,'base64')),before = encoded.slice();
    const decoded = decodeImage(encoded);
    assert.equal(decoded.format,frame.format,frame.id);
    assert.equal(decoded.hasAlpha,Boolean(frame.decoded.matte),frame.id);
    assert.deepEqual([decoded.width,decoded.height],[frame.decoded.width,frame.decoded.height],frame.id);
    assert.deepEqual(Buffer.from(decoded.bgra),Buffer.from(frame.decoded.bgra,'base64'),`${frame.id} decoded`);
    const initial = prepareImageState({ ...decoded,mode:0,amount:0,parameters:{ 'Shared::is_anti_aliased':frame.antiAlias } });
    assert.deepEqual(Buffer.from(initial.source.bgra),Buffer.from(frame.source.bgra,'base64'),`${frame.id} source setter`);
    const image = prepareImageState({ ...decoded,mode:frame.mode,amount:frame.amount,
      parameters:{ 'Shared::is_anti_aliased':frame.antiAlias } });
    assert.equal(frame.source.matte,1,frame.id);assert.equal(frame.prepared.matte,1,frame.id);
    assert.deepEqual([image.source.width,image.source.height],[frame.prepared.width,frame.prepared.height],frame.id);
    assert.deepEqual(Buffer.from(image.source.bgra),Buffer.from(frame.prepared.bgra,'base64'),`${frame.id} prepared source`);
    assert.deepEqual([image.working.width,image.working.height],[frame.working.width,frame.working.height],frame.id);
    assert.deepEqual(Buffer.from(image.working.bgra),Buffer.from(frame.working.bgra,'base64'),`${frame.id} working`);
    assert.deepEqual(encoded,before,frame.id);assert.notEqual(decoded.bgra,encoded,frame.id);
    cases++;opaque += frame.decoded.matte === 0;transparent += frame.decoded.matte === 1;
  }
  assert.deepEqual({cases,opaque,transparent},{cases:337,opaque:138,transparent:199});
});


test('filename metadata matches actual native MainWindow loads', () => {
  const reference = JSON.parse(readFileSync(new URL('./fixtures/image-filename-reference.json',import.meta.url),'utf8'));
  assert.equal(reference.wholeFreshProcessResponseRepeatIdentical,true);
  assert.equal(reference.cases.length,15);
  for (const {filename,extension,shadowControl} of reference.cases)
    assert.deepEqual(resolveImageFilename(filename),{extension,shadowControl},filename);
  assert.throws(() => resolveImageFilename(undefined),/Image filename must be a string/);
});

