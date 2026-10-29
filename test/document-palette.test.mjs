import assert from 'node:assert/strict';
import { createReadStream } from 'node:fs';
import { createInterface } from 'node:readline';
import { createGunzip } from 'node:zlib';
import test from 'node:test';
import { compositeSourceImage, prepareImagePixels, enhancePreparedImage } from '../src/image-preparation.mjs';
import { reduceWorkingImage } from '../src/image-resize.mjs';
import { resolveParameters } from '../src/settings.mjs';
import { CoreSession } from '../src/palette.mjs';
import { documentPaletteKey } from '../src/document-controls.mjs';

test('document keys match all original float32 formatting and control cases', async () => {
  const rows = createInterface({ input:createReadStream(new URL('./fixtures/document-key-reference.ndjson.gz',import.meta.url))
    .pipe(createGunzip()),crlfDelay:Infinity });
  let cases = 0, expectedCases;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical,true); expectedCases = frame.cases; continue; }
    const [category,sensitivity,advanced,antiAlias,backgroundMode] = frame.fields;
    const bytes = Buffer.from(frame.backgroundBits,'hex');
    const background = Float32Array.from({length:4},(_,index) => bytes.readFloatLE(index*4));
    assert.equal(documentPaletteKey({ category,sensitivity,advanced:Boolean(advanced),antiAlias:Boolean(antiAlias),
      backgroundMode,background }),frame.key,frame.id);
    cases++;
  }
  assert.equal(cases,expectedCases);assert.ok(cases > 21000);
});

function capturedSettings(bits) {
  const bytes = Buffer.from(bits,'hex');
  return { category:bytes.readInt32LE(),quality:bytes.readInt32LE(4),sensitivity:bytes.readInt32LE(12),
    advanced:Boolean(bytes.readUInt32LE(52)),antiAlias:Boolean(bytes.readUInt32LE(16)),backgroundMode:bytes.readInt32LE(68),
    background:Float32Array.from([84,80,76,72],offset => bytes.readFloatLE(offset)),
    complexity:bytes.readInt32LE(20),minimumPixels:bytes.readInt32LE(24),artifactRejection:bytes.readInt32LE(28),
    clusterColors:Boolean(bytes.readUInt32LE(32)),smoothness:bytes.readInt32LE(36),detectCorners:Boolean(bytes.readUInt32LE(40)),
    curveComplexity:bytes.readInt32LE(44),smoothingAntiAlias:Boolean(bytes.readUInt32LE(48)),
    paletteMode:bytes.readInt32LE(60),palette:new Float32Array() };
}

test('invalid document edits preserve committed caches, source and random state', () => {
  const session = new CoreSession();
  const colors = [[24,40,176,255],[201,167,35,255],[68,197,112,191],[183,36,153,223]];
  const bgra = Uint8Array.from({length:256},(_,index) => colors[(index/4%8>=4)+2*(index>=128)][index%4]);
  const document = session.createDocument({width:8,height:8,bgra,hasAlpha:true,shadowControl:0});
  const controls = {lower:2,upper:4,category:1,quality:1,sensitivity:1,advanced:false,antiAlias:true,
    backgroundMode:-1,background:[0,0,0,0],complexity:5,minimumPixels:0,artifactRejection:0,
    clusterColors:true,paletteMode:2,palette:[],smoothness:6,detectCorners:true,curveComplexity:6,smoothingAntiAlias:true};
  const committed = document.analyze(controls);
  assert.throws(() => document.smooth(controls),/needs completed segmentation/);
  assert.throws(() => document.fit(controls),/needs completed contours/);
  assert.deepEqual(document.snapshot(),committed);
  for (const stage of ['constructor',{toString:() => 'palette'}]) {
    assert.throws(() => document.runTask(stage,controls),/task stage must be/);
    assert.deepEqual(document.snapshot(),committed);
  }
  assert.throws(() => document.runTask('palette',{...controls,lower:1}),/lower must be/);
  assert.deepEqual(document.snapshot(),committed);
  for (const edit of [{lower:1},{upper:25},{category:3},{category:-2},{quality:0.5},{sensitivity:-1},{advanced:undefined},
    {antiAlias:1},{onProgress:1},{backgroundMode:2147483648},{background:[0,0,NaN,1]},{background:[0,0,0,2]},{background:[0,,0,1]},
    {complexity:0},{minimumPixels:-1},{artifactRejection:3},{clusterColors:1},{paletteMode:4},{palette:[0,0,0]},
    {palette:[0,0,NaN,1]},{palette:[0,,0,1]},{smoothness:0},{curveComplexity:13},{detectCorners:1},{smoothingAntiAlias:0}]) {
    assert.throws(() => document.analyze({...controls,...edit}));
    assert.deepEqual(document.snapshot(),committed);
  }
  assert.throws(() => document.setSource({width:8,height:8,bgra}));
  assert.deepEqual(document.snapshot(),committed);
  for (const shadowControl of [undefined,0.5,2147483648]) {
    assert.throws(() => document.setSource({width:8,height:8,bgra,hasAlpha:true,shadowControl}),/shadowControl/);
    assert.deepEqual(document.snapshot(),committed);
  }
  assert.throws(() => document.classify(controls),/extension string/);
  assert.throws(() => document.segment({...controls,category:-1}),/classified image category/);
  assert.deepEqual(document.snapshot(),committed);
  bgra.fill(0);
  assert.deepEqual(document.analyze(controls),committed);
  const thin = session.createDocument({width:1,height:8,bgra:new Uint8Array(32),hasAlpha:true,shadowControl:0});
  const thinBefore = thin.snapshot();
  assert.throws(() => thin.analyze(controls),/dimensions of at least 2 by 2/);
  assert.throws(() => thin.classify({...controls,extension:'.png'}),/dimensions of at least 2 by 2/);
  assert.deepEqual(thin.snapshot(),thinBefore);
});

