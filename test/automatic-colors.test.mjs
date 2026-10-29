import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createReadStream, readFileSync } from 'node:fs';
import { createGunzip } from 'node:zlib';
import { createInterface } from 'node:readline';
import { buildPaletteFeatures, classifyPaletteFeatures } from '../src/automatic-colors.mjs';
import { CoreSession, scoreUnweightedPalette } from '../src/palette.mjs';
import { presegmentRegions, segment } from '../src/segmentation.mjs';
import { buildSubpixelStatistics, optimizeSubpixel, refineShadowState } from '../src/subpixel.mjs';
import { rebuildConnectedRegions } from '../src/region-data.mjs';
import { scoreWeightedPalette } from '../src/blending.mjs';
import { classifyImageFeatures, measureClassifierGradients, measureClassifierEdges, sampleClassifierEdge,
  measureClassifierRegions, measureClassifierColors, blurClassifierPixels, premultiplyClassifierPixels } from '../src/image-classification.mjs';
import { acceptShadowFeatures, classifyShadowFeatures, principalColorDirection, measureShadowFeatures,
  reassignShadowPixels, writeShadowProposal } from '../src/shadow-classification.mjs';
import { classifyImageState } from '../src/image-pipeline.mjs';

test('palette-count model and label decoding match all 31604 original feature vectors', async () => {
  const path = new URL('./fixtures/classification-model-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let count = 0, summary;
  const labels = {}, profiles = new Set();
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {
      assert.equal(frame.sourceSha256, 'c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1');
      assert.equal(frame.wholeNativeResponseRepeatIdentical, true);
    } else if (frame.type === 'summary') summary = frame;
    else {
      assert.equal(frame.type, 'case');
      const features = frame.featureBits.map(bits => Buffer.from(bits, 'hex').readDoubleLE());
      assert.equal(features.length, 62);
      assert.deepEqual(classifyPaletteFeatures(features), frame.expected, `original palette classifier ${count}`);
      labels[frame.expected.label] = (labels[frame.expected.label] ?? 0) + 1;
      profiles.add(frame.profile); count++;
    }
  }
  assert.equal(count, 31604); assert.equal(profiles.size, 10);
  assert.deepEqual(summary, { type:'summary', cases:count, labels });
  assert.deepEqual(Object.keys(labels).sort((a,b) => Number(a)-Number(b)), ['2','3','4','5','6','7','8','9','13']);
});

test('palette feature assembly matches every original double bit and retains native fatal cases', async () => {
  const path = new URL('./fixtures/classification-features-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  const decodeDoubles = bits => Float64Array.from(bits, value => Buffer.from(value, 'hex').readDoubleLE());
  let count = 0, fatalCases = 0, summary;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {
      assert.equal(frame.completedOutputRepeatIdentical, true);
      assert.equal(frame.nativeFaultsRepeatIdentical, true);
      continue;
    }
    if (frame.type === 'summary') { summary = frame; continue; }
    const candidates = frame.candidates.map(candidate => {
      const bytes = Buffer.from(candidate.scoreBits, 'hex');
      return { weights:decodeDoubles(candidate.weightBits), score:{ total:bytes.readFloatLE(),
        error:bytes.readFloatLE(4), colors:bytes.readFloatLE(8) } };
    });
    const build = () => buildPaletteFeatures(frame.input.bgra, candidates, decodeDoubles(frame.peakWeightBits));
    if (frame.expectedFault) {
      assert.deepEqual(frame.expectedFault, { code:'0xc0000005', instruction:'0x48cac2', stage:13 });
      assert.throws(build, /Automatic color selection has no histogram peaks/);
      fatalCases++;
    } else {
      const features = build();
      const expectedBytes = Buffer.from(frame.featureBits.join(''), 'hex');
      assert.deepEqual(Buffer.from(features.buffer), expectedBytes, frame.input.id);
      assert.deepEqual(classifyPaletteFeatures(features), frame.expected, frame.input.id);
    }
    count++;
  }
  assert.equal(count, 576);
  assert.equal(fatalCases, 306);
  assert.equal(summary.cases, count);
  assert.equal(summary.candidates, count * 11);
  assert.equal(summary.fatalCases, fatalCases);
});

