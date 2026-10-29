import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {CoreSession} from '../../src/settings.mjs';
import {decodeImage,resolveImageFilename} from '../../src/image-input.mjs';

// Declared profile inputs, independent of the reference expectations. Both
// profiles use the same processing path and own their mutable control buffers.
export const restoredStartupProfile = {colorCount:3,controls:{category:1,quality:0,sensitivity:2,advanced:false,antiAlias:true,
  complexity:12,minimumPixels:0,artifactRejection:0,clusterColors:true,
  smoothness:4,detectCorners:true,curveComplexity:12,smoothingAntiAlias:true,
  paletteMode:1,palette:new Float32Array(),backgroundMode:0,background:Float32Array.of(1,1,1,1)}};
export const cleanStartupProfile = {colorCount:2,controls:{...restoredStartupProfile.controls,
  category:-1,quality:1,sensitivity:1,complexity:5,smoothness:6,curveComplexity:6,
  backgroundMode:-1,background:Float32Array.of(0,0,0,1)}};

// Only GUI bookkeeping lives here. Numerical Auto policy belongs to ImageDocument.
export function createAutomaticComparisonSession(startup) {
  const session = new CoreSession();
  let document,sourceSha256,extension;
  let controls = structuredClone(startup.controls),selectedColorCount = startup.colorCount;
  let paletteColorCount = 0,recommendedBackgroundMode = 0;
  function load(encoded,filename) {
    const decoded = decodeImage(encoded),metadata = resolveImageFilename(filename);
    extension = metadata.extension;
    sourceSha256 = createHash('sha256').update(encoded).digest('hex');
    if (document) {
      document.setSource({...decoded,shadowControl:metadata.shadowControl});
      controls = {...controls,palette:new Float32Array(),backgroundMode:-1,background:Float32Array.of(1,1,1,0)};
      paletteColorCount = 2;recommendedBackgroundMode = -1;
    } else document = session.createDocument({...decoded,shadowControl:metadata.shadowControl});
    const {original,classification,randomState,progress,cancelled} = document.snapshot();
    return {controls,selectedColorCount,paletteColorCount,recommendedBackgroundMode,
      original,classification,randomState,progress,cancelled,sourceSha256};
  }
  function runAutomatic(observeStage) {
    const result = document.runAutomatic({...controls,extension},observeStage);
    controls = result.settings;
    if (result.paletteCount !== null) selectedColorCount = paletteColorCount = result.paletteCount;
    recommendedBackgroundMode = result.recommendedBackgroundMode;
    return {controls,selectedColorCount,paletteColorCount,recommendedBackgroundMode,
      classification:{...result.state.classification},state:result.state,sourceSha256,alphaType:result.sourceAlphaType};
  }
  return {load,runAutomatic};
}

export function compareLoadedFrame(actual,frame) {
  assert.equal(actual.sourceSha256,frame.sourceSha256,'loaded file');
  assert.deepEqual(actual.controls,settingsFromBits(frame.selectedSettingsBits,frame.selectedPaletteArgbBits),'loaded selected controls');
  const bits = Buffer.from(frame.selectedSettingsBits,'hex');
  assert.equal(actual.selectedColorCount,bits.readInt32LE(8),'loaded selected color count');
  assert.equal(actual.paletteColorCount,bits.readInt32LE(56),'loaded palette color count');
  assert.equal(actual.recommendedBackgroundMode,bits.readInt32LE(88),'loaded background recommendation');
  assert.deepEqual([actual.original.width,actual.original.height],frame.header.slice(10,12),'loaded dimensions');
  assert.deepEqual(Buffer.from(actual.original.bgra),Buffer.from(frame.originalBgra,'base64'),'loaded original pixels');
  assert.equal(actual.classification.category,frame.documentClassification[0],'reset category');
  assert.deepEqual(frame.recommendedControls,[-1,-1,-1],'reset GUI recommendations');
  assert.deepEqual([actual.progress,Number(actual.cancelled)],frame.header.slice(12,14),'loaded progress/cancellation');
  assert.equal(actual.randomState,frame.header[14],'load preserves PRNG');
}

