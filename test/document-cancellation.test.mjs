import assert from 'node:assert/strict';
import { createReadStream } from 'node:fs';
import { createInterface } from 'node:readline';
import { createGunzip } from 'node:zlib';
import test from 'node:test';
import { CoreSession } from '../src/palette.mjs';
import { settingsFromBits, compareDocumentSharedControls } from './helpers/native-gui.mjs';
import { exportBitmap } from '../src/bitmap-export.mjs';
import { defaultDocumentSettings } from '../src/document-controls.mjs';

test('document progress and cancellation exceptions match native direct and Qt-thread checkpoints', async () => {
  const rows = createInterface({input:createReadStream(new URL('./fixtures/document-cancellation-reference.ndjson.gz',import.meta.url))
    .pipe(createGunzip()),crlfDelay:Infinity});
  let cases = 0, completed = 0, cancelled = 0, unwound = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {assert.equal(frame.wholeNativeResponsesRepeatIdentical,true);continue;}
    const session = new CoreSession(),bgra = Uint8Array.from(Buffer.from(frame.bgra,'base64'));
    const document = session.createDocument({width:frame.width,height:frame.height,bgra,hasAlpha:true,shadowControl:0});
    let previousPercent = 0;
    for (let index = 0; index < frame.steps.length; index++) {
      const step = frame.steps[index],reference = frame.states[index],label = `${frame.id} step ${index}`;
      const [lower,upper,category,quality,sensitivity,advanced,antiAlias,backgroundMode] = step.values;
      const events = [];
      let requested = false;
      if (step.cancelAt < 0) document.requestCancel();
      const analyze = () => document.analyze({lower,upper,category,quality,sensitivity,advanced:Boolean(advanced),
        antiAlias:Boolean(antiAlias),backgroundMode,background:step.background,complexity:5,minimumPixels:0,artifactRejection:0,
        clusterColors:true,paletteMode:2,palette:[],smoothness:6,detectCorners:true,curveComplexity:6,smoothingAntiAlias:true,onProgress:event => {
          // Operation 98 uses the actual native observer, which emits only
          // changed integer percentages. Operation 97 observes all checkpoints.
          if (frame.operation === 98 && event.percent === previousPercent) return;
          previousPercent = event.percent;
          const before = requested;
          if (step.cancelAt > 0 && events.length+1 === step.cancelAt) {document.requestCancel();requested = true;}
          const fraction = Buffer.alloc(8);fraction.writeDoubleLE(event.fraction);
          events.push({ordinal:events.length+1,stage:1,colorCount:event.colorCount,phase:event.iteration*5,
            randomState:session.randomState,cancelledBefore:before,cancelledAfter:requested,
            fractionBits:fraction.toString('hex')});
        }});
      if (reference.callReturned) {analyze();completed++;}
      else {assert.throws(analyze,{name:'CancelTaskException'},label);cancelled++;}
      if (reference.callerCaught) {assert.equal(reference.prePostUnwindStateIdentical,true,label);unwound++;}
      assert.deepEqual(events,reference.events,label);
      const state = document.snapshot(),native = reference.state;
      // Native status 0xffffffff is the recorder's no-return sentinel. No
      // method return value exists at the first-chance exception snapshot.
      if (reference.callReturned) assert.equal(state.status,native.status,label);
      assert.deepEqual([state.progress,state.cancelled,state.processed,state.randomState],
        [native.progress,native.cancelled,native.processed,native.randomState],label);
      assert.equal(state.stageKeys.palette,native.cacheKey,label);
      assert.deepEqual(state.completed,Uint8Array.from(native.completed),label);
      assert.deepEqual(Buffer.from(state.scores.buffer),Buffer.from(native.scoreBits,'hex'),label);
      for (let count = 0; count < 25; count++)
        assert.deepEqual(Buffer.from(state.palettes[count].buffer),Buffer.from(native.paletteBits[count],'hex'),`${label} count ${count}`);
      for (const kind of ['source','working']) {
        assert.deepEqual([state[kind].width,state[kind].height],native[`${kind}Dimensions`],label);
        assert.deepEqual(Buffer.from(state[kind].bgra),Buffer.from(native[`${kind}Bgra`],'base64'),label);
      }
      const settings = Buffer.from(native.settingsBits,'hex');
      assert.equal(state.settings.category,settings.readInt32LE(),label);
      assert.equal(state.settings.quality,settings.readInt32LE(4),label);
      assert.equal(state.settings.sensitivity,settings.readInt32LE(12),label);
      assert.equal(state.settings.advanced,Boolean(settings.readUInt32LE(52)),label);
      assert.equal(state.settings.antiAlias,Boolean(settings.readUInt32LE(16)),label);
      assert.equal(state.settings.backgroundMode,settings.readInt32LE(68),label);
      assert.deepEqual(state.settings.background,Float32Array.from([84,80,76,72],offset => settings.readFloatLE(offset)),label);
      assert.deepEqual([state.settings.complexity,state.settings.minimumPixels,state.settings.artifactRejection,
        state.settings.clusterColors,state.settings.paletteMode],[settings.readInt32LE(20),settings.readInt32LE(24),
        settings.readInt32LE(28),Boolean(settings.readUInt32LE(32)),settings.readInt32LE(60)],label);
      assert.equal(state.settings.palette.length,0,label);
      assert.deepEqual(Buffer.from(state.original.bgra),Buffer.from(frame.originalBgra,'base64'),label);
    }
    cases++;
  }
  assert.deepEqual({cases,cancelled,unwound},{cases:234,cancelled:144,unwound:48});assert.ok(completed > 216);
});