test('independent session reproduces all native candidates, scores, features and retained random transitions', async () => {
  const path = new URL('./fixtures/classification-features-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let analyzer = new CoreSession(), generator = new CoreSession(), previousFault = false;
  let count = 0, generatedCount = 0, scoredCount = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type !== 'case') continue;
    if (previousFault) { analyzer = new CoreSession(); generator = new CoreSession(); }
    assert.equal(analyzer.randomState, frame.initialRandomState, frame.input.id);
    assert.equal(generator.randomState, frame.initialRandomState, frame.input.id);
    for (const candidate of frame.candidates) {
      const input = { ...frame.input, colorCount:candidate.colorCount };
      const actual = frame.input.antiAlias ? generator.generateWeighted(input) : generator.generateUnweighted(input);
      assert.deepEqual(Array.from(actual.palette), candidate.palette, `${frame.input.id} palette ${candidate.colorCount}`);
      assert.deepEqual(Array.from(actual.indices), candidate.indices, `${frame.input.id} indices ${candidate.colorCount}`);
      assert.deepEqual(Buffer.from(actual.weights.buffer), Buffer.from(candidate.weightBits.join(''), 'hex'),
        `${frame.input.id} weights ${candidate.colorCount}`);
      assert.equal(generator.randomState, candidate.randomState, `${frame.input.id} random ${candidate.colorCount}`);
      generatedCount++;
    }
    if (frame.expectedFault) {
      assert.throws(() => analyzer.analyzeColorCounts(frame.input), /Automatic color selection has no histogram peaks/);
    } else {
      const analyzed = analyzer.analyzeColorCounts(frame.input);
      assert.deepEqual(Buffer.from(analyzed.peakWeights.buffer), Buffer.from(frame.peakWeightBits.join(''), 'hex'), frame.input.id);
      assert.deepEqual(Buffer.from(analyzed.features.buffer), Buffer.from(frame.featureBits.join(''), 'hex'), frame.input.id);
      assert.deepEqual(analyzed.classification, frame.expected, frame.input.id);
      for (let index = 0; index < analyzed.candidates.length; index++) {
        const actual = analyzed.candidates[index], expected = frame.candidates[index];
        const scores = Buffer.alloc(12);
        scores.writeFloatLE(actual.score.total); scores.writeFloatLE(actual.score.error, 4); scores.writeFloatLE(actual.score.colors, 8);
        assert.equal(scores.toString('hex'), expected.scoreBits, `${frame.input.id} score ${actual.colorCount}`);
        assert.deepEqual(Array.from(actual.palette), expected.palette, `${frame.input.id} analyzed palette ${actual.colorCount}`);
        assert.deepEqual(Array.from(actual.indices), expected.indices, `${frame.input.id} analyzed indices ${actual.colorCount}`);
        assert.deepEqual(Buffer.from(actual.weights.buffer), Buffer.from(expected.weightBits.join(''), 'hex'),
          `${frame.input.id} analyzed weights ${actual.colorCount}`);
        assert.equal(actual.randomState, expected.randomState);
        scoredCount++;
      }
    }
    assert.equal(analyzer.randomState, frame.randomState, frame.input.id);
    previousFault = Boolean(frame.expectedFault);
    count++;
  }
  assert.equal(count, 576);
  assert.equal(generatedCount, 6336);
  assert.equal(scoredCount, 2970);
});

test('pre-quantization scoring matches all 6336 original raw candidates, including native fatal images', async () => {
  const rawPath = new URL('./fixtures/classification-raw-reference.ndjson.gz', import.meta.url);
  const rawRows = createInterface({ input:createReadStream(rawPath).pipe(createGunzip()), crlfDelay:Infinity });
  const rawPalettes = new Map();
  for await (const line of rawRows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {
      assert.equal(frame.wholeNativeResponseRepeatIdentical, true);
      assert.equal(frame.fullGenerationScoresMapsWeightsRandomAndFinalizedPalettesMatch, true);
    } else {
      assert.equal(frame.type, 'case');
      rawPalettes.set(frame.id, frame.rawPaletteBits);
    }
  }
  assert.equal(rawPalettes.size, 576);
  const path = new URL('./fixtures/classification-features-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let count = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type !== 'case') continue;
    const palettes = rawPalettes.get(frame.input.id);
    assert.equal(palettes.length, 11);
    for (let index = 0; index < frame.candidates.length; index++) {
      const native = frame.candidates[index], bytes = Buffer.from(palettes[index], 'hex');
      const palette = Float32Array.from({ length:bytes.length / 4 }, (_, channel) => bytes.readFloatLE(channel * 4));
      const input = { ...frame.input, palette, indices:native.indices };
      const score = input.antiAlias ? scoreWeightedPalette(input) : scoreUnweightedPalette(input);
      const actual = Buffer.alloc(12);
      actual.writeFloatLE(score.total); actual.writeFloatLE(score.error, 4); actual.writeFloatLE(score.colors, 8);
      assert.equal(actual.toString('hex'), native.scoreBits, `${input.id} ${native.colorCount}`);
      count++;
    }
  }
  assert.equal(count, 6336);
});

test('automatic analysis validates prepared-image controls before consuming the session random stream', () => {
  const input = { width:2, height:2, bgra:new Uint8Array(16), bins:8, connected:true, antiAlias:false, costPerColor:0.3 };
  const session = new CoreSession();
  for (const change of [{ width:0 }, { height:3 }, { bins:4 }, { connected:1 }, { antiAlias:1 },
    { costPerColor:Infinity }, { costPerColor:1e100 }, { width:1, height:4, antiAlias:true }]) {
    assert.throws(() => session.analyzeColorCounts({ ...input, ...change }));
    assert.equal(session.randomState, 1);
  }
});

test('image type/quality model matches all 57472 original float vectors and literal extension decisions', async () => {
  const path = new URL('./fixtures/image-classifier-model-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let count = 0, summary;
  const labels = {}, extensions = new Set(), finalExtensionDecisions = new Map();
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {
      assert.equal(frame.sourceSha256, 'c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1');
      assert.equal(frame.wholeNativeResponseRepeatIdentical, true);
    } else if (frame.type === 'summary') summary = frame;
    else {
      assert.equal(frame.type, 'case');
      const features = Float32Array.from(frame.featureBits, bits => Buffer.from(bits, 'hex').readFloatLE());
      assert.equal(features.length, 20);
      assert.deepEqual(classifyImageFeatures(features, frame.extension), frame.expected, `native image classifier ${count}`);
      labels[frame.expected.label] = (labels[frame.expected.label] ?? 0) + 1;
      extensions.add(frame.extension);
      if (count >= 57450) finalExtensionDecisions.set(frame.extension, frame.expected.label);
      count++;
    }
  }
  assert.equal(count, 57472); assert.equal(extensions.size, 22);
  assert.deepEqual(summary, { type:'summary', cases:count, labels, thresholdComparisons:65 });
  assert.deepEqual(Object.keys(labels).sort(), ['logo 2','logo_aa 0','logo_aa 1','logo_aa 2','photo 1']);
  assert.equal(finalExtensionDecisions.size, 22);
  for (const [extension, label] of finalExtensionDecisions) {
    assert.equal(label, ['.bmp','.jpeg','.png','.tiff'].includes(extension) ? 'logo_aa 1' : 'logo_aa 2');
  }
});