export function settingsFromBits(bits,paletteBits) {
  const encoded = Buffer.from(bits,'hex'),argb = Buffer.from(paletteBits,'hex');
  return {category:encoded.readInt32LE(),quality:encoded.readInt32LE(4),sensitivity:encoded.readInt32LE(12),
    advanced:Boolean(encoded.readUInt32LE(52)),antiAlias:Boolean(encoded.readUInt32LE(16)),backgroundMode:encoded.readInt32LE(68),
    background:Float32Array.from([84,80,76,72],offset => encoded.readFloatLE(offset)),complexity:encoded.readInt32LE(20),
    minimumPixels:encoded.readInt32LE(24),artifactRejection:encoded.readInt32LE(28),clusterColors:Boolean(encoded.readUInt32LE(32)),
    smoothness:encoded.readInt32LE(36),detectCorners:Boolean(encoded.readUInt32LE(40)),
    curveComplexity:encoded.readInt32LE(44),smoothingAntiAlias:Boolean(encoded.readUInt32LE(48)),
    paletteMode:encoded.readInt32LE(60),palette:Float32Array.from({length:argb.length/4},(_,index) =>
      argb.readFloatLE((Math.floor(index/4)*4+3-index%4)*4))};
}

export function profileFromFrame({settingsBits,paletteArgbBits}) {
  const bits = Buffer.from(settingsBits,'hex');
  return {settings:settingsFromBits(settingsBits,paletteArgbBits),colorCount:bits.readInt32LE(8),
    basicColorCount:bits.readInt32LE(64),paletteColorCount:bits.readInt32LE(56),recommendedBackgroundMode:bits.readInt32LE(88)};
}

export function compareAutomaticFrame(computed,frame) {
  const {controls,selectedColorCount,paletteColorCount,recommendedBackgroundMode,classification,state,sourceSha256,alphaType} = computed;
  const expected = frame.state;
  assert.equal(sourceSha256,frame.encodedFileSha256,'encoded source');
  assert.deepEqual(controls,settingsFromBits(frame.selectedSettingsBits,frame.selectedPaletteArgbBits),'selected controls');
  assert.equal(Buffer.from(frame.selectedSettingsBits,'hex').readInt32LE(8),selectedColorCount,'selected color count');
  assert.equal(Buffer.from(frame.selectedSettingsBits,'hex').readInt32LE(56),paletteColorCount,'palette color count');
  assert.equal(Buffer.from(frame.selectedSettingsBits,'hex').readInt32LE(88),recommendedBackgroundMode,'recommended background mode');
  assert.deepEqual([classification.category,classification.quality,classification.paletteMode,classification.colorCount],
    frame.documentClassification,'recommendations');
  assert.deepEqual([classification.category,classification.quality,classification.paletteMode],frame.recommendedControls,'GUI recommendations');
  assert.deepEqual(frame.controllerCompletion,[0,0,2,12],'completed non-batch controller bounds');
  compareDocumentState(state,expected,frame.parameters);
  return {independentAutomaticPilotMatches:true,sourceSha256:frame.encodedFileSha256,
    parameterComparisons:Object.keys(frame.parameters).length,randomState:state.randomState,
    sourceAlphaType:alphaType,selectedBackgroundMode:controls.backgroundMode,
    scope:'PNG/BMP automatic processing with filename-derived shadow context. Complete GUI policy and broader workflows remain unverified.'};
}

/** Shared strict document assertions for automatic and post-edit completions. */
export function compareDocumentState(state,expected,parameters) {
  assert.equal(state.progress,expected.progress,'completion progress');
  compareDocumentComputation(state,expected,parameters);
}

/** Compare the live shared controls, including flags outside the registry. */
export function compareDocumentSharedControls(state,hex,label = 'typed shared controls') {
  const shared = Buffer.from(hex,'hex');
  assert.deepEqual([Number(state.parameters.usePalette),state.parameters['Shared::is_anti_aliased'],0,
    Number(state.parameters.shadowing),Number(state.parameters.clusterColors),state.parameters['Shared::image_type_code']],
    Array.from({length:6},(_,index) => shared.readUInt32LE(index*4)),label);
}

/** Numerical document state shared by captures at different controller
 * phases. Callers must assert progress for their own phase separately. */