const registeredParameters = (async () => {
  const input = createReadStream(new URL('./fixtures/document-vector-stages-reference.ndjson.gz',import.meta.url)).pipe(createGunzip());
  const rows = createInterface({input,crlfDelay:Infinity});
  try {for await (const line of rows) return JSON.parse(line).parameters;}
  finally {rows.close();input.destroy();}
})();

function compareVectorDocument(state,native,registry,label,returned = true,fittingComplete = returned && native.status === 0) {
  compareDocumentSharedControls(state,native.sharedBits,label+' typed shared controls');
  if (returned) assert.equal(state.status,native.status,label+' result');
  assert.deepEqual([state.progress,state.cancelled,state.processed,state.randomState],
    [native.progress,native.cancelled,native.processed,native.randomState],label+' task state');
  assert.deepEqual(state.settings,settingsFromBits(native.settingsBits,native.committedPaletteArgbBits),label+' committed settings');
  for (const [key,value] of Object.entries(native.stageKeys)) assert.equal(state.stageKeys[key],value,label+' '+key);
  assert.equal(state.stageKeys.palette,native.cacheKey,label+' palette key');
  assert.deepEqual(state.completed,Uint8Array.from(native.completed),label+' candidates');
  assert.equal(Buffer.from(state.scores.buffer).toString('hex'),native.scoreBits,label+' scores');
  for (let count = 0; count < 25; count++)
    assert.equal(Buffer.from(state.palettes[count].buffer).toString('hex'),native.paletteBits[count],label+' palette '+count);
  for (const kind of ['source','working']) {
    assert.deepEqual([state[kind].width,state[kind].height],native[`${kind}Dimensions`],label+' '+kind);
    assert.deepEqual(Buffer.from(state[kind].bgra),Buffer.from(native[`${kind}Bgra`],'base64'),label+' '+kind+' pixels');
  }
  const map = state.segmentation;
  assert.deepEqual([map.width,map.height],[native.segmentation.width,native.segmentation.height],label+' map dimensions');
  if (native.segmentation.indicesInitialized) assert.equal(Buffer.from(map.indices.buffer).toString('hex'),native.segmentation.indexBits,label+' labels');
  else assert.equal(map.indices,null,label+' unavailable labels');
  assert.equal(Buffer.from(map.palette.buffer).toString('hex'),native.segmentation.paletteBits,label+' live colors');
  assert.deepEqual(Array.from(map.counts),native.segmentation.counts,label+' regions');
  assert.deepEqual(Array.from(map.colorIndices),native.segmentation.colorIndices,label+' region colors');
  for (const [name,descriptor] of Object.entries(registry)) {
    const values = Array.isArray(state.parameters[name]) ? state.parameters[name] : [state.parameters[name]];
    const size = descriptor.kind === 'float64' ? 8 : 4,encoded = Buffer.alloc(descriptor.count*size);
    for (let index = 0; index < values.length; index++) {
      if (descriptor.kind === 'float64') encoded.writeDoubleLE(values[index],index*size);
      else if (descriptor.kind === 'float32') encoded.writeFloatLE(values[index],index*size);
      else encoded.writeInt32LE(values[index],index*size);
    }
    assert.equal(encoded.toString('hex'),native.parameterBits[name],label+' '+name);
  }
  if (native.contours) {
    assert.deepEqual(state.contours,native.contours,label+' live contours');
    assert.deepEqual(state.smoothing.borderCounts,Int32Array.from(native.smoothingMetadata.borderCounts),label+' borders');
    assert.deepEqual(state.smoothing.referencePositions,Float32Array.from(native.smoothingMetadata.referencePositions),label+' references');
    assert.deepEqual(state.smoothing.penaltyCounts,Uint8Array.from(native.smoothingMetadata.penaltyCounts),label+' penalties');
  }
  if (native.fittedGeometry) {
    const {cubicAvailability,...geometry} = native.fittedGeometry;
    assert.deepEqual(state.fitting.geometry,geometry,label+' fitting geometry');
    assert.deepEqual(state.fitting.cubicAvailability,cubicAvailability,label+' cubic availability');
    assert.equal(state.fitting.complete,fittingComplete,label+' completion');
  }
}