function assertClassifierFeatureStages(frame, edgeControls) {
  const [width, height] = frame.dimensions.slice(4);
  const blurredBgra = blurClassifierPixels({ width, height, bgra:frame.workingBgra });
  assert.deepEqual(Array.from(blurredBgra), frame.blurredBgra, `${frame.input.id} independent blur`);
  const actual = measureClassifierGradients({ width, height, bgra:frame.workingBgra }, { width, height, bgra:blurredBgra });
  assert.deepEqual(Buffer.from(actual.gradient.buffer), Buffer.from(frame.gradientBits[0].join(''), 'hex'), `${frame.input.id} gradient`);
  assert.deepEqual(Buffer.from(actual.blurGradient.buffer), Buffer.from(frame.gradientBits[1].join(''), 'hex'), `${frame.input.id} blurred gradient`);
  assert.deepEqual(Buffer.from(actual.rootMeanSquare.buffer), Buffer.from(frame.featureBits.slice(0, 2).join(''), 'hex'), `${frame.input.id} RMS`);
  const edges = measureClassifierEdges({ width, height, bgra:frame.workingBgra });
  assert.deepEqual(Buffer.from(edges.buffer), Buffer.from(frame.featureBits.slice(2, 5).join(''), 'hex'), `${frame.input.id} edge means`);
  const regions = { indices:Int32Array.from(frame.indices), counts:new Int32Array(frame.regionMomentBits.length),
    sums:new Float32Array(frame.regionMomentBits.length * 4) };
  for (const [region, bits] of frame.regionMomentBits.entries()) {
    const moments = Buffer.from(bits, 'hex');
    regions.counts[region] = moments.readInt32LE();
    for (let channel = 0; channel < 4; channel++) regions.sums[region * 4 + channel] = moments.readFloatLE(4 + channel * 4);
  }
  const regionFeatures = measureClassifierRegions({ width, height, bgra:frame.workingBgra }, regions);
  assert.deepEqual(Buffer.from(regionFeatures.buffer), Buffer.from(frame.featureBits.slice(5, 14).join(''), 'hex'), `${frame.input.id} region features`);
  const colors = measureClassifierColors({ width, height, bgra:frame.workingBgra }, edgeControls);
  assert.deepEqual(Array.from(colors.measurement), frame.measurement, `${frame.input.id} measurement`);
  assert.deepEqual(Buffer.from(colors.features.buffer), Buffer.from(frame.featureBits.slice(14).join(''), 'hex'), `${frame.input.id} color features`);
  const features = new Float32Array(20);
  features.set(actual.rootMeanSquare, 0); features.set(edges, 2);
  features.set(regionFeatures, 5); features.set(colors.features, 14);
  assert.deepEqual(Buffer.from(features.buffer), Buffer.from(frame.featureBits.join(''), 'hex'), `${frame.input.id} complete feature vector`);
  assert.deepEqual(classifyImageFeatures(features, frame.input.extension), frame.expected, frame.input.id);
}

test('all twenty classifier feature slots match complete native image-stage recordings', async () => {
  const path = new URL('./fixtures/image-features-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let count = 0, completed = 0, empty = 0, summary, previousState = 1;
  const labels = {};
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {
      assert.equal(frame.wholeNativeResponseRepeatIdentical, true);
      continue;
    }
    if (frame.type === 'summary') { summary = frame; continue; }
    assert.equal(frame.type, 'case');
    assert.equal(frame.initialRandomState, previousState);
    assert.deepEqual(frame.dimensions.slice(0, 4), [frame.input.width, frame.input.height, frame.input.width, frame.input.height]);
    const [width, height] = frame.dimensions.slice(4);
    if (frame.outcome === 'empty-working-image') {
      assert.deepEqual([width, height], [0, 0]);
      assert.equal(frame.input.preprocessingMode, 1);
      assert.equal(frame.input.preprocessingAmount, 1);
      assert.ok(frame.input.width <= 4 || frame.input.height <= 4);
      empty++;
    } else {
      assert.equal(frame.outcome, 'classified');
      assertClassifierFeatureStages(frame, { edgeScale:50, edgeFloor:0.01 });
      labels[frame.expected.label] = (labels[frame.expected.label] ?? 0) + 1;
      completed++;
    }
    previousState = frame.randomState;
    count++;
  }
  assert.equal(count, 96); assert.equal(completed, 72); assert.equal(empty, 24);
  assert.deepEqual(summary, { type:'summary', cases:count, completed, emptyWorkingImages:empty, labels });
});

