import assert from 'node:assert/strict';
import { createReadStream } from 'node:fs';
import { createInterface } from 'node:readline';
import { createGunzip } from 'node:zlib';
import test from 'node:test';
import { CoreSession,resolveParameters } from '../src/settings.mjs';

test('document stage profiles match every original registered parameter byte', async () => {
  const rows = createInterface({input:createReadStream(new URL('./fixtures/document-segmentation-reference.ndjson.gz',import.meta.url))
    .pipe(createGunzip()),crlfDelay:Infinity});
  let registry,cases = 0,actions = 0,comparisons = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {assert.equal(frame.wholeNativeResponseRepeatIdentical,true);registry = frame.parameters;continue;}
    for (let index = 0; index < frame.steps.length; index++) {
      const step = frame.steps[index],reference = frame.states[index].state;
      const stage = step.action === 0 ? 'palette' : 'segmentation';
      let options;
      if (!step.advanced) {
        options = {category:['photograph','artwork-with-blending','artwork-without-blending'][step.category],
          quality:['high','medium','low'][step.quality]};
      } else if (stage === 'palette') {
        options = {colorSensitivity:['high','medium','low'][step.sensitivity],antiAlias:step.antiAlias};
      } else {
        options = {complexity:step.complexity,minimumPixels:step.minimumPixels,
          artifactRejection:['none','moderate','aggressive'][step.artifactRejection],clusterColors:step.clusterColors};
      }
      const parameters = resolveParameters({stage,mode:step.advanced ? 'advanced' : 'basic',...options});
      for (const [name,descriptor] of Object.entries(registry)) {
        const values = Array.isArray(parameters[name]) ? parameters[name] : [parameters[name]];
        const bytes = descriptor.kind === 'float64' ? 8 : 4,encoded = Buffer.alloc(descriptor.count*bytes);
        for (let item = 0; item < values.length; item++) {
          if (descriptor.kind === 'float64') encoded.writeDoubleLE(values[item],item*bytes);
          else if (descriptor.kind === 'float32') encoded.writeFloatLE(values[item],item*bytes);
          else encoded.writeInt32LE(values[item],item*bytes);
        }
        assert.equal(encoded.toString('hex'),reference.parameterBits[name],`${frame.id} ${step.id} ${name}`);
        comparisons++;
      }
      actions++;
    }
    cases++;
  }
  assert.deepEqual({cases,actions,comparisons},{cases:4,actions:145,comparisons:14500});
});

function capturedSettings(bits,paletteBits = '') {
  const bytes = Buffer.from(bits,'hex'),argb = Buffer.from(paletteBits,'hex');
  return {category:bytes.readInt32LE(),quality:bytes.readInt32LE(4),sensitivity:bytes.readInt32LE(12),
    advanced:Boolean(bytes.readUInt32LE(52)),antiAlias:Boolean(bytes.readUInt32LE(16)),backgroundMode:bytes.readInt32LE(68),
    background:Float32Array.from([84,80,76,72],offset => bytes.readFloatLE(offset)),complexity:bytes.readInt32LE(20),
    minimumPixels:bytes.readInt32LE(24),artifactRejection:bytes.readInt32LE(28),clusterColors:Boolean(bytes.readUInt32LE(32)),
    smoothness:bytes.readInt32LE(36),detectCorners:Boolean(bytes.readUInt32LE(40)),
    curveComplexity:bytes.readInt32LE(44),smoothingAntiAlias:Boolean(bytes.readUInt32LE(48)),
    paletteMode:bytes.readInt32LE(60),palette:Float32Array.from({length:argb.length/4},(_,index) =>
      argb.readFloatLE((Math.floor(index/4)*4+3-index%4)*4))};
}