for (const [fixtureName,expectedCases,expectedCancellations = expectedCases] of [['fitting-cancellation',40],['fitting-region-progress',64],['fitting-resmoothing',16],['fitting-resegmenting',16],['fitting-cold-cancellation',40],['smoothing-cancellation',388],['smoothing-warm-cancellation',16],
  ['segmentation-cancellation',441],['segmentation-after-fitting-cancellation',22],['segmentation-gradient-cancellation',225],
  ['segmentation-after-palette-cancellation',1770],['segmentation-reduced-palette-cancellation',8],
  ['segmentation-reduced-palette-warm-cancellation',8],['segmentation-cached-palette-cancellation',16,32],
  ['palette-iteration-cancellation',480],['palette-after-fitting-cancellation',24],
  ['palette-resize-cancellation',24],['palette-after-cancellation',16,32],['vector-cached-requests',36,24]]) {
for (const invocation of ['direct','task']) test(`document vector-stage cleanup and retry match native: ${fixtureName} ${invocation}`,async () => {
  const registry = await registeredParameters;
  const rows = createInterface({input:createReadStream(new URL(`./fixtures/${fixtureName}-reference.ndjson.gz`,import.meta.url))
    .pipe(createGunzip()),crlfDelay:Infinity});
  let cases = 0,cancellations = 0;
  for await (const line of rows) {
    const fixture = JSON.parse(line);
    if (fixture.type === 'metadata') {assert.equal(fixture.wholeNativeResponsesRepeatIdentical,true);continue;}
    const {input,native,id,mode} = fixture,session = new CoreSession();
    const document = session.createDocument({width:input.width,height:input.height,
      bgra:Uint8Array.from(Buffer.from(input.bgra,'base64')),hasAlpha:true,shadowControl:0});
    let previousPercent = 0;
    for (const [index,selected] of input.steps.entries()) {
      const expected = native.steps[index],controls = {...selected,palette:Float32Array.from(selected.palette.flat())};
      const events = [],label = `${id} step ${index}`;
      let state;
      const stage = ['','segmentation','smoothing','fitting','palette','palette','palette',
        'segmentation','smoothing','fitting'][selected.action];
      controls.onProgress = event => {
        if (mode === 1 && event.percent === previousPercent) return;
        previousPercent = event.percent;
        const fraction = Buffer.alloc(8);fraction.writeDoubleLE(event.fraction);
        events.push(stage === 'palette' ? {phase:1,substep:event.colorCount,counter:event.iteration*5,fractionBits:fraction.toString('hex')}
          : {phase:event.phase,substep:event.substep,counter:event.counter,fractionBits:fraction.toString('hex')});
        if (events.length === selected.cancelAt) {
          document.requestCancel();
          compareVectorDocument(document.snapshot(),expected.exceptions[0].state,registry,label+' first chance',false);
        }
      };
      if (selected.action === 5) {
        const before = document.snapshot(),completed = document.runTask('palette',controls);
        assert.equal(completed.didWork,false,label+' native cached request started no task');
        assert.equal(expected.status,0,label+' native request return');
        state = completed.state;
        assert.equal(state.status,before.status,label+' no document method returned a new result');
      }
      else if (selected.action >= 7) {
        const before = document.snapshot(),completed = document.runTask(stage,controls);
        assert.equal(before.fitting.complete,true,label+' completed prefix geometry');
        assert.equal(completed.didWork,false,label+' native cached request started no task');
        assert.equal(expected.status,0,label+' native request return');
        state = completed.state;
        assert.deepEqual(state,before,label+' skipped vector request retained document state');
      }
      else if (selected.action === 6 && expected.caught) {
        const previousStatus = document.snapshot().status;
        const analyze = () => invocation === 'task' ? document.runTask('palette',controls) : document.analyze(controls);
        assert.throws(analyze,{name:'CancelTaskException'},label+' original method threw');
        assert.equal(expected.status,0xffffffff,label+' no original method result');
        state = document.snapshot();
        assert.equal(state.status,previousStatus,label+' throwing palette call produced no new result');
      }
      else if (stage === 'palette') state = selected.action === 6 && invocation === 'task'
        ? document.runTask('palette',controls).state : document.analyze(controls);
      else if (invocation === 'task') state = document.runTask(stage,controls).state;
      else state = document[{segmentation:'segment',smoothing:'smooth',fitting:'fit'}[stage]](controls);
      assert.deepEqual(events,expected.events.map(({header,fractionBits}) => ({phase:header[3],substep:header[4]|0,
        counter:header[5]|0,fractionBits})),label+' progress');
      if (selected.cancelAt) cancellations++;
      if (selected.cancelAt && stage !== 'palette') {
        if (stage === 'fitting') assert.equal(state.fitting.complete,false,label);
        else if (stage === 'smoothing') assert.equal(state.fitting,null,label);
        else assert.equal(state.stageKeys.segmentation,'',label);
        await assert.rejects(document.runExport(() => {throw new Error('An incomplete fit reached the writer.');}),
          /Document export needs completed fitted geometry/,label);
        assert.throws(() => exportBitmap(document,{format:'png'}),/Finish vectorizing/,label);
      }
      // Cached ThreadManager requests return admission information, not a
      // document method result. A skipped fit still keeps complete geometry.
      if (stage === 'palette' && state.fitting) assert.deepEqual([
        state.fitting.geometry.regions.length,state.fitting.geometry.nodes.length,state.fitting.geometry.cubicStorageSlots,
      ],expected.state.geometryContainerCounts,label+' retained geometry containers');
      compareVectorDocument(state,expected.state,registry,label,selected.action !== 5 && selected.action < 7 &&
        !(selected.action === 6 && expected.caught),selected.action === 9 ? true : undefined);
      // Returned partial state is an owned view. Editing it must not change
      // the retained owner that the next fitting request will retry.
      if (state.fitting) {
        state.fitting.geometry.nodes[0].x = NaN;state.fitting.cubicAvailability.fill(99);
        if (state.fitting.geometry.cubics.length) state.fitting.geometry.cubics[0].points.fill(NaN);
      }
      if (selected.action === 2) state.contours.nodes[0].x = NaN;
    }
    cases++;
  }
  assert.deepEqual({cases,cancellations},{cases:expectedCases,cancellations:expectedCancellations});
});
}