test('initial alpha preparation and Gaussian blur match every original byte across 2224 rasters', async () => {
  const path = new URL('./fixtures/classifier-blur-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let cases = 0, compared = 0, oneDimensional = 0, changed = 0, exhaustive = 0, summary;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {
      assert.equal(frame.wholeNativeResponseRepeatIdentical, true);
      assert.equal(frame.wholeHardwareTraceRepeatIdentical, true);
      assert.equal(frame.originalImageResultPayloadIdentical, true);
      assert.equal(frame.kernelCalls, 288);
      continue;
    }
    if (frame.type === 'summary') { summary = frame; continue; }
    assert.equal(frame.type, 'case'); assert.equal(frame.outcome, 'blurred');
    const original = Array.from(frame.inputBgra), preparedInput = { ...frame, bgra:frame.inputBgra };
    const prepared = premultiplyClassifierPixels(preparedInput);
    assert.deepEqual(Array.from(prepared), frame.bgra, `${frame.id} initial native preparation`);
    assert.deepEqual(frame.inputBgra, original, `${frame.id} raw input ownership`);
    const blurred = blurClassifierPixels({ ...frame, bgra:prepared });
    assert.deepEqual(Array.from(blurred), frame.expected, `${frame.id} native Gaussian bytes`);
    assert.deepEqual(Array.from(prepared), frame.bgra, `${frame.id} prepared input ownership`);
    for (let pixel = 0; pixel < frame.width * frame.height; pixel++) {
      assert.equal(blurred[pixel * 4 + 3], prepared[pixel * 4 + 3], `${frame.id} original alpha ${pixel}`);
    }
    cases++; compared += blurred.length;
    oneDimensional += Number(frame.width === 1 || frame.height === 1);
    changed += Number(original.some((byte, index) => byte !== prepared[index]));
    if (frame.id.startsWith('all-alpha-byte-pairs-')) exhaustive += frame.width * frame.height;
  }
  assert.equal(cases, 2224); assert.equal(compared, 3208328); assert.equal(oneDimensional, 290);
  assert.equal(changed, 648); assert.equal(exhaustive, 65536);
  assert.deepEqual(summary, { type:'summary', cases, completed:cases, emptyWorkingImages:0,
    comparedBytes:compared, oneDimensionalCases:oneDimensional, kernelCalls:288,
    changedPreparationCases:changed, exhaustiveAlphaBytePairs:exhaustive });
});

test('six-pixel classifier edges match all 71620 original stencils including signed nonfinite profiles', async () => {
  const path = new URL('./fixtures/classifier-edges-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let count = 0, accepted = 0, nonfinite = 0, summary;
  const profile = new Float32Array(5);
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical, true); continue; }
    if (frame.type === 'summary') { summary = frame; continue; }
    assert.equal(frame.type, 'case');
    const samples = Float32Array.from(frame.bgra, byte => Math.fround(byte * Math.fround(1 / 255)));
    assert.deepEqual(Buffer.from(samples.buffer), Buffer.from(frame.expected.sampleBits.join(''), 'hex'), `native edge sample ${count}`);
    const threshold = Buffer.from(frame.thresholdBits, 'hex').readFloatLE();
    const selected = sampleClassifierEdge(samples, threshold, profile);
    assert.equal(selected, frame.expected.accepted, `native edge acceptance ${count}`);
    assert.deepEqual(Buffer.from(profile.buffer), Buffer.from(frame.expected.profileBits.join(''), 'hex'), `native edge profile ${count}`);
    accepted += Number(selected);
    nonfinite += Array.from(profile).filter(value => !Number.isFinite(value)).length;
    count++;
  }
  assert.equal(count, 71620);
  assert.deepEqual(summary, { type:'summary', cases:count, accepted, nonfiniteProfileValues:nonfinite });
  assert.equal(nonfinite, 1550);
});


function classifierRegionParameters(frame, subpixelParameters) {
  const parameterBuffer = Buffer.from(frame.segmentationParameterBits, 'hex');
  const parameterNames = ['use_over_seg_diag_extr','max_iterations','lambda_pre','lambda_initial','lambda_final',
    'min_num_pixels','num_iter_btwn_renum','rise_fraction','size_penalty_weight','size_penalty_power'];
  const parameters = Object.fromEntries(parameterNames.map((name, index) => [`Segmenter::${name}`,
    [0,1,5,6].includes(index) ? parameterBuffer.readUInt32LE(index * 4) : parameterBuffer.readFloatLE(index * 4)]));
  return Object.assign(parameters, subpixelParameters);
}

test('complete classification region dispatch matches native constructor-context recordings', async () => {
  const context = JSON.parse(readFileSync(new URL('./fixtures/classifier-context-reference.json', import.meta.url), 'utf8'));
  assert.equal(context.wholeNativeResponseRepeatIdentical, true);
  assert.equal(context.originalResponsePayloadMatchesOperation66, true);
  const contexts = new Map(context.cases.map(frame => [frame.id, frame]));
  const path = new URL('./fixtures/image-features-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let completed = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type !== 'case' || frame.outcome !== 'classified') continue;
    const [width, height] = frame.dimensions.slice(4);
    const parameters = classifierRegionParameters(frame, contexts.get(frame.input.id).parameters);
    const regions = segment({ width, height, bgra:frame.workingBgra, parameters });
    const rebuilt = rebuildConnectedRegions({ width, height, bgra:frame.workingBgra, regions:regions.indices });
    regions.sums = Float32Array.from(rebuilt.statistics.flatMap(statistic => Array.from(statistic.sums)));
    assert.deepEqual(Array.from(regions.indices), frame.indices, `${frame.input.id} classification region labels`);
    assert.equal(regions.counts.length, frame.regionMomentBits.length);
    for (let region = 0; region < regions.counts.length; region++) {
      const moments = Buffer.alloc(20);
      moments.writeInt32LE(regions.counts[region]);
      Buffer.from(regions.sums.buffer, region * 16, 16).copy(moments, 4);
      assert.deepEqual(moments, Buffer.from(frame.regionMomentBits[region], 'hex'), `${frame.input.id} region ${region}`);
    }
    completed++;
  }
  assert.equal(completed, 72);
});

