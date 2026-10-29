import assert from 'node:assert/strict';
import { createReadStream, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { createGunzip } from 'node:zlib';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { CoreSession } from '../src/palette.mjs';

async function* frames() {
  const rows = createInterface({ input:createReadStream(new URL('./fixtures/image-controller-reference.ndjson.gz', import.meta.url))
    .pipe(createGunzip()), crlfDelay:Infinity });
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { assert.equal(frame.stableNativePayloadRepeatIdentical, true); continue; }
    yield frame;
  }
}

function imageInput(frame) {
  const { width, height, mode, amount, extension, parameters, paletteControls } = frame;
  return { width, height, mode, amount, extension, hasAlpha:true, parameters:structuredClone(parameters),
    paletteControls, bgra:Uint8Array.from(Buffer.from(frame.bgra, 'base64')) };
}

test('session image analysis matches complete native controllers and fatal random effects', async () => {
  let session, sessionId = -1, completed = 0, fatal = 0;
  for await (const frame of frames()) {
    if (frame.session !== sessionId) { session = new CoreSession(); sessionId = frame.session; }
    assert.equal(session.randomState, frame.initialRandomState, frame.id);
    const input = imageInput(frame), before = structuredClone(input);
    if (frame.outcome === 'empty-peak-fault') {
      assert.throws(() => session.analyzeImage(input), /has no histogram peaks/, frame.id);
      fatal++;
    } else {
      const analyzed = session.analyzeImage(input), { source, working } = analyzed.image;
      assert.deepEqual([source.width,source.height], frame.preparedDimensions, frame.id);
      assert.deepEqual(Buffer.from(source.bgra), Buffer.from(frame.preparedBgra, 'base64'), frame.id);
      assert.deepEqual([working.width,working.height], frame.workingDimensions, frame.id);
      assert.deepEqual(Buffer.from(working.bgra), Buffer.from(frame.workingBgra, 'base64'), frame.id);
      assert.notEqual(source.bgra, input.bgra, frame.id); assert.notEqual(working.bgra, source.bgra, frame.id);
      assert.deepEqual(analyzed.imageAnalysis.classification, frame.imageClassification, frame.id);
      assert.deepEqual(Buffer.from(analyzed.imageAnalysis.features.buffer), Buffer.from(frame.imageFeatureBits, 'hex'), frame.id);
      assert.equal(analyzed.parameters['Shared::is_anti_aliased'], frame.shared[1], frame.id);
      assert.equal(analyzed.parameters['Shared::image_type_code'], frame.shared[5], frame.id);
      assert.deepEqual(analyzed.paletteAnalysis.classification, frame.paletteClassification, frame.id);
      assert.deepEqual(Buffer.from(analyzed.paletteAnalysis.features.buffer), Buffer.from(frame.paletteFeatureBits, 'hex'), frame.id);
      assert.deepEqual(Buffer.from(analyzed.paletteAnalysis.peakWeights.buffer), Buffer.from(frame.peakWeightBits, 'hex'), frame.id);
      assert.equal(analyzed.paletteAnalysis.candidates.length, 11, frame.id);
      for (const [index,candidate] of analyzed.paletteAnalysis.candidates.entries()) {
        const expected = frame.candidates[index];
        assert.equal(candidate.colorCount, expected.colorCount, frame.id);
        assert.deepEqual(Buffer.from(candidate.palette.buffer), Buffer.from(expected.paletteBits, 'hex'), frame.id);
        assert.deepEqual(Buffer.from(candidate.weights.buffer), Buffer.from(expected.weightBits, 'hex'), frame.id);
        const scores = Float32Array.of(analyzed.recommendationScores[index],candidate.score.error,candidate.score.colors);
        assert.deepEqual(Buffer.from(scores.buffer), Buffer.from(expected.scoreBits, 'hex'), `${frame.id} candidate ${index+2}`);
      }
      assert.deepEqual(Buffer.from(analyzed.paletteAnalysis.candidates.at(-1).indices.buffer), Buffer.from(frame.indices, 'base64'), frame.id);
      assert.equal(analyzed.randomState, frame.randomState, frame.id);
      completed++;
    }
    assert.equal(session.randomState, frame.randomState, frame.id);
    assert.deepEqual(input, before, frame.id);
  }
  assert.deepEqual({ completed,fatal,sessions:sessionId+1 }, { completed:42,fatal:2,sessions:3 });
});