test('host fitting observer errors stay errors and cannot cache an incomplete result',() => {
  const document = new CoreSession().createDocument({width:8,height:8,hasAlpha:true,shadowControl:0,
    bgra:Uint8Array.from({length:256},(_,index) => index%4===3 ? 255 : Math.floor(index/4)%8<4 ? 20 : 220)});
  const settings = {...defaultDocumentSettings(),category:2,quality:1,paletteMode:2};
  document.segment(settings);document.smooth(settings);
  const before = document.snapshot();
  assert.throws(() => document.fit({...settings,onProgress:1}),/Document onProgress/);
  assert.deepEqual(document.snapshot(),before);
  const failure = new Error('The host observer failed.');failure.name = 'CancelTaskException';
  assert.throws(() => document.fit({...settings,onProgress:event => {if (event.substep === 2) throw failure;}}),
    error => error === failure);
  const failed = document.snapshot();
  assert.equal(failed.cancelled,false);assert.equal(failed.fitting.complete,false);
  assert.equal(failed.stageKeys.fitting,'');
  const retried = document.runTask('fitting',settings);
  assert.equal(retried.didWork,true);assert.equal(retried.state.fitting.complete,true);
  assert.equal(retried.state.status,0);
});

test('host smoothing observer errors preserve their identity and allow a fresh task retry',() => {
  const session = new CoreSession();
  const document = session.createDocument({width:8,height:8,hasAlpha:true,shadowControl:0,
    bgra:Uint8Array.from({length:256},(_,index) => index%4===3 ? 255 : Math.floor(index/4)%8<4 ? 20 : 220)});
  const settings = {...defaultDocumentSettings(),category:1,quality:1,paletteMode:2};
  document.segment(settings);
  const before = document.snapshot();
  assert.throws(() => document.smooth({...settings,onProgress:1}),/Document onProgress/);
  assert.deepEqual(document.snapshot(),before);
  const failure = new Error('The host smoothing observer failed.');failure.name = 'CancelTaskException';
  assert.throws(() => document.smooth({...settings,onProgress:event => {if (event.substep === 1) throw failure;}}),
    error => error === failure);
  const failed = document.snapshot();
  assert.equal(failed.cancelled,false);assert.equal(failed.fitting,null);
  assert.equal(failed.stageKeys.smoothing,'');assert.equal(failed.stageKeys.fitting,'');
  assert.notEqual(session.randomState,before.randomState);
  const retried = document.runTask('smoothing',settings);
  assert.equal(retried.didWork,true);assert.equal(retried.state.status,0);
  assert.ok(retried.state.stageKeys.smoothing);
});