function assertPreparedClassifierPipeline(frame, parameters) {
  const [width, height] = frame.dimensions.slice(4);
  const raster = { width, height, bgra:frame.workingBgra };
  const input = { image:{ source:raster, working:raster }, extension:frame.input.extension, parameters };
  const original = structuredClone(input), analysis = classifyImageState(input);
  assert.deepEqual(input, original, `${frame.input.id} complete pipeline input ownership`);
  assert.deepEqual(Buffer.from(analysis.features.buffer), Buffer.from(frame.featureBits.join(''), 'hex'),
    `${frame.input.id} independent complete feature bits`);
  assert.deepEqual(analysis.classification, frame.expected, `${frame.input.id} independent type/quality decision`);
  assert.deepEqual(Array.from(analysis.blurred.bgra), frame.blurredBgra, `${frame.input.id} complete pipeline blur`);
  assert.deepEqual(Buffer.from(analysis.gradient.buffer), Buffer.from(frame.gradientBits[0].join(''), 'hex'), `${frame.input.id} complete pipeline gradient`);
  assert.deepEqual(Buffer.from(analysis.blurGradient.buffer), Buffer.from(frame.gradientBits[1].join(''), 'hex'), `${frame.input.id} complete pipeline blur gradient`);
  assert.deepEqual(Array.from(analysis.measurement), frame.measurement, `${frame.input.id} complete pipeline contrast`);
  assert.deepEqual(Array.from(analysis.regions.indices), frame.indices, `${frame.input.id} complete pipeline regions`);
  assert.equal(analysis.regions.counts.length, frame.regionMomentBits.length);
  for (const [region, bits] of frame.regionMomentBits.entries()) {
    const moments = Buffer.alloc(20); moments.writeInt32LE(analysis.regions.counts[region]);
    Buffer.from(analysis.regions.sums.buffer, region * 16, 16).copy(moments, 4);
    assert.deepEqual(moments, Buffer.from(bits, 'hex'), `${frame.input.id} independent region moment ${region}`);
  }
}

test('independent prepared image pipeline matches all 150 native type/quality analyses', async () => {
  const context = JSON.parse(readFileSync(new URL('./fixtures/classifier-context-reference.json', import.meta.url), 'utf8'));
  const contexts = new Map(context.cases.map(frame => [frame.id, frame.parameters]));
  let constructor = 0, multicolor = 0;
  for (const fixture of ['image-features','classifier-colors']) {
    const path = new URL(`./fixtures/${fixture}-reference.ndjson.gz`, import.meta.url);
    const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
    for await (const line of rows) {
      const frame = JSON.parse(line);
      if (frame.type !== 'case' || frame.outcome !== 'classified') continue;
      let parameters;
      if (fixture === 'image-features') {
        parameters = classifierRegionParameters(frame, contexts.get(frame.input.id));
        constructor++;
      } else {
        assert.equal(frame.input.antiAlias, 0);
        // The second native corpus explicitly disables AA and changes the edge
        // controls. Its ordinary region parameters are captured in each frame.
        parameters = classifierRegionParameters(frame, { ...context.cases[0].parameters,
          'Shared::is_anti_aliased':0, 'EdgeImage::edge_weight_scale':frame.input.edgeScale,
          'EdgeImage::edge_weight_floor':frame.input.edgeFloor });
        multicolor++;
      }
      assertPreparedClassifierPipeline(frame, parameters);
    }
  }
  assert.equal(constructor, 72); assert.equal(multicolor, 78);
});

test('prepared image classification rejects missing controls and unsupported dimensions', () => {
  const raster = { width:2, height:2, bgra:new Uint8Array(16) };
  const image = { source:raster, working:raster }, extension = '.png';
  const parameters = { 'EdgeImage::edge_weight_scale':50, 'EdgeImage::edge_weight_floor':0.01 };
  const narrow = { width:1, height:2, bgra:new Uint8Array(8) };
  for (const [input, reason] of [
    [null, /explicit classifier parameters/], [{}, /explicit classifier parameters/],
    [{ parameters:{} }, /source\/working image state/],
    [{ image, parameters }, /explicit extension/],
    [{ image:{ source:narrow, working:narrow }, extension, parameters }, /at least 2 by 2/],
    [{ image:{ source:narrow, working:raster }, extension, parameters }, /fit within/],
    [{ image, extension, parameters:{} }, /Classifier edge controls/],
    [{ image, extension, parameters:{ ...parameters, 'EdgeImage::edge_weight_scale':-1 } }, /Classifier edge controls/],
    [{ image, extension, parameters:{ ...parameters, 'EdgeImage::edge_weight_floor':2 } }, /Classifier edge controls/]
  ]) {
    assert.throws(() => classifyImageState(input), reason);
  }
});