async function compareDocuments(fixture,expected) {
  const rows = createInterface({input:createReadStream(new URL(`./fixtures/${fixture}-reference.ndjson.gz`,import.meta.url))
    .pipe(createGunzip()),crlfDelay:Infinity});
  const session = new CoreSession();
  let document,registry,controllerMode = false,classificationMode = false,cases = 0,actions = 0,recomputations = 0,hits = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {
      assert.equal(frame.wholeNativeResponseRepeatIdentical,true);registry = frame.parameters;
      controllerMode = frame.operation >= 102;classificationMode = frame.operation === 104;continue;
    }
    const bgra = Uint8Array.from(Buffer.from(frame.bgra,'base64')),before = bgra.slice();
    const input = {width:frame.width,height:frame.height,bgra,hasAlpha:true,shadowControl:classificationMode ? frame.shadowControl : 0};
    if (document) document.setSource(input);else document = session.createDocument(input);
    assert.deepEqual(document.snapshot().settings,capturedSettings(frame.initialSettingsBits),frame.id);
    if (classificationMode) compareClassification(document.snapshot().classification,frame.initialClassificationFields,frame.id+' initial recommendations');
    const previousKeys = {segmentation:'',smoothing:'',fitting:''};
    for (let index = 0; index < frame.steps.length; index++) {
      const step = frame.steps[index],native = frame.states[index].state,label = `${frame.id} ${step.id}`;
      const controls = {smoothness:6,detectCorners:true,curveComplexity:6,smoothingAntiAlias:true,...step,
        ...(classificationMode ? {extension:frame.extension} : {}),palette:Float32Array.from(step.palette.flat())},controlsBefore = structuredClone(controls);
      const stage = ['palette','segmentation','smoothing','fitting','classification'][step.action];
      const method = ['analyze','segment','smooth','fit','classify'][step.action];
      const task = controllerMode ? document.runTask(stage,controls) : undefined;
      const state = task ? task.state : document[method](controls);
      if (task) {
        const captured = frame.states[index].controller,info = captured.info;
        assert.equal(task.didWork,frame.states[index].needsWork,label+' scheduler decision');
        assert.deepEqual(task.settings,capturedSettings(captured.activeSettingsBits,captured.activePaletteArgbBits),label+' requested settings');
        assert.equal(captured.activeSettingsBits,frame.states[index].requestedSettingsBits,label+' active copy');
        assert.equal(captured.pendingSettingsBits,frame.states[index].requestedSettingsBits,label+' pending copy');
        assert.deepEqual(info,[Number(task.didWork),task.didWork ? step.action === 4 ? 1 : step.action+2 : 0,0,Number(task.didWork),
          0,0,0,Number(task.didWork),0,1],label+' original Qt completion');
        task.settings.background.fill(NaN);task.settings.palette.fill(NaN);
      }
      if (step.action >= 1 && step.action <= 3) assert.equal(previousKeys[stage] !== state.stageKeys[stage],frame.states[index].needsWork,label);
      if (step.action === 1) {
        if (frame.states[index].needsWork) recomputations++;else hits++;
      }
      if (step.action === 1 || (classificationMode && state.segmentation !== null)) {
        const segmented = state.segmentation,reference = native.segmentation;
        assert.deepEqual([segmented.width,segmented.height],[reference.width,reference.height],label);
        assert.deepEqual(Buffer.from(segmented.indices.buffer),Buffer.from(reference.indexBits,'hex'),label+' indices');
        assert.deepEqual(Buffer.from(segmented.palette.buffer),Buffer.from(reference.paletteBits,'hex'),label+' live palette');
        assert.deepEqual(Array.from(segmented.counts),reference.counts,label+' retained counts');
        assert.deepEqual(Array.from(segmented.colorIndices),reference.colorIndices,label+' retained color indices');
      }
      Object.assign(previousKeys,state.stageKeys);
      if (classificationMode ? native.geometryPresent : step.action >= 2) {
        assert.deepEqual(state.contours,native.contours,label+' complete contours');
        assert.deepEqual(state.smoothing.borderCounts,Int32Array.from(native.smoothingMetadata.borderCounts),label+' borders');
        assert.deepEqual(state.smoothing.referencePositions,Float32Array.from(native.smoothingMetadata.referencePositions),label+' references');
        assert.deepEqual(state.smoothing.penaltyCounts,Uint8Array.from(native.smoothingMetadata.penaltyCounts),label+' penalties');
        if (classificationMode ? native.fittingPresent : step.action === 3) {
          assert.deepEqual(state.fitting.geometry,native.fittedGeometry,label+' fitted graph');
          assert.deepEqual(state.fitting.outputChains,native.outputChains,label+' assembled chains');
        }
      }
      if (classificationMode) compareClassification(state.classification,native.classificationFields,label+' recommendations');
      if (step.action === 4) {
        const analyzed = task.analysis,reference = native.classificationAnalysis;
        assert.equal(task.didWork,true,label+' always recomputes');
        compareClassification({category:analyzed.imageAnalysis.classification.imageType,quality:analyzed.imageAnalysis.classification.quality,
          paletteMode:analyzed.paletteAnalysis.classification.mode,colorCount:analyzed.paletteAnalysis.classification.colorCount},native.classificationFields,label+' analysis recommendations');
        assert.deepEqual(Buffer.from(analyzed.imageAnalysis.features.buffer),Buffer.from(reference.imageFeatureBits,'hex'),label+' image features');
        assert.deepEqual(Buffer.from(analyzed.paletteAnalysis.features.buffer),Buffer.from(reference.paletteFeatureBits,'hex'),label+' palette features');
        assert.equal(analyzed.paletteAnalysis.peakWeights.length,reference.peakCount,label+' peak count');
        assert.deepEqual(Buffer.from(analyzed.paletteAnalysis.peakWeights.buffer),Buffer.from(reference.peakBits,'hex'),label+' peaks');
        assert.equal(analyzed.paletteAnalysis.candidates.length,11,label+' candidate count');
        for (const [index,candidate] of analyzed.paletteAnalysis.candidates.entries()) {
          const expectedCandidate = reference.candidates[index];
          assert.equal(candidate.colorCount,expectedCandidate.count,label+' candidate count');
          assert.deepEqual(Buffer.from(candidate.palette.buffer),Buffer.from(expectedCandidate.paletteBits,'hex'),label+' candidate palette');
          assert.deepEqual(Buffer.from(candidate.weights.buffer),Buffer.from(expectedCandidate.weightBits,'hex'),label+' candidate weights');
          const scores = Float32Array.of(analyzed.recommendationScores[index],candidate.score.error,candidate.score.colors);
          assert.deepEqual(Buffer.from(scores.buffer),Buffer.from(expectedCandidate.scoreBits,'hex'),label+' candidate scores');
        }
        const final = analyzed.paletteAnalysis.candidates.at(-1);
        assert.deepEqual(Buffer.from(final.indices.buffer),Buffer.from(native.segmentation.indexBits,'hex'),label+' final live indices');
        assert.deepEqual(Buffer.from(final.palette.buffer),Buffer.from(native.segmentation.paletteBits,'hex'),label+' final live palette');
        assert.deepEqual(analyzed.image.source,state.source,label+' prepared source');
        assert.deepEqual(analyzed.image.working,state.working,label+' final working');
        assert.equal(analyzed.randomState,state.randomState,label+' analysis random stream');
        // Mutate every analysis buffer retained by document state, then let
        // the next strict native step check that no internal alias escaped.
        analyzed.image.source.bgra.fill(0);analyzed.image.working.bgra.fill(0);
        analyzed.parameters['ContourSmoother::prior_types'].fill(-1);
        final.indices.fill(-1);final.palette.fill(NaN);
      }
      if (cases === 0 && step.id === 'flexible-initial') {
        for (const invalid of [{complexity:0},{minimumPixels:-1},{paletteMode:1,palette:[]},
          {background:[0,0,NaN,1]},{palette:[0,,0,1]}]) {
          assert.throws(() => document.segment({...controls,...invalid}));
          assert.deepEqual(document.snapshot(),state,label+' invalid edit preserves state');
        }
      }
      assert.equal(state.stageKeys.palette,native.cacheKey,label);
      for (const [stage,key] of Object.entries(native.stageKeys)) assert.equal(state.stageKeys[stage],key,label+' '+stage);
      assert.deepEqual(state.settings,capturedSettings(native.settingsBits,native.committedPaletteArgbBits),label);
      assert.deepEqual([state.status,state.progress,state.cancelled,state.processed,state.randomState],
        [native.status,native.progress,native.cancelled,native.processed,native.randomState],label);
      assert.deepEqual(state.completed,Uint8Array.from(native.completed),label);
      assert.deepEqual(Buffer.from(state.scores.buffer),Buffer.from(native.scoreBits,'hex'),label);
      for (let count = 0; count < 25; count++)
        assert.deepEqual(Buffer.from(state.palettes[count].buffer),Buffer.from(native.paletteBits[count],'hex'),`${label} count ${count}`);
      for (const kind of ['source','working']) {
        assert.deepEqual([state[kind].width,state[kind].height],native[`${kind}Dimensions`],label);
        assert.deepEqual(Buffer.from(state[kind].bgra),Buffer.from(native[`${kind}Bgra`],'base64'),label);
      }
      const shared = Buffer.from(native.sharedBits,'hex');
      assert.deepEqual([Number(state.parameters.usePalette),state.parameters['Shared::is_anti_aliased'],0,
        Number(state.parameters.shadowing),Number(state.parameters.clusterColors),state.parameters['Shared::image_type_code']],
        Array.from({length:6},(_,offset) => shared.readUInt32LE(offset*4)),label+' shared');
      assert.equal(state.parameters.shadowControl,native.shadowControl,label);
      for (const [name,descriptor] of Object.entries(registry)) {
        const values = Array.isArray(state.parameters[name]) ? state.parameters[name] : [state.parameters[name]];
        const size = descriptor.kind === 'float64' ? 8 : 4,encoded = Buffer.alloc(descriptor.count*size);
        for (let item = 0; item < values.length; item++) {
          if (descriptor.kind === 'float64') encoded.writeDoubleLE(values[item],item*size);
          else if (descriptor.kind === 'float32') encoded.writeFloatLE(values[item],item*size);
          else encoded.writeInt32LE(values[item],item*size);
        }
        assert.equal(encoded.toString('hex'),native.parameterBits[name],`${label} ${name}`);
      }
      assert.deepEqual(Buffer.from(state.original.bgra),Buffer.from(frame.originalBgra,'base64'),label);
      assert.deepEqual(bgra,before,label);assert.deepEqual(controls,controlsBefore,label);
      // Every public snapshot owns its arrays, including shared live-core data.
      state.source.bgra.fill(0);state.working.bgra.fill(0);state.original.bgra.fill(0);
      state.completed.fill(0);state.scores.fill(-1);state.palettes.forEach(palette => palette.fill(0));
      state.settings.background.fill(0);state.settings.palette.fill(0);
      state.classification.category = 99;
      state.stageKeys.segmentation = 'caller edit';
      if (state.segmentation) {
        assert.notEqual(state.settings.palette.buffer,state.segmentation.palette.buffer,label+' distinct native color copies');
        state.segmentation.indices.fill(0);state.segmentation.palette.fill(0);
        state.segmentation.counts.fill(0);state.segmentation.colorIndices.fill(0);
      }
      if (state.contours) state.contours.nodes[0].x = NaN;
      if (state.smoothing) {
        state.smoothing.borderCounts.fill(-1);state.smoothing.referencePositions.fill(NaN);state.smoothing.penaltyCounts.fill(255);
      }
      if (state.fitting) {
        state.fitting.geometry.nodes[0].x = NaN;
        state.fitting.outputChains.children[0].push(-1);
      }
      actions++;
    }
    cases++;
  }
  assert.deepEqual({cases,actions,recomputations,hits},expected);
}