test('session palette documents match native caches, recommendations, settings and random transitions', async () => {
  const rows = createInterface({ input:createReadStream(new URL('./fixtures/document-palette-reference.ndjson.gz',import.meta.url))
    .pipe(createGunzip()),crlfDelay:Infinity });
  const session = new CoreSession();
  let document, cases = 0, actions = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical,true); continue; }
    const bgra = Uint8Array.from(Buffer.from(frame.bgra,'base64')), before = bgra.slice();
    const input = { width:frame.width,height:frame.height,bgra,hasAlpha:true,shadowControl:0 };
    if (document) document.setSource(input);
    else document = session.createDocument(input);
    assert.deepEqual(document.snapshot().settings,capturedSettings(frame.initialSettingsBits),frame.id);
    for (let step = 0; step < frame.edits.length; step++) {
      const edit = frame.edits[step], reference = frame.states[step], label = `${frame.id} ${edit.id}`;
      const [lower,upper,category,quality,sensitivity,advanced,antiAlias,backgroundMode] = edit.values;
      const controls = { lower,upper,category,quality,sensitivity,advanced:Boolean(advanced),antiAlias:Boolean(antiAlias),
        backgroundMode,background:Float32Array.from(edit.background),complexity:5,minimumPixels:0,artifactRejection:0,
        clusterColors:true,paletteMode:2,palette:[],smoothness:6,detectCorners:true,curveComplexity:6,smoothingAntiAlias:true };
      const controlsBefore = structuredClone(controls);
      const state = document.analyze(controls);
      assert.equal(state.stageKeys.palette,reference.cacheKey,label);
      assert.deepEqual(state.completed,Uint8Array.from(reference.completed),label);
      assert.deepEqual(Buffer.from(state.scores.buffer),Buffer.from(reference.scoreBits,'hex'),`${label} scores`);
      for (let count = 0; count < 25; count++) {
        assert.deepEqual(Buffer.from(state.palettes[count].buffer),Buffer.from(reference.paletteBits[count],'hex'),`${label} palette ${count}`);
      }
      assert.deepEqual(state.settings,capturedSettings(reference.settingsBits),label);
      assert.equal(state.randomState,reference.randomState,label);
      assert.deepEqual([state.status,state.progress,state.cancelled,state.processed],
        [reference.status,reference.progress,reference.cancelled,reference.processed],label);
      for (const kind of ['source','working']) {
        assert.deepEqual([state[kind].width,state[kind].height],reference[`${kind}Dimensions`],label);
        assert.deepEqual(Buffer.from(state[kind].bgra),Buffer.from(reference[`${kind}Bgra`],'base64'),label);
      }
      assert.deepEqual(Buffer.from(state.original.bgra),Buffer.from(frame.originalBgra,'base64'),label);
      assert.deepEqual(bgra,before,label);assert.deepEqual(controls,controlsBefore,label);
      // Returned state is fully owned, including cache-hit results.
      state.source.bgra.fill(0);state.working.bgra.fill(0);state.original.bgra.fill(0);
      state.completed.fill(0);state.scores.fill(-1);state.palettes.forEach(palette => palette.fill(0));
      state.settings.background.fill(0);state.parameters['Shared::is_anti_aliased'] = -1;
      actions++;
    }
    cases++;
  }
  assert.deepEqual({cases,actions},{cases:10,actions:220});
});