test('multipeak image features match all 78 native edge-control and color-distribution cases', async () => {
  const path = new URL('./fixtures/classifier-colors-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let count = 0, summary;
  const labels = {};
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical, true); continue; }
    if (frame.type === 'summary') { summary = frame; continue; }
    assert.equal(frame.type, 'case'); assert.equal(frame.outcome, 'classified');
    assert.equal(frame.input.antiAlias, 0);
    assertClassifierFeatureStages(frame, frame.input);
    labels[frame.expected.label] = (labels[frame.expected.label] ?? 0) + 1;
    count++;
  }
  assert.equal(count, 78);
  assert.deepEqual(summary, { type:'summary', cases:count, completed:count, emptyWorkingImages:0, labels });
});

test('classification base and subpixel stages match all 72 native region-dispatch traces', async () => {
  const path = new URL('./fixtures/classifier-regions-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let completed = 0, positive = 0, summary;
  const changed = [];
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical, true); continue; }
    if (frame.type === 'summary') { summary = frame; continue; }
    assert.equal(frame.type, 'case');
    const parameters = classifierRegionParameters(frame, { ...frame.parameters, clusterColors:false });
    const base = presegmentRegions({ ...frame, parameters });
    assert.deepEqual(Array.from(base.indices), frame.stages[0].indices, `${frame.id} base regions`);
    const subpixel = optimizeSubpixel({ ...frame, regions:base.indices, parameters });
    assert.deepEqual(Array.from(subpixel.indices), frame.stages[1].indices, `${frame.id} subpixel regions`);
    positive += frame.stages[4].shadowFlags.filter(flag => flag & 1).length;
    assert.equal(frame.stages[4].shadowFeatureBits.length, frame.stages[4].slots);
    for (const [region, bits] of frame.stages[4].shadowFeatureBits.entries()) {
      assert.equal(bits.length, 42);
      const features = Float64Array.from(bits, value => Buffer.from(value, 'hex').readDoubleLE());
      assert.equal(classifyShadowFeatures(features), Boolean(frame.stages[4].shadowFlags[region] & 1), `${frame.id} shadow region ${region}`);
    }
    if (frame.stages[4].indices.some((region, pixel) => region !== frame.stages[5].indices[pixel])) changed.push(frame.id);
    completed++;
  }
  assert.equal(completed, 72); assert.equal(positive, 6); assert.equal(changed.length, 6);
  assert.deepEqual(summary, { type:'summary', completed, emptyWorkingImages:24, shadowPositiveRegions:positive,
    shadowChangedImages:changed, allFinalMapsMatchUninstrumentedDispatch:true });
});

test('shadow region and refinement models match all 51008 native feature/control cases', async () => {
  const path = new URL('./fixtures/shadow-models-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let count = 0, summary;
  const controls = new Set(), outcomes = {};
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {
      assert.equal(frame.sourceSha256, 'c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1');
      assert.equal(frame.wholeNativeResponseRepeatIdentical, true);
      continue;
    }
    if (frame.type === 'summary') { summary = frame; continue; }
    assert.equal(frame.type, 'case'); assert.equal(frame.randomState, 1);
    const features = Float64Array.from(frame.featureBits, bits => Buffer.from(bits, 'hex').readDoubleLE());
    assert.equal(features.length, 42);
    const decisions = { region:classifyShadowFeatures(features), accepted:acceptShadowFeatures(features, frame.control) };
    assert.deepEqual(decisions, frame.expected, `native shadow models ${count}`);
    const key = `${Number(decisions.region)}:${Number(decisions.accepted)}`;
    outcomes[key] = (outcomes[key] ?? 0) + 1;
    controls.add(frame.control); count++;
  }
  assert.equal(count, 51008);
  assert.deepEqual(Array.from(controls).sort((a, b) => a - b), [-2147483648,-1,0,1,2,3,2147483647]);
  assert.deepEqual(summary, { type:'summary', cases:count, recordedFeatureCases:14042, thresholdComparisons:24, outcomes });
  assert.equal(Object.keys(outcomes).length, 4);
});

test('principal-color iteration matches every original matrix/state case and traced image call', async () => {
  const path = new URL('./fixtures/principal-colors-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let count = 0, calls = 0, summary;
  const cases = new Set();
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {
      assert.equal(frame.wholeNativeResponseRepeatIdentical, true);
      assert.equal(frame.tracedImageResponseRepeatIdentical, true);
      assert.equal(frame.tracedOriginalResultPayloadIdentical, true);
      continue;
    }
    if (frame.type === 'summary') { summary = frame; continue; }
    assert.equal(frame.type, 'case');
    const matrix = Float32Array.from(frame.matrixBits, bits => Buffer.from(bits, 'hex').readFloatLE());
    const previous = Buffer.from(frame.previousEigenvalueBits, 'hex').readFloatLE();
    const epsilon = Buffer.from(frame.epsilonBits, 'hex').readFloatLE();
    const actual = principalColorDirection(matrix, previous, frame.iterations, epsilon);
    const encoded = Buffer.alloc(20);
    encoded.writeFloatLE(actual.eigenvalue);
    Buffer.from(actual.direction.buffer).copy(encoded, 4);
    assert.deepEqual(encoded, Buffer.from(frame.expectedBits.join(''), 'hex'), `native principal color ${count}`);
    assert.deepEqual(Buffer.from(matrix.buffer), Buffer.from(frame.matrixBits.join(''), 'hex'));
    if (frame.nativeImageCall) { calls++; cases.add(`${frame.nativeImageCall.trace}:${frame.nativeImageCall.case}`); }
    count++;
  }
  assert.equal(count, 15249); assert.equal(calls, 4012); assert.equal(cases.size, 144);
  assert.deepEqual(summary, { type:'summary', cases:count, nativeImageCalls:calls, originalCallsPerImageRun:2006,
    nativeImageRuns:2, thresholdCalibrationMatrices:256 });
});

