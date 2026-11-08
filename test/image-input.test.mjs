import assert from 'node:assert/strict';
import { createReadStream, readFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { createGunzip } from 'node:zlib';
import test from 'node:test';
import { decodeImage, resolveImageFilename } from '../src/image-input.mjs';
import { prepareImageState } from '../src/image-pipeline.mjs';

// Original scratch output is not an oracle for this structural PNG class.
function correctAdam7(encoded) {
  return encoded.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10]))
    && encoded[24] === 16 && [2,6].includes(encoded[25]) && encoded[28] === 1;
}

function adam7SamplePixels(frame, encoded) {
  assert.equal(correctAdam7(encoded),true);
  const channels = encoded[25] === 2 ? 3 : 4;
  const samples = frame.writer.samples;
  assert.equal(samples.length,frame.width * frame.height * channels);
  let key;
  for (let offset = 8; offset < encoded.length; offset += encoded.readUInt32BE(offset) + 12) {
    if (encoded.toString('ascii',offset + 4,offset + 8) === 'tRNS') {
      key = [0,2,4].map(channel => encoded.readUInt16BE(offset + 8 + channel));
    }
  }
  const bgra = Buffer.alloc(frame.width * frame.height * 4);
  for (let source = 0, target = 0; source < samples.length; source += channels, target += 4) {
    bgra[target] = samples[source + 2] >>> 8;
    bgra[target + 1] = samples[source + 1] >>> 8;
    bgra[target + 2] = samples[source] >>> 8;
    bgra[target + 3] = channels === 4 ? samples[source + 3] >>> 8
      : key?.every((value,channel) => value === samples[source + channel]) ? 0 : 255;
  }
  return bgra;
}

function adam7Preparation(frame, decoded, expected) {
  assert.deepEqual([frame.mode,frame.amount],[0,0]);
  const preparedPixels = Buffer.from(expected);
  for (let p = 0; p < expected.length; p += 4) {
    const alpha = expected[p + 3] * (1 / 255);
    for (let channel = 0; channel < 3; channel++) preparedPixels[p + channel] = expected[p + channel] * alpha;
  }
  for (const antiAlias of [0,1]) {
    const image = prepareImageState({ ...decoded,mode:0,amount:0,parameters:{ 'Shared::is_anti_aliased':antiAlias } });
    for (const actual of [image.source,image.working]) {
      assert.deepEqual([actual.width,actual.height],[frame.width,frame.height],frame.id);
      assert.deepEqual(Buffer.from(actual.bgra),preparedPixels,`${frame.id} prepared samples`);
    }
  }
}

test('encoded-image native parity and defined Adam7 sample correctness', async () => {
  const rows = createInterface({ input:createReadStream(new URL('./fixtures/image-input-reference.ndjson.gz', import.meta.url))
    .pipe(createGunzip()),crlfDelay:Infinity });
  let cases = 0, opaque = 0, transparent = 0, definedAdam7 = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical,true); continue; }
    const encoded = Buffer.from(frame.encoded,'base64'),before = Buffer.from(encoded);
    const decoded = decodeImage(encoded);
    assert.equal(decoded.format,frame.format,frame.id);
    assert.equal(decoded.hasAlpha,Boolean(frame.decoded.matte),frame.id);
    assert.deepEqual([decoded.width,decoded.height],[frame.decoded.width,frame.decoded.height],frame.id);
    if (correctAdam7(encoded)) {
      const expected = adam7SamplePixels(frame,encoded);
      assert.deepEqual(Buffer.from(decoded.bgra),expected,`${frame.id} sample-domain decode`);
      adam7Preparation(frame,decoded,expected);
      definedAdam7++;
    } else {
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
    }
    assert.deepEqual(encoded,before,frame.id);assert.notEqual(decoded.bgra,encoded,frame.id);
    cases++;opaque += frame.decoded.matte === 0;transparent += frame.decoded.matte === 1;
  }
  assert.deepEqual({cases,opaque,transparent},{cases:337,opaque:138,transparent:199});
  assert.equal(definedAdam7,16);
});

test('authored Adam7 PNG filters, channel order, alpha and full-depth transparency', () => {
  const fixture = JSON.parse(readFileSync(new URL('./fixtures/png-adam7-correctness.json',import.meta.url),'utf8'));
  for (const frame of fixture.cases) {
    const encoded = Buffer.from(frame.encoded,'base64'), before = Buffer.from(encoded);
    const decoded = decodeImage(encoded);
    assert.deepEqual([decoded.width,decoded.height],[frame.width,frame.height],frame.id);
    assert.equal(decoded.hasAlpha,frame.writer.colorType === 6 || frame.writer.transparentKey,frame.id);
    const expected = adam7SamplePixels(frame,encoded);
    assert.deepEqual(Buffer.from(decoded.bgra),expected,`${frame.id} authored samples`);
    adam7Preparation(frame,decoded,expected);
    decoded.bgra.fill(0);
    assert.deepEqual(encoded,before,frame.id);
    assert.deepEqual(Buffer.from(decodeImage(encoded).bgra),expected,`${frame.id} repeat decode`);
  }
});


test('filename metadata matches actual native MainWindow loads', () => {
  const reference = JSON.parse(readFileSync(new URL('./fixtures/image-filename-reference.json',import.meta.url),'utf8'));
  assert.equal(reference.wholeFreshProcessResponseRepeatIdentical,true);
  assert.equal(reference.cases.length,15);
  for (const {filename,extension,shadowControl} of reference.cases)
    assert.deepEqual(resolveImageFilename(filename),{extension,shadowControl},filename);
  assert.throws(() => resolveImageFilename(undefined),/Image filename must be a string/);
});