function compareClassification(actual,fields,label) {
  assert.deepEqual([actual.category,actual.quality,actual.paletteMode,actual.colorCount],fields,label);
}

test('completed odd-sized segmentation matches the new native gradient interruption baselines',async () => {
  const rows = createInterface({input:createReadStream(new URL('./fixtures/segmentation-gradient-cancellation-reference.ndjson.gz',import.meta.url))
    .pipe(createGunzip()),crlfDelay:Infinity});
  const seen = new Set();
  for await (const line of rows) {
    const fixture = JSON.parse(line);
    if (fixture.type === 'metadata') {assert.equal(fixture.wholeNativeResponsesRepeatIdentical,true);continue;}
    if (fixture.mode !== 0) continue;
    const {input,native} = fixture,controls = input.steps.at(-1),key = JSON.stringify([input.width,input.height,input.bgra,controls]);
    if (seen.has(key)) continue;
    seen.add(key);
    const document = new CoreSession().createDocument({width:input.width,height:input.height,
      bgra:Uint8Array.from(Buffer.from(input.bgra,'base64')),hasAlpha:true,shadowControl:0});
    const state = document.segment({...controls,palette:Float32Array.from(controls.palette.flat())});
    const expected = native.steps.at(-1).state,label = fixture.id;
    assert.deepEqual([state.status,state.progress,state.cancelled,state.processed,state.randomState],
      [expected.status,expected.progress,expected.cancelled,expected.processed,expected.randomState],label);
    assert.deepEqual(state.settings,capturedSettings(expected.settingsBits,expected.committedPaletteArgbBits),label);
    assert.deepEqual(state.stageKeys,{palette:expected.cacheKey,...expected.stageKeys},label);
    for (const kind of ['source','working']) {
      assert.deepEqual([state[kind].width,state[kind].height],expected[`${kind}Dimensions`],label);
      assert.deepEqual(Buffer.from(state[kind].bgra),Buffer.from(expected[`${kind}Bgra`],'base64'),label);
    }
    const map = state.segmentation,reference = expected.segmentation;
    assert.deepEqual([map.width,map.height],[reference.width,reference.height],label);
    assert.equal(Buffer.from(map.indices.buffer).toString('hex'),reference.indexBits,label);
    assert.equal(Buffer.from(map.palette.buffer).toString('hex'),reference.paletteBits,label);
    assert.deepEqual(Array.from(map.counts),reference.counts,label);
    assert.deepEqual(Array.from(map.colorIndices),reference.colorIndices,label);
  }
  assert.equal(seen.size,4);
});

test('one image document matches native interleaved palette, segmentation and shared cache transitions', async () => {
  await compareDocuments('document-segmentation',{cases:4,actions:145,recomputations:93,hits:36});
});

test('one image document matches all native smoothing/fitting caches, shared geometry and assembled chains', async () => {
  await compareDocuments('document-vector-stages',{cases:8,actions:378,recomputations:44,hits:5});
});

test('completed document tasks match original Qt controller skips, commits and full core state', async () => {
  await compareDocuments('document-task-controller',{cases:8,actions:384,recomputations:44,hits:6});
});

test('document classification matches real Qt recommendations, all candidates and interleaved cache/geometry lifetimes', async () => {
  await compareDocuments('document-classification',{cases:19,actions:380,recomputations:57,hits:0});
});