test('image analysis rejects missing explicit controls before consuming random state', async () => {
  let reference;
  for await (const frame of frames()) { reference = frame; break; }
  const input = imageInput(reference), session = new CoreSession();
  for (const invalid of [null, { ...input,mode:9 }, { ...input,paletteControls:undefined },
    { ...input,parameters:{ ...input.parameters,shadowing:undefined } }]) {
    assert.throws(() => session.analyzeImage(invalid));
    assert.equal(session.randomState, 1);
  }
});

test('image analysis owns parameter arrays and palette controls cannot replace its working raster', async () => {
  for await (const frame of frames()) {
    const input = imageInput(frame);
    input.parameters['ContourSmoother::prior_types'] = [3,3,3];
    input.paletteControls = { ...input.paletteControls,width:0,height:0,bgra:[] };
    const analyzed = new CoreSession().analyzeImage(input);
    assert.deepEqual(analyzed.paletteAnalysis.classification, frame.paletteClassification);
    assert.deepEqual(Buffer.from(analyzed.paletteAnalysis.features.buffer), Buffer.from(frame.paletteFeatureBits, 'hex'));
    analyzed.parameters['ContourSmoother::prior_types'][0] = 0;
    assert.deepEqual(input.parameters['ContourSmoother::prior_types'], [3,3,3]);
    break;
  }
});

const cli = fileURLToPath(new URL('../bin/openvector.mjs', import.meta.url));

function nativeCliOutput(frame) {
  return { imageClassification:frame.imageClassification,imageTypeCode:frame.shared[5],antiAlias:Boolean(frame.shared[1]),
    width:frame.workingDimensions[0],height:frame.workingDimensions[1],paletteClassification:frame.paletteClassification,
    candidates:frame.candidates.map(candidate => {
      const palette = Buffer.from(candidate.paletteBits, 'hex'), weights = Buffer.from(candidate.weightBits, 'hex');
      const scores = Buffer.from(candidate.scoreBits, 'hex');
      return { colorCount:candidate.colorCount,palette:Array.from({ length:palette.length/4 }, (_,index) => palette.readFloatLE(index*4)),
        weights:Array.from({ length:weights.length/8 }, (_,index) => weights.readDoubleLE(index*8)),
        error:scores.readFloatLE(4),colorCost:scores.readFloatLE(8),recommendationScore:scores.readFloatLE(0) };
    }),randomState:frame.randomState };
}

test('analyze-image CLI matches native summaries from stdin and a path with spaces', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-analysis-'));
  let checked = 0;
  try {
    for await (const frame of frames()) {
      if (frame.initialRandomState !== 1 || frame.outcome !== 'classified') continue;
      const input = imageInput(frame); input.bgra = Array.from(input.bgra);
      const filename = join(directory, 'image with spaces.json'), document = JSON.stringify(input);
      writeFileSync(filename, document);
      const run = spawnSync(process.execPath, [cli,'analyze-image',checked ? filename : '-'],
        { encoding:'utf8',input:checked ? undefined : document });
      assert.equal(run.status, 0, run.stderr); assert.equal(run.stderr, '');
      assert.deepEqual(JSON.parse(run.stdout), nativeCliOutput(frame), frame.id);
      checked++;
    }
  } finally { rmSync(directory, { recursive:true }); }
  assert.equal(checked, 2);
});

test('analyze-image CLI reports native empty-peak failure and invalid input without partial JSON', async () => {
  for await (const frame of frames()) {
    if (frame.outcome !== 'empty-peak-fault' || frame.initialRandomState !== 1) continue;
    const input = imageInput(frame); input.bgra = Array.from(input.bgra);
    const run = spawnSync(process.execPath, [cli,'analyze-image','-'], { encoding:'utf8',input:JSON.stringify(input) });
    assert.equal(run.status, 1); assert.equal(run.stdout, ''); assert.match(run.stderr, /has no histogram peaks/);
  }
  for (const args of [['analyze-image','-'],['analyze-image','-','extra']]) {
    const run = spawnSync(process.execPath, [cli,...args], { encoding:'utf8',input:'{}' });
    assert.equal(run.status, 1); assert.equal(run.stdout, ''); assert.ok(run.stderr.length > 0);
  }
  const help = spawnSync(process.execPath, [cli,'analyze-image','--help'], { encoding:'utf8' });
  assert.equal(help.status, 0); assert.equal(help.stderr, ''); assert.match(help.stdout, /analyze-image/);
});