test('all 42 shadow features, covariances and principal directions match native image-stage state', async () => {
  const imagePath = new URL('./fixtures/image-features-reference.ndjson.gz', import.meta.url);
  const imageRows = createInterface({ input:createReadStream(imagePath).pipe(createGunzip()), crlfDelay:Infinity });
  const images = new Map();
  let nativeCase = 0;
  for await (const line of imageRows) {
    const frame = JSON.parse(line);
    if (frame.type !== 'case') continue;
    if (frame.outcome === 'classified') images.set(frame.input.id, { ...frame, nativeCase });
    nativeCase++;
  }
  const principalPath = new URL('./fixtures/principal-colors-reference.ndjson.gz', import.meta.url);
  const principalRows = createInterface({ input:createReadStream(principalPath).pipe(createGunzip()), crlfDelay:Infinity });
  const principalCalls = new Map();
  for await (const line of principalRows) {
    const frame = JSON.parse(line);
    if (frame.type !== 'case' || frame.nativeImageCall?.trace !== 'engine-principal-trace-native.bin') continue;
    const index = frame.nativeImageCall.case;
    if (!principalCalls.has(index)) principalCalls.set(index, []);
    principalCalls.get(index).push(frame);
  }
  const path = new URL('./fixtures/classifier-regions-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let cases = 0, regions = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type !== 'case') continue;
    const image = images.get(frame.id), calls = principalCalls.get(image.nativeCase);
    const raster = { width:frame.width, height:frame.height, bgra:frame.bgra };
    const state = buildSubpixelStatistics({ ...raster, regions:frame.stages[3].indices,
      neighborhood:'eight', parameters:frame.parameters });
    assert.deepEqual(Array.from(state.indices), frame.stages[4].indices, `${frame.id} shadow preparation labels`);
    const blurredBgra = blurClassifierPixels(raster);
    assert.deepEqual(Array.from(blurredBgra), image.blurredBgra, `${frame.id} independent shadow blur`);
    const gradients = measureClassifierGradients(raster, { ...raster, bgra:blurredBgra });
    const initialEigenvalue = Buffer.from(calls[0].previousEigenvalueBits, 'hex').readFloatLE();
    const actual = measureShadowFeatures(raster, state, gradients, initialEigenvalue);
    const canonical = measureShadowFeatures(raster, state, gradients, 0);
    assert.equal(actual.features.length, frame.stages[4].shadowFeatureBits.length);
    assert.equal(actual.features.length, calls.length);
    for (const [region, features] of actual.features.entries()) {
      assert.deepEqual(Buffer.from(actual.covariances[region].buffer), Buffer.from(calls[region].matrixBits.join(''), 'hex'), `${frame.id} covariance ${region}`);
      assert.deepEqual(Buffer.from(actual.directions[region].buffer), Buffer.from(calls[region].expectedBits.slice(1).join(''), 'hex'), `${frame.id} principal direction ${region}`);
      assert.deepEqual(Buffer.from(canonical.directions[region].buffer), Buffer.from(actual.directions[region].buffer), `${frame.id} observable zero-state direction ${region}`);
      assert.deepEqual(Buffer.from(features.buffer), Buffer.from(frame.stages[4].shadowFeatureBits[region].join(''), 'hex'), `${frame.id} shadow features ${region}`);
      assert.deepEqual(Buffer.from(canonical.features[region].buffer), Buffer.from(features.buffer), `${frame.id} observable zero-state features ${region}`);
      assert.equal(classifyShadowFeatures(features), Boolean(frame.stages[4].shadowFlags[region] & 1), `${frame.id} independent shadow decision ${region}`);
      regions++;
    }
    const expectedEigenvalue = Buffer.from(calls.at(-1).expectedBits[0], 'hex');
    const eigenvalue = Buffer.alloc(4); eigenvalue.writeFloatLE(actual.eigenvalue);
    assert.deepEqual(eigenvalue, expectedEigenvalue, `${frame.id} retained principal eigenvalue`);
    cases++;
  }
  assert.equal(cases, 72); assert.equal(regions, 2006);
});