test('independent source preparation matches actual document palette edits', async () => {
  const rows = createInterface({ input:createReadStream(new URL('./fixtures/document-source-reference.ndjson.gz',import.meta.url))
    .pipe(createGunzip()),crlfDelay:Infinity });
  let cases = 0, actions = 0;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical,true); continue; }
    const { width,height } = frame;
    const bgra = Uint8Array.from(Buffer.from(frame.bgra,'base64')), inputBefore = bgra.slice();
    const original = prepareImagePixels({ width,height,bgra,mode:0,amount:0 }), originalBefore = original.bgra.slice();
    assert.deepEqual(Buffer.from(original.bgra),Buffer.from(frame.originalBgra,'base64'),frame.id);
    for (let step = 0; step < frame.edits.length; step++) {
      const edit = frame.edits[step], reference = frame.states[step], label = `${frame.id} ${edit.id}`;
      const [,,category,quality,sensitivity,advanced,explicitAntiAlias,backgroundMode] = edit.values;
      const parameters = resolveParameters(advanced
        ? { stage:'palette',mode:'advanced',colorSensitivity:['high','medium','low'][sensitivity],antiAlias:Boolean(explicitAntiAlias) }
        : { stage:'palette',mode:'basic',category:['photograph','artwork-with-blending','artwork-without-blending'][category],
          quality:['high','medium','low'][quality] });
      const antiAlias = parameters['Shared::is_anti_aliased'];
      const mode = parameters['Preprocessor::filter_type'], amount = parameters['Preprocessor::reduce_noise_order'];
      assert.deepEqual([mode,amount],reference.effectivePreprocessing,label);
      const shared = Buffer.from(reference.sharedBits,'hex');
      assert.equal(shared.readUInt32LE(0),0,label);
      assert.equal(shared.readUInt32LE(4),antiAlias,label);
      assert.equal(shared.readUInt32LE(20),parameters['Shared::image_type_code'],label);
      const controls = Buffer.alloc(12);
      controls.writeFloatLE(parameters['PaletteFinder::cost_per_color'],0);
      controls.writeUInt32LE(parameters['PaletteFinder::use_16_hist_bins'],4);
      controls.writeUInt32LE(parameters['PaletteFinder::use_26_connected_peaks'],8);
      assert.equal(controls.toString('hex'),reference.paletteControlBits,label);
      const color = Float32Array.from(edit.background), colorBefore = color.slice();
      let source = compositeSourceImage(original,{ enabled:backgroundMode === 1 || backgroundMode === 2,color });
      if (mode === 1) for (let pass = 0; pass < amount; pass++) source = enhancePreparedImage(source);
      const working = reduceWorkingImage({ ...source,antiAlias,matte:1 });
      assert.deepEqual([source.width,source.height],reference.sourceDimensions,label);
      assert.deepEqual(Buffer.from(source.bgra),Buffer.from(reference.sourceBgra,'base64'),label);
      assert.deepEqual([working.width,working.height],reference.workingDimensions,label);
      assert.deepEqual(Buffer.from(working.bgra),Buffer.from(reference.workingBgra,'base64'),label);
      assert.notEqual(source.bgra,original.bgra,label);
      assert.deepEqual(original.bgra,originalBefore,label);assert.deepEqual(bgra,inputBefore,label);
      assert.deepEqual(color,colorBefore,label);
      assert.deepEqual([reference.status,reference.progress,reference.cancelled,reference.processed,reference.matte],
        [0,100,false,mode !== 0,1],label);
      actions++;
    }
    cases++;
  }
  assert.deepEqual({cases,actions},{cases:10,actions:160});
});
