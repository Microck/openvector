import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import test from 'node:test';
import { enhancePreparedImage, prepareEnhancedImage, reducePreparedNoise,
  despecklePreparedImage, prepareImagePixels } from '../src/image-preparation.mjs';
import { blurClassifierPixels } from '../src/image-classification.mjs';
import { classifyImageState } from '../src/image-pipeline.mjs';

for (const scope of ['kernel', 'preparation']) {
  test(`native EnhanceImage ${scope}: exact bytes, dimensions and input ownership`, () => {
    const frames = gunzipSync(readFileSync(new URL(`./fixtures/image-enhance-${scope}-reference.ndjson.gz`, import.meta.url)))
      .toString().trim().split('\n').map(line => JSON.parse(line));
    assert.equal(frames[0].wholeNativeResponseRepeatIdentical, true);
    assert.equal(frames.length - 1, frames[0].cases);
    for (const frame of frames.slice(1)) {
      const bgra = Uint8Array.from(frame.bgra), before = bgra.slice();
      const input = { width:frame.width, height:frame.height, bgra, amount:frame.amount };
      const prepared = scope === 'kernel' ? enhancePreparedImage(input) : prepareEnhancedImage(input);
      assert.deepEqual([prepared.width, prepared.height], frame.expectedDimensions, frame.id);
      assert.deepEqual(prepared.bgra, Uint8Array.from(frame.expected), frame.id);
      assert.deepEqual(bgra, before, frame.id);
      assert.notEqual(prepared.bgra, bgra, frame.id);
      if (frame.blurredBgra) assert.deepEqual(blurClassifierPixels(prepared), Uint8Array.from(frame.blurredBgra), frame.id);
    }
  });
}

for (const scope of ['kernel', 'preparation']) {
  test(`native median/despeckle ${scope}: exact dispatch, bytes and ownership`, () => {
    const frames = gunzipSync(readFileSync(new URL(`./fixtures/image-filters-${scope}-reference.ndjson.gz`, import.meta.url)))
      .toString().trim().split('\n').map(line => JSON.parse(line));
    assert.equal(frames[0].wholeNativeResponseRepeatIdentical, true);
    assert.equal(frames.length - 1, frames[0].cases);
    for (const frame of frames.slice(1)) {
      const bgra = Uint8Array.from(frame.bgra), before = bgra.slice(), input = { ...frame, bgra };
      const prepared = scope === 'preparation' ? prepareImagePixels(input)
        : frame.mode === 2 ? reducePreparedNoise(input) : despecklePreparedImage(input);
      assert.deepEqual([prepared.width, prepared.height], frame.expectedDimensions, frame.id);
      assert.deepEqual(prepared.bgra, Uint8Array.from(frame.expected), frame.id);
      assert.deepEqual(bgra, before, frame.id); assert.notEqual(prepared.bgra, bgra, frame.id);
      if (frame.blurredBgra) assert.deepEqual(blurClassifierPixels(prepared), Uint8Array.from(frame.blurredBgra), frame.id);
    }
  });
}

test('median size preconditions preserve every native ErrorOption reason', () => {
  const reference = JSON.parse(readFileSync(new URL('./fixtures/image-filters-errors-reference.json', import.meta.url), 'utf8'));
  assert.equal(reference.stableErrorTypeMessageStageRepeatIdentical, true);
  assert.equal(reference.cases.length, 176);
  for (const frame of reference.cases) {
    const bgra = Uint8Array.from(frame.bgra), before = bgra.slice(), input = { ...frame, bgra };
    assert.equal(frame.descriptor, '.?AVErrorOption@Magick@@');
    assert.equal(frame.reason, 'ImageSmallerThanKernelRadius');
    assert.throws(() => frame.operation === 85 ? reducePreparedNoise(input) : prepareImagePixels(input),
      { name:'RangeError', message:frame.reason }, frame.id);
    assert.deepEqual(bgra, before, frame.id);
  }
});

for (const fixture of ['image-features', 'image-filters-classification']) {
  test(`${fixture}: decoded-pixel preparation composes with complete native classification`, () => {
    const contextName = fixture === 'image-features' ? 'classifier-context' : 'image-filters-classifier-context';
    const context = JSON.parse(readFileSync(new URL(`./fixtures/${contextName}-reference.json`, import.meta.url), 'utf8'));
    const contexts = new Map(context.cases.map(frame => [frame.id, frame.parameters]));
    const frames = gunzipSync(readFileSync(new URL(`./fixtures/${fixture}-reference.ndjson.gz`, import.meta.url)))
      .toString().trim().split('\n').map(line => JSON.parse(line));
    let completed = 0, empty = 0;
    const names = ['use_over_seg_diag_extr','max_iterations','lambda_pre','lambda_initial','lambda_final',
      'min_num_pixels','num_iter_btwn_renum','rise_fraction','size_penalty_weight','size_penalty_power'];
    for (const frame of frames) {
      if (frame.type !== 'case') continue;
      const input = frame.input, bgra = Uint8Array.from(input.bgra), before = bgra.slice();
      const prepared = prepareImagePixels({ ...input, bgra, mode:input.preprocessingMode, amount:input.preprocessingAmount });
      assert.deepEqual([prepared.width, prepared.height], frame.dimensions.slice(4), input.id);
      assert.deepEqual(bgra, before, input.id);
      if (frame.outcome === 'empty-working-image') { assert.equal(prepared.bgra.length, 0); empty++; continue; }
      assert.deepEqual(Array.from(prepared.bgra), frame.workingBgra, input.id);
      const bytes = Buffer.from(frame.segmentationParameterBits, 'hex');
      const parameters = Object.assign(Object.fromEntries(names.map((name,index) => [`Segmenter::${name}`,
        [0,1,5,6].includes(index) ? bytes.readInt32LE(index*4) : bytes.readFloatLE(index*4)])), contexts.get(input.id));
      const analyzed = classifyImageState({ image:{ source:prepared, working:prepared }, extension:input.extension, parameters });
      assert.deepEqual(Buffer.from(analyzed.features.buffer), Buffer.from(frame.featureBits.join(''), 'hex'), input.id);
      assert.deepEqual(analyzed.classification, frame.expected, input.id);
      assert.deepEqual(Array.from(analyzed.regions.indices), frame.indices, input.id);
      assert.deepEqual(Array.from(analyzed.blurred.bgra), frame.blurredBgra, input.id);
      assert.deepEqual(Buffer.from(analyzed.gradient.buffer), Buffer.from(frame.gradientBits[0].join(''), 'hex'), input.id);
      assert.deepEqual(Buffer.from(analyzed.blurGradient.buffer), Buffer.from(frame.gradientBits[1].join(''), 'hex'), input.id);
      assert.deepEqual(Array.from(analyzed.measurement), frame.measurement, input.id);
      assert.equal(analyzed.regions.counts.length, frame.regionMomentBits.length);
      for (const [region,bits] of frame.regionMomentBits.entries()) {
        const moments = Buffer.alloc(20); moments.writeInt32LE(analyzed.regions.counts[region]);
        Buffer.from(analyzed.regions.sums.buffer, region*16, 16).copy(moments, 4);
        assert.deepEqual(moments, Buffer.from(bits, 'hex'), input.id);
      }
      completed++;
    }
    assert.equal(completed, fixture === 'image-features' ? 72 : 123);
    assert.equal(empty, fixture === 'image-features' ? 24 : 0);
  });
}