test('shadow proposals and ordered fill selections match every native hardware-traced state', async () => {
  const path = new URL('./fixtures/shadow-proposals-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  const byImage = new Map(), images = {};
  let count = 0, accepted = 0, rejected = 0, summary;
  const proposal = { cost:0, target:-123 };
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {
      assert.equal(frame.wholeNativeResponseRepeatIdentical, true);
      assert.equal(frame.wholeHardwareTraceRepeatIdentical, true);
      assert.equal(frame.originalResultPayloadIdentical, true);
      continue;
    }
    if (frame.type === 'summary') { summary = frame; continue; }
    assert.equal(frame.type, 'case');
    proposal.target = -123;
    const selected = writeShadowProposal(frame, frame.pixel, proposal);
    assert.equal(selected, frame.expected.accepted, `native shadow proposal ${count}`);
    const cost = Buffer.alloc(4); cost.writeFloatLE(proposal.cost);
    assert.deepEqual(cost, Buffer.from(frame.expected.costBits, 'hex'), `native shadow proposal cost ${count}`);
    assert.equal(proposal.target, selected ? frame.expected.target : -123, `native shadow proposal target ${count}`);
    if (!byImage.has(frame.id)) byImage.set(frame.id, []);
    byImage.get(frame.id).push(frame);
    images[frame.id] = (images[frame.id] ?? 0) + 1;
    accepted += Number(selected); rejected += Number(!selected); count++;
  }
  assert.equal(count, 1474); assert.equal(accepted, 1274); assert.equal(rejected, 200);
  assert.deepEqual(summary, { type:'summary', cases:count, accepted, rejected, images });
  let assignments = 0;
  for (const [id, proposals] of byImage) {
    const first = proposals[0], choices = [];
    let previousMap, best;
    for (const frame of proposals) {
      if (previousMap && previousMap.some((region, pixel) => region !== frame.indices[pixel])) {
        assert.ok(best); choices.push(best); best = undefined;
      }
      if (frame.expected.accepted) {
        const cost = Buffer.from(frame.expected.costBits, 'hex').readFloatLE();
        if (!best || cost < best.cost) best = { pixel:frame.pixel, target:frame.expected.target, cost };
      }
      previousMap = frame.indices;
    }
    assert.ok(best); choices.push(best);
    const originalIndices = Array.from(first.indices);
    const fill = reassignShadowPixels(first, first.indices, first.shadowFlags);
    assert.deepEqual(fill.assignments, choices, `${id} original fill order and costs`);
    const finalMap = Int32Array.from(first.indices);
    for (const choice of choices) finalMap[choice.pixel] = choice.target;
    assert.deepEqual(fill.indices, finalMap, `${id} original pre-acceptance map`);
    assert.deepEqual(first.indices, originalIndices, `${id} caller map ownership`);
    assignments += choices.length;
  }
  assert.equal(assignments, 130); assert.equal(byImage.size, 6);
});

test('shadow refinement matches complete native features, wider states and accepted or restored maps', async () => {
  const path = new URL('./fixtures/shadow-refinement-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let explicit = 0, images = 0, shadows = 0, empty = 0, maximum = 0, summary;
  let accepted = 0, rejected = 0, mixed = 0;
  const plainState = state => ({ indices:Array.from(state.indices), costs:Array.from(state.costs),
    regions:state.regions.map(statistic => Object.fromEntries(Object.entries(statistic).map(([name, value]) =>
      [name, ArrayBuffer.isView(value) ? Array.from(value) : value]))) });
  const featureBits = values => Array.from(values, value => {
    const bytes = Buffer.alloc(8); bytes.writeDoubleLE(value); return bytes.toString('hex');
  });
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {
      assert.equal(frame.wholeNativeResponseRepeatIdentical, true);
      assert.equal(frame.wholeHardwareTraceRepeatIdentical, true);
      assert.equal(frame.originalResultPayloadIdentical, true);
      continue;
    }
    if (frame.type === 'summary') { summary = frame; continue; }
    assert.equal(frame.type, 'case');
    const original = structuredClone({ before:frame.before, features:frame.features, flags:frame.shadowFlags });
    const refined = refineShadowState(frame, frame.before);
    assert.deepEqual({ before:frame.before, features:frame.features, flags:frame.shadowFlags }, original,
      `${frame.id} caller ownership`);
    if (frame.trace.length > 0) {
      assert.deepEqual(plainState(refined.before), frame.trace[1].state, `${frame.id} original wider state`);
      assert.deepEqual(plainState(refined.after), frame.trace[3].state, `${frame.id} filled wider state`);
    }
    if (frame.context === 'explicit') {
      assert.deepEqual(plainState(refined.state), frame.expected, `${frame.id} final native state`);
      assert.deepEqual(refined.features.map(featureBits), frame.expectedFeatureBits, `${frame.id} all feature bits`);
      assert.deepEqual(Array.from(refined.accepted), frame.expectedAccepted, `${frame.id} original acceptance decisions`);
      const count = frame.shadowFlags.filter(flag => flag & 1).length;
      const kept = frame.expectedAccepted.filter(Boolean).length;
      accepted += kept; rejected += count - kept; mixed += Number(kept > 0 && kept < count);
      shadows += count; empty += Number(count === 0); maximum = Math.max(maximum, count); explicit++;
    } else {
      assert.equal(frame.context, 'image');
      assert.deepEqual(Array.from(refined.state.indices), frame.expectedIndices, `${frame.id} final native image map`);
      assert.deepEqual(featureBits(refined.features[frame.region]), frame.expectedFeatureBits[0], `${frame.id} original acceptance vector`);
      assert.equal(Boolean(refined.accepted[frame.region]), frame.accepted); images++;
    }
  }
  assert.equal(explicit, 640); assert.equal(images, 6); assert.equal(shadows, 7600);
  assert.equal(empty, 160); assert.equal(maximum, 86);
  assert.equal(accepted, 7576); assert.equal(rejected, 24); assert.equal(mixed, 24);
  assert.deepEqual(summary, { type:'summary', explicitCases:explicit, imageCases:images,
    explicitFlaggedRegions:shadows, explicitNoShadowCases:empty, maximumSimultaneousShadows:maximum,
    imageAcceptedRegions:images, explicitAcceptedRegions:accepted, explicitRejectedRegions:rejected,
    explicitMixedDecisionCases:mixed });
});
