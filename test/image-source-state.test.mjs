import assert from 'node:assert/strict';
import { createReadStream } from 'node:fs';
import { createInterface } from 'node:readline';
import { createGunzip } from 'node:zlib';
import test from 'node:test';
import { prepareImageState, classifyImageState } from '../src/image-pipeline.mjs';
import { blurClassifierPixels, measureClassifierGradients } from '../src/image-classification.mjs';

test('independent source blur and distinct-stride working gradients match native state', async () => {
  const rows = createInterface({ input:createReadStream(new URL('./fixtures/image-source-gradients-reference.ndjson.gz', import.meta.url))
    .pipe(createGunzip()), crlfDelay:Infinity });
  let cases = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical, true); continue; }
    const bgra = Uint8Array.from(Buffer.from(frame.bgra, 'base64')), before = bgra.slice();
    const { source, working } = prepareImageState({ ...frame, bgra, hasAlpha:true,
      parameters:{ 'Shared::is_anti_aliased':frame.antiAlias } });
    assert.notEqual(source.bgra, bgra, frame.id);
    assert.notEqual(working.bgra, source.bgra, frame.id);
    assert.deepEqual(Buffer.from(source.bgra), Buffer.from(frame.preparedBgra, 'base64'), frame.id);
    assert.deepEqual(Buffer.from(working.bgra), Buffer.from(frame.workingBgra, 'base64'), frame.id);
    const blurredBgra = blurClassifierPixels(source), blurred = { ...source, bgra:blurredBgra };
    assert.deepEqual(Buffer.from(blurredBgra), Buffer.from(frame.blurredBgra, 'base64'), frame.id);
    const gradients = measureClassifierGradients(working, blurred);
    for (const [index, key] of ['gradient','blurGradient'].entries()) {
      assert.deepEqual(Buffer.from(gradients[key].buffer), Buffer.from(frame.gradientBits[index], 'base64'), `${frame.id} ${key}`);
    }
    assert.deepEqual(bgra, before, frame.id);
    assert.deepEqual(Buffer.from(source.bgra), Buffer.from(frame.preparedBgra, 'base64'), frame.id);
    assert.deepEqual(Buffer.from(working.bgra), Buffer.from(frame.workingBgra, 'base64'), frame.id);
    cases++;
  }
  assert.equal(cases, 5);
});

test('fresh source/working state reproduces complete native large-image classification', async () => {
  const rows = createInterface({ input:createReadStream(new URL('./fixtures/image-source-state-reference.ndjson.gz', import.meta.url))
    .pipe(createGunzip()), crlfDelay:Infinity });
  let cases = 0, reduced = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical, true); continue; }
    const bgra = Uint8Array.from(Buffer.from(frame.bgra, 'base64')), before = bgra.slice();
    const image = prepareImageState({ ...frame, bgra, hasAlpha:true });
    assert.deepEqual([image.source.width,image.source.height], frame.preparedDimensions, frame.id);
    assert.deepEqual([image.working.width,image.working.height], frame.workingDimensions, frame.id);
    assert.deepEqual(Buffer.from(image.source.bgra), Buffer.from(frame.preparedBgra, 'base64'), frame.id);
    assert.deepEqual(Buffer.from(image.working.bgra), Buffer.from(frame.workingBgra, 'base64'), frame.id);
    const analyzed = classifyImageState({ image, extension:frame.extension, parameters:frame.parameters });
    assert.deepEqual(Buffer.from(analyzed.features.buffer), Buffer.from(frame.featureBits, 'hex'), `${frame.id} features`);
    assert.deepEqual(analyzed.classification, frame.expected, `${frame.id} type/quality`);
    assert.deepEqual([analyzed.blurred.width,analyzed.blurred.height], frame.blurDimensions, frame.id);
    assert.deepEqual(Buffer.from(analyzed.blurred.bgra), Buffer.from(frame.blurredBgra, 'base64'), `${frame.id} source blur`);
    for (const [index,key] of ['gradient','blurGradient'].entries()) {
      assert.deepEqual(Buffer.from(analyzed[key].buffer), Buffer.from(frame.gradientBits[index], 'base64'), `${frame.id} ${key}`);
    }
    assert.deepEqual(Buffer.from(analyzed.measurement), Buffer.from(frame.measurement, 'base64'), `${frame.id} contrast`);
    assert.deepEqual(Buffer.from(analyzed.regions.indices.buffer), Buffer.from(frame.indices, 'base64'), `${frame.id} regions`);
    assert.equal(analyzed.regions.counts.length, frame.regionMomentBits.length, frame.id);
    for (const [region,bits] of frame.regionMomentBits.entries()) {
      const moments = Buffer.alloc(20);
      moments.writeInt32LE(analyzed.regions.counts[region]);
      Buffer.from(analyzed.regions.sums.buffer,region*16,16).copy(moments,4);
      assert.deepEqual(moments, Buffer.from(bits, 'hex'), `${frame.id} region ${region}`);
    }
    assert.deepEqual(bgra, before, frame.id);
    assert.deepEqual(Buffer.from(image.source.bgra), Buffer.from(frame.preparedBgra, 'base64'), frame.id);
    assert.deepEqual(Buffer.from(image.working.bgra), Buffer.from(frame.workingBgra, 'base64'), frame.id);
    reduced += Number(image.working.width !== image.source.width || image.working.height !== image.source.height);
    cases++;
  }
  assert.equal(cases, 16);
  assert.equal(reduced, 12);
});
