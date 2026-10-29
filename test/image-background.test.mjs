import assert from 'node:assert/strict';
import { createReadStream } from 'node:fs';
import { createInterface } from 'node:readline';
import { createBrotliDecompress } from 'node:zlib';
import test from 'node:test';
import { compositeSourceImage, prepareImagePixels, enhancePreparedImage,
  reducePreparedNoise, despecklePreparedImage } from '../src/image-preparation.mjs';
import { reduceWorkingImage } from '../src/image-resize.mjs';

test('background composition, filtered working state and source restoration match original document bytes', async () => {
  const rows = createInterface({ input:createReadStream(new URL('./fixtures/image-background-reference.ndjson.br',import.meta.url))
    .pipe(createBrotliDecompress()),crlfDelay:Infinity });
  let cases = 0, exhaustive = 0, large = 0, disabled = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical,true); continue; }
    const { width,height,mode,amount } = frame;
    const bgra = Uint8Array.from(Buffer.from(frame.bgra,'base64')), before = bgra.slice();
    const backup = prepareImagePixels({ width,height,bgra,mode:0,amount:0 }), backupBefore = backup.bgra.slice();
    assert.deepEqual([backup.width,backup.height],[frame.backup.width,frame.backup.height],frame.id);
    assert.deepEqual(Buffer.from(backup.bgra),Buffer.from(frame.backup.bgra,'base64'),`${frame.id} backup`);
    const color = Float32Array.from(frame.background), colorBefore = color.slice();
    assert.equal(Buffer.from(color.buffer).toString('hex'),frame.backgroundBits,frame.id);
    const composed = compositeSourceImage(backup,{ enabled:frame.enabled,color });
    assert.deepEqual([composed.width,composed.height],[frame.composed.width,frame.composed.height],frame.id);
    assert.deepEqual(Buffer.from(composed.bgra),Buffer.from(frame.composed.bgra,'base64'),`${frame.id} composed`);
    assert.notEqual(composed.bgra,backup.bgra,frame.id);
    let prepared = composed;
    if (mode === 1) for (let pass = 0; pass < amount; pass++) prepared = enhancePreparedImage(prepared);
    else if (mode === 2) prepared = reducePreparedNoise({ ...prepared,amount });
    else if (mode === 3) prepared = despecklePreparedImage(prepared);
    assert.deepEqual([prepared.width,prepared.height],[frame.prepared.width,frame.prepared.height],frame.id);
    assert.deepEqual(Buffer.from(prepared.bgra),Buffer.from(frame.prepared.bgra,'base64'),`${frame.id} prepared`);
    const working = reduceWorkingImage({ ...prepared,antiAlias:frame.antiAlias,matte:1 });
    assert.deepEqual([working.width,working.height],[frame.working.width,frame.working.height],frame.id);
    assert.deepEqual(Buffer.from(working.bgra),Buffer.from(frame.working.bgra,'base64'),`${frame.id} working`);
    const restored = compositeSourceImage(backup,{ enabled:false,color });
    assert.deepEqual([restored.width,restored.height],[frame.restored.width,frame.restored.height],frame.id);
    assert.deepEqual(Buffer.from(restored.bgra),Buffer.from(frame.restored.bgra,'base64'),`${frame.id} restored`);
    assert.notEqual(restored.bgra,backup.bgra,frame.id);
    for (const cache of ['composed','prepared','restored']) assert.equal(frame[cache].matte,1,frame.id);
    assert.deepEqual([frame.composedValidity,frame.preparedValidity,frame.restoredValidity],[0,Number(mode !== 0),0],frame.id);
    assert.deepEqual(backup.bgra,backupBefore,frame.id);assert.deepEqual(bgra,before,frame.id);
    assert.deepEqual(color,colorBefore,frame.id);assert.equal(frame.randomState,1,frame.id);
    cases++;exhaustive += frame.id.startsWith('exhaustive');large += frame.id.startsWith('large');disabled += !frame.enabled;
  }
  assert.deepEqual({cases,exhaustive,large,disabled},{cases:636,exhaustive:28,large:8,disabled:40});
});