export function compareDocumentComputation(state,expected,parameters) {
  const bytes = value => Buffer.from(value.buffer,value.byteOffset,value.byteLength);
  const equalBits = (actual,hex,label) => assert.deepEqual(bytes(actual),Buffer.from(hex,'hex'),label);
  assert.deepEqual(state.settings,settingsFromBits(expected.settingsBits,expected.committedPaletteArgbBits),'committed controls');
  assert.deepEqual([state.status,state.cancelled,state.processed,state.randomState],
    [expected.status,expected.cancelled,expected.processed,expected.randomState],'completion and random state');
  assert.equal(state.stageKeys.palette,expected.cacheKey,'palette key');
  for (const [stage,key] of Object.entries(expected.stageKeys)) assert.equal(state.stageKeys[stage],key,stage+' key');
  assert.deepEqual(Array.from(state.completed),expected.completed,'completed candidates');
  equalBits(state.scores,expected.scoreBits,'candidate scores');
  for (let count = 0; count < 25; count++) equalBits(state.palettes[count],expected.paletteBits[count],`candidate ${count}`);
  for (const kind of ['source','working']) {
    assert.deepEqual([state[kind].width,state[kind].height],expected[`${kind}Dimensions`],kind+' dimensions');
    assert.deepEqual(Buffer.from(state[kind].bgra),Buffer.from(expected[`${kind}Bgra`],'base64'),kind+' pixels');
  }
  compareDocumentSharedControls(state,expected.sharedBits);
  const paletteControls = Buffer.alloc(12);
  paletteControls.writeFloatLE(state.parameters['PaletteFinder::cost_per_color']);
  paletteControls.writeInt32LE(state.parameters['PaletteFinder::use_16_hist_bins'],4);
  paletteControls.writeInt32LE(state.parameters['PaletteFinder::use_26_connected_peaks'],8);
  assert.equal(paletteControls.toString('hex'),expected.paletteControlBits,'palette controls');
  for (const [name,descriptor] of Object.entries(parameters)) {
    const values = Array.isArray(state.parameters[name]) ? state.parameters[name] : [state.parameters[name]];
    const size = descriptor.kind === 'float64' ? 8 : 4,encoded = Buffer.alloc(descriptor.count*size);
    for (let index = 0; index < values.length; index++) {
      if (descriptor.kind === 'float64') encoded.writeDoubleLE(values[index],index*size);
      else if (descriptor.kind === 'float32') encoded.writeFloatLE(values[index],index*size);
      else encoded.writeInt32LE(values[index],index*size);
    }
    assert.equal(encoded.toString('hex'),expected.parameterBits[name],name);
  }
  if (expected.segmentation.width === 0 && expected.segmentation.height === 0) {
    assert.deepEqual(expected.segmentation,{width:0,height:0,indices:[],counts:[],colorIndices:[],palette:[]},'native empty segmentation');
    assert.equal(state.segmentation,null,'no completed segmentation');
  } else {
    assert.deepEqual([state.segmentation.width,state.segmentation.height],
      [expected.segmentation.width,expected.segmentation.height],'segmentation dimensions');
    for (const key of ['indices','counts','colorIndices','palette'])
      assert.deepEqual(Array.from(state.segmentation[key]),expected.segmentation[key],key);
  }
  assert.deepEqual(state.contours,expected.contours,'complete contours');
  if (expected.smoothingMetadata === null) assert.equal(state.smoothing,null,'smoothing unavailable');
  else {
    assert.deepEqual(Array.from(state.smoothing.borderCounts),expected.smoothingMetadata.borderCounts,'border counts');
    equalBits(state.smoothing.referencePositions,bytes(Float32Array.from(expected.smoothingMetadata.referencePositions)).toString('hex'),'reference positions');
    assert.deepEqual(Array.from(state.smoothing.penaltyCounts),expected.smoothingMetadata.penaltyCounts,'penalty counts');
  }
  if (expected.fittedGeometry === null) assert.equal(state.fitting,null,'fitting unavailable');
  else {
    assert.deepEqual(state.fitting.geometry,expected.fittedGeometry,'complete fitted geometry');
    assert.deepEqual(state.fitting.outputChains,expected.outputChains,'complete assembled chains');
  }
}