test('host segmentation observer errors keep their identity and invalidate the partial request',() => {
  const document = new CoreSession().createDocument({width:8,height:8,hasAlpha:true,shadowControl:0,
    bgra:Uint8Array.from({length:256},(_,index) => index%4===3 ? 255 : Math.floor(index/4)%8<4 ? 20 : 220)});
  const settings = {...defaultDocumentSettings(),category:2,quality:1,paletteMode:2},before = document.snapshot();
  assert.throws(() => document.segment({...settings,onProgress:1}),/Document onProgress/);
  assert.deepEqual(document.snapshot(),before);
  const failure = new Error('The host segmentation observer failed.');failure.name = 'CancelTaskException';
  assert.throws(() => document.segment({...settings,onProgress:event => {
    if (event.phase===3 && event.substep===1) throw failure;
  }}),error => error===failure);
  const failed = document.snapshot();
  assert.equal(failed.cancelled,false);assert.equal(failed.stageKeys.segmentation,'');
  assert.equal(failed.stageKeys.smoothing,'');assert.equal(failed.stageKeys.fitting,'');
  assert.equal(failed.parameters['Segmenter::min_num_pixels'],0);
  assert.ok(failed.segmentation.indices instanceof Int32Array);
  assert.throws(() => document.smooth(settings),/needs completed segmentation/);
  const retry = document.runTask('segmentation',settings);
  assert.equal(retry.didWork,true);assert.equal(retry.state.status,0);assert.equal(retry.state.progress,100);
});
