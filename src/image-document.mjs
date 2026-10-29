import { resolveParameters } from './settings.mjs';
import numericData from './parameter-data.json' with { type: 'json' };
import { prepareOriginalImage } from './image-pipeline.mjs';
import { compositeSourceImage, filterPreparedImage } from './image-preparation.mjs';
import { reduceWorkingImage } from './image-resize.mjs';
import { SegmentationState } from './segmentation-state.mjs';
import { extractContours } from './contours.mjs';
import { ContourFitter } from './contour-fitting.mjs';
import { assembleOutputShapes } from './vector-output.mjs';
import { readDocumentSettings,readBatchControls,batchDocumentSettings,defaultDocumentSettings,documentPaletteKey,documentSegmentationKey,documentSmoothingKey,documentFittingKey } from './document-controls.mjs';
import { SegmentationEditor } from './segmentation-editor.mjs';
import { BackgroundEditor } from './background-editor.mjs';
import { paletteFromByteColors,rebuildConnectedRegions } from './region-data.mjs';
import {CancelTaskException} from './task-cancellation.mjs';
export {CancelTaskException} from './task-cancellation.mjs';

const categories = {'-1':'unclassified',0:'photograph',1:'artwork-with-blending',2:'artwork-without-blending'};
const levels = ['high','medium','low'];
const rejections = ['none','moderate','aggressive'];

const taskKeys = {palette:documentPaletteKey,segmentation:documentSegmentationKey,
  smoothing:documentSmoothingKey,fitting:documentFittingKey};

// Both application workflows commit a candidate through QColor's 16-bit
// channels. Candidate storage uses ARGB; document controls use BGRA.
function selectedPalette(state,count) {
  const argb = Float32Array.from(state.palettes[count],value => Math.fround(Math.round(value*65535)/65535));
  return Float32Array.from(argb,(_,index) => argb[Math.floor(index/4)*4+3-index%4]);
}

function minimumPaletteCount(state) {
  let minimum=1e10,selected=null;
  for (let count=0;count<state.completed.length;count++) {
    if (state.completed[count] && state.scores[count]<minimum) {
      selected=count;minimum=state.scores[count];
    }
  }
  return selected;
}

function readPaletteControls(input) {
  const {lower,upper} = input,onProgress = readProgress(input);
  for (const [name,value] of [['lower',lower],['upper',upper]])
    if (!Number.isInteger(value) || value < 2 || value > 24) throw new Error(`Document ${name} must be an integer from 2 through 24.`);
  return {lower,upper,onProgress};
}

function readProgress(input) {
  const {onProgress} = input;
  if (onProgress !== undefined && typeof onProgress !== 'function') throw new Error('Document onProgress must be a function.');
  return onProgress;
}

function readDocumentSource(input) {
  const original = prepareOriginalImage(input);
  if (!Number.isInteger(input.shadowControl) || input.shadowControl < -2147483648 || input.shadowControl > 2147483647)
    throw new Error('Document source shadowControl must be an explicit signed int32 integer.');
  return {original,shadowControl:input.shadowControl};
}

/** Document owned by one application PRNG session. Candidate caches, shared
 * live image/map/palette and retained region metadata have distinct native
 * lifetimes. A cache hit must not restore an old copy of the live core. */
export class ImageDocument {
  #session;
  #state;
  #candidates;
  #runtime;
  #sourceAlphaType = -1;
  // MainView keeps this point when source data or the bitmap editor changes.
  // Both editing modes use the same viewer-owned gesture state.
  #pointer = {previousPoint:[-1,-1]};
  #editor = null;
  #committedEditing = {undo:[],redo:[]};
  #regionPalette = null;
  #background = new BackgroundEditor();
  #fitter = null;
  #fitterRegions = null;

  constructor(input,session) {
    this.#session = session;
    if (input === undefined) this.#resetSource();
    else this.setSource(input);
  }

  setSource(input) {
    // Validate and prepare before replacing state, so invalid source edits
    // preserve the current document and application random stream.
    const source = readDocumentSource(input);
    this.#resetSource();
    this.#installSource(source);
  }

  /** Clear image/core state while retaining the visibility owner's slot count
   * and undo-to-redo transition, as on native source replacement and Reset. */
  reset() { this.#resetSource(); }

  #resetSource() {
    const original = {width:0,height:0,bgra:new Uint8Array(),matte:0};
    const classification = {...(this.#state?.classification ?? {quality:0,paletteMode:0,colorCount:0}),category:-1};
    const settings = this.#state?.settings ?? defaultDocumentSettings();
    this.#state = { original,source:{ ...original,bgra:original.bgra.slice() },
      working:{ width:0,height:0,bgra:new Uint8Array() },settings,classification,
      parameters:{...structuredClone(numericData.constructor),usePalette:false,shadowing:true,clusterColors:true},
      stageKeys:{palette:'',reservedC0:'',segmentation:'',segmentationEdits:'',smoothing:'',fitting:''},
      completed:new Uint8Array(25),scores:new Float32Array(25).fill(1e10),segmentation:null,contours:null,smoothing:null,fitting:null,
      palettes:Array.from({length:25},() => new Float32Array()),processed:false,
      status:0,progress:0,cancelled:false };
    this.#candidates = new Array(25);
    this.#runtime = {shadowing:true,clusterColors:true};
    this.#sourceAlphaType = -1;
    this.#fitter = null;this.#fitterRegions = null;
    this.#editor = null;this.#regionPalette = null;this.#committedEditing = {undo:[],redo:[]};
    this.#background.revert();this.#background.finish();
  }

  #installSource({original,shadowControl}) {
    this.#state.original = original;
    this.#state.source = {...original,bgra:original.bgra.slice()};
    this.#runtime.shadowControl = shadowControl;
    this.#state.parameters.shadowControl = shadowControl;
  }

  /** A file-load task clears the old source before reading. Direct decoded
   * setSource edits still validate before replacing anything. The host owns
   * file access/decoding and serializes operations while this task is pending. */
  async runBatchLoad(input,load) {
    if (typeof load !== 'function') throw new TypeError('Document loading requires a source operation.');
    const settings = batchDocumentSettings(this.#state.settings,readBatchControls(input));
    // MainModel synchronizes the selected controls while an image is present.
    // A first failed load cannot commit them to an empty startup document.
    if (this.#state.original.width !== 0) this.#state.settings = settings;
    // Native loading clears the committed colors before file access. The
    // requested profile owns a separate palette, restored only after success.
    this.clearCommittedPalette();
    this.#resetSource();
    try {
      this.#installSource(readDocumentSource(await load()));
      this.#state.settings = settings;
    } catch (error) {
      this.#state.status = 1;
      throw error;
    }
  }

  /** MainModel's cancelled-load completion (0x431c40) resets the source
   * again. Preserve the finished worker result, not its discarded image. */
  cancelLoad() {
    const status = this.#state.status;
    this.clearCommittedPalette();
    this.#resetSource();
    this.#state.status = status;
  }

  snapshot() {
    return { ...structuredClone(this.#state),randomState:this.#session.randomState() };
  }

  /** Store application controls without loading a core profile or running a
   * numerical stage. GUI startup settings differ from raw document defaults. */
  commitSettings(input) {
    this.#state.settings = readDocumentSettings(input);
  }

  /** Application file loading clears committed colors independently of the
   * selected profile and runtime revert copy. Direct setSource callers retain
   * their explicit settings until their controller requests this transition. */
  clearCommittedPalette() {
    this.#state.settings = {...this.#state.settings,palette:new Float32Array()};
  }

  requestCancel() {
    this.#state.cancelled = true;
  }

  #applyProfile(settings,parameters) {
    if (parameters.shadowing !== undefined) {
      this.#runtime.shadowing = parameters.shadowing;
      this.#runtime.clusterColors = parameters.clusterColors;
    }
    Object.assign(parameters,this.#runtime,{usePalette:settings.paletteMode === 0 || settings.paletteMode === 1});
    this.#state.parameters = parameters;
    // Native profile loading transfers supplied colors even on a key hit.
    if (parameters.usePalette && this.#state.segmentation !== null)
      this.#state.segmentation.palette = settings.palette.slice();
  }

  #prepareSource(settings,parameters) {
    const composed = compositeSourceImage(this.#state.original,{enabled:settings.backgroundMode === 1 || settings.backgroundMode === 2,
      color:settings.background});
    this.#state.source = filterPreparedImage({...composed,mode:parameters['Preprocessor::filter_type'],
      amount:parameters['Preprocessor::reduce_noise_order']});
    this.#state.processed = parameters['Preprocessor::filter_type'] !== 0;
  }

  analyze(input) {
    return this.#analyze(readDocumentSettings(input),readPaletteControls(input));
  }

  /** Classification recommendations have their own lifetime. Native success
   * commits requested settings, leaves vector geometry/candidate caches intact
   * and invalidates only the segmentation key. */
  classify(input) {
    const settings = readDocumentSettings(input),state = this.#state;
    if (typeof input.extension !== 'string') throw new Error('Document classification requires an explicit extension string.');
    if (state.original.width < 2 || state.original.height < 2)
      throw new Error('Document classification needs dimensions of at least 2 by 2.');
    const parameters = {...resolveParameters({stage:'classification'}),...this.#runtime,usePalette:false};
    state.cancelled = false;state.progress = 0;
    this.#prepareSource(settings,parameters);
    state.stageKeys.segmentation = '';
    // Analysis owns its diagnostic buffers; internal live state keeps separate
    // copies so returned features, pixels and candidates cannot edit the core.
    const source = {...state.source,bgra:state.source.bgra.slice()};
    // Document 0x46d910 prepares full-size pixels via 0x473850 before type
    // analysis. The later palette controller performs the working reduction.
    const analysis = this.#session.classify({image:{source,working:{...source,bgra:source.bgra.slice()}},
      parameters,extension:input.extension,paletteControls:{bins:parameters['PaletteFinder::use_16_hist_bins'] ? 16 : 8,
        connected:Boolean(parameters['PaletteFinder::use_26_connected_peaks']),costPerColor:parameters['PaletteFinder::cost_per_color']}});
    state.working = {...analysis.image.working,bgra:analysis.image.working.bgra.slice()};
    state.parameters = structuredClone(analysis.parameters);
    if (state.segmentation !== null) {
      const candidate = analysis.paletteAnalysis.candidates.at(-1);
      Object.assign(state.segmentation,{width:state.working.width,height:state.working.height,
        indices:candidate.indices.slice(),palette:candidate.palette.slice()});
    }
    const {imageType:category,quality} = analysis.imageAnalysis.classification;
    const {mode:paletteMode,colorCount} = analysis.paletteAnalysis.classification;
    state.classification = {category,quality,paletteMode,colorCount};
    state.progress = 100;state.settings = settings;
    return {state:this.snapshot(),analysis};
  }

  /** Completed Auto policy from the real MainModel. Selected controls are
   * distinct from committed settings when a controller stage is skipped.
   * Observers own their snapshots and never supply algorithm inputs. */
  runAutomatic(input,observeStage) {
    if (input === null || typeof input !== 'object' || Array.isArray(input))
      throw new Error('Automatic processing requires controls with an extension.');
    if (typeof input.extension !== 'string') throw new Error('Automatic processing requires an extension string.');
    if (observeStage !== undefined && typeof observeStage !== 'function') throw new Error('Automatic observer must be a function.');
    const onProgress = readProgress(input);
    let settings = readDocumentSettings({...this.#state.settings,...input});
    settings.advanced = false;
    this.sourceAlphaType();
    let completed = this.runTask('classification',{...settings,extension:input.extension}),state = completed.state;
    const {category,quality,paletteMode} = state.classification;
    const recommendedBackgroundMode = Number(category === 1 && this.#sourceAlphaType === 1);
    settings = {...settings,category,quality,paletteMode,backgroundMode:recommendedBackgroundMode,
      background:Float32Array.of(1,1,1,recommendedBackgroundMode)};
    let paletteCount = null;
    const publish = stage => observeStage?.(stage,structuredClone(state),
      {settings:structuredClone(settings),paletteCount,recommendedBackgroundMode,didWork:completed.didWork});
    publish('classification');
    if (paletteMode === 0 || paletteMode === 1) {
      completed = this.runTask('palette',{...settings,lower:2,upper:12,
        onProgress:onProgress === undefined ? undefined : event => onProgress({stage:'palette',status:this.#state.status,...event})});
      state = completed.state;
      paletteCount = minimumPaletteCount(state);
      if (paletteCount === null) throw new Error('Automatic palette selection found no eligible candidate.');
      settings = {...settings,palette:selectedPalette(state,paletteCount)};
      publish('palette');
    }
    for (const stage of ['segmentation','smoothing','fitting']) {
      completed = this.runTask(stage,{...settings,
        onProgress:onProgress === undefined ? undefined : event => onProgress({stage,status:this.#state.status,...event})});
      state = completed.state;
      if (completed.didWork && state.status === 2) throw new CancelTaskException(`Automatic ${stage} was cancelled.`);
      publish(stage);
    }
    return {settings,paletteCount,sourceAlphaType:this.#sourceAlphaType,recommendedBackgroundMode,state};
  }

  /** Native alpha classification is source-owned and independent of later
   * background composition. Batch and single-image policies share its cache. */
  sourceAlphaType() {
    if (this.#sourceAlphaType < 0) {
      this.#sourceAlphaType = 0;
      for (let offset = 3; offset < this.#state.original.bgra.length; offset += 4) {
        const alpha = this.#state.original.bgra[offset];
        if (alpha > 0 && alpha < 255) {this.#sourceAlphaType = 2;break;}
        if (alpha === 0) this.#sourceAlphaType = 1;
      }
    }
    return this.#sourceAlphaType;
  }

  /** Batch processing reuses the same core stages and application random
   * stream. Its shared controls can retain explicit choices while other
   * choices remain automatic. Queue execution and export belong to the host. */
  runBatchImage(input,observeStage) {
    const requested = readBatchControls(input);
    if (typeof input.extension !== 'string') throw new Error('Batch image processing requires an extension string.');
    if (observeStage !== undefined && typeof observeStage !== 'function') throw new Error('Batch observer must be a function.');
    const {onProgress} = input;
    if (onProgress !== undefined && typeof onProgress !== 'function') throw new TypeError('Batch progress observer must be a function.');
    let settings = batchDocumentSettings(this.#state.settings,requested);
    const sourceAlphaType = this.sourceAlphaType();
    const classify = !requested.advanced && [requested.category,requested.quality,requested.paletteMode].includes(3);
    let state = classify ? this.runTask('classification',{...settings,extension:input.extension}).state : this.snapshot();
    if (classify) observeStage?.('classification',structuredClone(state));
    settings = {...settings,
      category:classify && requested.category===3 ? state.classification.category : requested.category,
      quality:classify && requested.quality===3 ? state.classification.quality : requested.quality,
      paletteMode:classify && requested.paletteMode===3 ? state.classification.paletteMode : requested.paletteMode};
    const recommendedBackgroundMode = Number(settings.category===1 && sourceAlphaType===1);
    if (!requested.advanced && requested.backgroundMode===3) settings = {...settings,backgroundMode:recommendedBackgroundMode,
      background:Float32Array.of(1,1,1,recommendedBackgroundMode)};
    let paletteCount = null;
    // Basic auto-detect still evaluates 2..12 when classification resolves to
    // Flexible Palette. Advanced fixed mode evaluates only its requested count;
    // the other advanced modes retain supplied colors and skip this task.
    if (requested.advanced ? requested.paletteMode===1 : requested.paletteMode!==2) {
      state = this.runTask('palette',{...settings,
        lower:requested.advanced ? requested.colorCount : 2,
        upper:requested.advanced ? requested.colorCount : 12,
        onProgress:event => onProgress?.({stage:'palette',status:this.#state.status,...event})}).state;
      observeStage?.('palette',structuredClone(state));
      // Only a requested Fixed Palette permits the explicit count override.
      if (requested.advanced) paletteCount = requested.colorCount;
      else if (settings.paletteMode===0) paletteCount = 2;
      else if (requested.paletteMode===1 && requested.colorCount) paletteCount = requested.colorCount;
      if (paletteCount === null) paletteCount = minimumPaletteCount(state);
      if (paletteCount === null) throw new Error('Batch palette selection found no eligible candidate.');
      settings = {...settings,palette:selectedPalette(state,paletteCount)};
    }
    for (const stage of ['segmentation','smoothing','fitting']) {
      state = this.runTask(stage,{...settings,
        onProgress:event => onProgress?.({stage,status:this.#state.status,...event})}).state;
      if (state.status === 2) throw new CancelTaskException(`Batch contour ${stage} was cancelled.`);
      observeStage?.(stage,structuredClone(state),{paletteCount});
    }
    return {settings,paletteCount,sourceAlphaType,recommendedBackgroundMode,state};
  }

  #analyze(settings,{lower,upper,onProgress}) {
    const parameters = resolveParameters(settings.advanced
      ? { stage:'palette',mode:'advanced',colorSensitivity:levels[settings.sensitivity],antiAlias:settings.antiAlias }
      : { stage:'palette',mode:'basic',category:categories[settings.category],quality:levels[settings.quality] });
    const state = this.#state, last = Math.max(lower,upper);
    if (parameters['Shared::is_anti_aliased'] && (state.original.width < 2 || state.original.height < 2)) {
      throw new Error('Anti-alias document analysis needs dimensions of at least 2 by 2.');
    }
    state.cancelled = false;
    this.#applyProfile(settings,parameters);
    const key = documentPaletteKey(settings), changed = key !== state.stageKeys.palette;
    if (changed) state.completed.fill(0);
    state.stageKeys.palette = key;
    let needsWork = changed;
    for (let count = lower; count <= last; count++) needsWork ||= !state.completed[count];
    state.progress = 0;
    if (needsWork) {
      this.#prepareSource(settings,parameters);
      state.working = reduceWorkingImage({ ...state.source,matte:1,antiAlias:parameters['Shared::is_anti_aliased'] });
      const prepared = this.#session.prepare({ ...state.working,antiAlias:Boolean(parameters['Shared::is_anti_aliased']),
        bins:parameters['PaletteFinder::use_16_hist_bins'] ? 16 : 8,
        connected:Boolean(parameters['PaletteFinder::use_26_connected_peaks']),costPerColor:parameters['PaletteFinder::cost_per_color'],
        edgeScale:parameters['EdgeImage::edge_weight_scale'],edgeFloor:parameters['EdgeImage::edge_weight_floor'],
        onProgress:(colorCount,iteration,live) => {
          // Candidate colors change before the first assignment. Same-size
          // labels survive until that producer runs; resizing invalidates them.
          state.segmentation ??= {counts:new Int32Array(),colorIndices:new Int32Array(),indices:null};
          const map = state.segmentation;
          if (map.width !== state.working.width || map.height !== state.working.height) map.indices = null;
          Object.assign(map,{width:state.working.width,height:state.working.height,palette:live.palette});
          if (live.indices !== null) map.indices = live.indices;
          // Native 0x46f790 scales each of ten refinement iterations by 50.
          // 0x46ead0 caps the per-color portion, then multiplies by the range reciprocal.
          const fraction = (Math.min(iteration*5*0.04,1)+(colorCount-lower))*(1/(last-lower+1));
          state.progress = Math.trunc(fraction*100+0.5);
          onProgress?.({colorCount,iteration,fraction,percent:state.progress});
          if (state.cancelled) {
            throw new CancelTaskException('Palette analysis was cancelled.');
          }
        } });
      for (let count = lower; count <= last; count++) {
        if (state.completed[count]) continue;
        const candidate = prepared.evaluate(count);
        this.#candidates[count] = candidate;
        // Palette work creates the live label/color map before any regions
        // exist. Region counts and the completed region palette have their
        // own lifetime; map allocation alone must not enable editing.
        state.segmentation ??= {counts:new Int32Array(),colorIndices:new Int32Array()};
        Object.assign(state.segmentation,{width:state.working.width,height:state.working.height,
          indices:candidate.indices,palette:candidate.palette});
        const argb = new Float32Array(candidate.palette.length);
        for (let offset = 0; offset < argb.length; offset += 4) {
          for (let channel = 0; channel < 4; channel++) argb[offset+channel] = candidate.palette[offset+3-channel];
        }
        state.palettes[count] = argb;
        state.scores[count] = candidate.score.total;
        state.completed[count] = 1;
      }
      if (lower === 2 && last === 12) {
        const { classification } = prepared.classify(this.#candidates.slice(2,13));
        let minimum = lower, minimumScore = state.scores[lower];
        for (let count = lower; count < state.scores.length; count++) {
          if (state.scores[count] < minimumScore) { minimum = count; minimumScore = state.scores[count]; }
          state.scores[count] = 1;
        }
        if (classification.mode === 2) state.scores[minimum] = 0.75;
        else state.scores[classification.colorCount] = 0;
      }
    }
    state.status = 0;state.progress = 100;
    state.settings = settings;
    return this.snapshot();
  }

  segment(input) {
    return this.#segment(readDocumentSettings(input),readProgress(input));
  }

  #segment(settings,onProgress) {
    if ((settings.paletteMode === 0 || settings.paletteMode === 1) && settings.palette.length === 0) throw new Error('Fixed document segmentation needs at least one palette color.');
    const parameters = resolveParameters(settings.advanced
      ? {stage:'segmentation',mode:'advanced',complexity:settings.complexity,minimumPixels:settings.minimumPixels,
        artifactRejection:rejections[settings.artifactRejection],clusterColors:settings.clusterColors}
      : {stage:'segmentation',mode:'basic',category:categories[settings.category],quality:levels[settings.quality]});
    const state = this.#state;
    if (parameters['Shared::is_anti_aliased'] && (state.original.width < 2 || state.original.height < 2))
      throw new Error('Anti-alias document segmentation needs dimensions of at least 2 by 2.');
    state.cancelled = false;state.progress = 0;
    this.#applyProfile(settings,parameters);
    const key = documentSegmentationKey(settings),needsWork = key !== state.stageKeys.segmentation;
    state.stageKeys.segmentation = key;
    if (needsWork) {
      this.#prepareSource(settings,parameters);
      // Segmentation uses the full source. Palette reduction owns a different
      // preparation and can later replace the live working/grid dimensions.
      state.working = {...state.source,bgra:state.source.bgra.slice()};
      const palette = parameters.usePalette ? settings.palette : undefined;
      const owner = new SegmentationState(state.segmentation,state.working.width,state.working.height,palette);
      this.#regionPalette = null;
      if (state.fitting) state.fitting = {...state.fitting,complete:false};
      let segmented;
      try {
        segmented = owner.run({...state.working,parameters,palette},(event,live) => {
          state.segmentation = live;state.progress = event.percent;
          onProgress?.(event);
          if (state.cancelled) throw new CancelTaskException('Segmentation was cancelled.');
        });
      } catch (error) {
        state.stageKeys.segmentation = '';state.stageKeys.segmentationEdits = '';
        state.stageKeys.smoothing = '';state.stageKeys.fitting = '';
        if (!(error instanceof CancelTaskException)) throw error;
        state.status = 2;state.progress = 100;
        return this.snapshot();
      }
      state.segmentation = segmented;
      this.#regionPalette = segmented.palette.slice();this.#editor = null;this.#committedEditing = {undo:[],redo:[]};
      state.contours = null;state.smoothing = null;state.fitting = null;
      state.stageKeys.segmentationEdits = '';state.stageKeys.smoothing = '';state.stageKeys.fitting = '';
    }
    state.status = 0;state.progress = 100;state.settings = settings;
    return this.snapshot();
  }

  #vectorParameters(settings,stage) {
    return resolveParameters(settings.advanced ? {stage,mode:'advanced',smoothness:settings.smoothness,
      curveComplexity:settings.curveComplexity,detectCorners:settings.detectCorners,antiAlias:settings.smoothingAntiAlias}
      : {stage,mode:'basic',category:categories[settings.category],quality:levels[settings.quality]});
  }

  /** Editing owns a base bitmap independent of later live palette transfers.
   * Native history replays onto this base, not onto the last edited result. */
  beginEditing() {
    if (this.#state.segmentation === null || this.#regionPalette === null) throw new Error('Document editing needs completed segmentation.');
    this.#background.finish();
    if (this.#editor === null) {
      const {width,height,indices,colorIndices} = this.#state.segmentation;
      const bgra = new Uint8Array(width*height*4),palette = this.#regionPalette;
      for (let pixel = 0; pixel < indices.length; pixel++) for (let channel = 0; channel < 4; channel++)
        bgra[pixel*4+channel] = Math.round(palette[colorIndices[indices[pixel]]*4+channel]*255);
      const {backgroundMode,background} = this.#state.settings;
      this.#editor = new SegmentationEditor({width,height,bgra,source:this.#state.original.bgra,backgroundMode,background,
        color:palette.subarray(0,4)});
    }
    return this.#editor.snapshot();
  }

  #editingSession() {
    if (this.#editor === null) throw new Error('Call beginEditing before editing the document.');
    return this.#editor;
  }

  edit(command) { return this.#editingSession().apply(command); }
  editingState() { return this.#editor?.snapshot() ?? null; }

  editingHistory() { return this.#editor?.historyState() ?? {undoCount:0,redoCount:0}; }

  leaveEditing() { this.#editingSession().endEditing(); }

  restoreCommittedEditing() { return this.#editor?.restoreCheckpoint(this.#committedEditing) ?? null; }
  gesture(event) { return this.#editingSession().gesture(event,this.#pointer); }
  undoEdit() { return this.#editingSession().undo(); }
  redoEdit() { return this.#editingSession().redo(); }
  revertEdits() { return this.#editingSession().revert(); }

  backgroundState() { return this.#background.snapshot(); }
  beginBackgroundEditing() {
    const background = this.#background.begin();
    this.#editor?.endEditing();return background;
  }
  backgroundGesture(event) { return this.#background.gesture(event,this.#state.segmentation,this.#state.contours,this.#pointer); }
  toggleBackground(point) { return this.#background.toggle(point,this.#state.segmentation,this.#state.contours); }
  undoBackground() { return this.#background.undo(); }
  redoBackground() { return this.#background.redo(); }
  revertBackground() { return this.#background.revert(); }
  finishBackgroundEditing() { return this.#background.finish(); }

  /** Native Next commits the serialized history before vector tasks. Regions
   * retain their creation colors even when task profiles replace the palette. */
  commitEditing() {
    const editor = this.#editingSession().snapshot(),state = this.#state;
    this.#committedEditing = this.#editor.historyCheckpoint();
    this.#editor.endEditing();
    this.#background.finish();
    if (editor.key !== state.stageKeys.segmentationEdits) {
      const {width,height,bgra} = editor;
      function* colors() {
        for (let offset = 0; offset < bgra.length; offset += 4) yield bgra.subarray(offset,offset+4);
      }
      const {palette,colorIndices:labels} = paletteFromByteColors(colors());
      const previous = Array.from({length:palette.length/4},(_,colorIndex) => ({colorIndex}));
      const {indices,statistics} = rebuildConnectedRegions({width,bgra,regions:labels},previous);
      state.segmentation = {width,height,indices,palette,counts:Int32Array.from(statistics,region => region.count),
        colorIndices:Int32Array.from(statistics,region => region.colorIndex)};
      this.#regionPalette = palette.slice();
      state.stageKeys.segmentationEdits = editor.key;
      // Native rebuilds region boundary containers but retains cache strings
      // and old node allocation. Expose incomplete stages as unavailable; task
      // cache decisions still use the original retained keys.
      state.contours = null;state.smoothing = null;state.fitting = null;
    }
    return this.snapshot();
  }

  finishEditing(input = this.#state.settings,onStage) {
    const settings = readDocumentSettings(input),onProgress = readProgress(input);
    if (onStage !== undefined && typeof onStage !== 'function') throw new Error('Editing stage observer must be a function.');
    this.commitEditing();
    for (const stage of ['smoothing','fitting']) {
      const completed = this.runTask(stage,{...settings,
        onProgress:onProgress === undefined ? undefined : event => onProgress({stage,...event})});
      // A skipped task retains its preceding result, including cancellation.
      if (completed.didWork && completed.state.status === 2) throw new CancelTaskException(`Editing ${stage} was cancelled.`);
      onStage?.(stage,completed);
    }
    // Completed-task observers own their snapshots, separate from this result.
    return this.snapshot();
  }

  smooth(input) {
    return this.#smooth(readDocumentSettings(input),readProgress(input));
  }

  #retainSmoothing(smoothed) {
    const state = this.#state;
    state.contours = smoothed.geometry;
    state.smoothing = {borderCounts:smoothed.borderCounts,referencePositions:smoothed.referencePositions,
      penaltyCounts:smoothed.penaltyCounts};
  }

  #bindSmoothing() {
    const state = this.#state;
    // Smoothing retains region records; segmentation/edit reconstruction
    // creates a new identity. Cubic storage outlives either operation.
    if (this.#fitter === null) this.#fitter = new ContourFitter(state.contours);
    else if (this.#fitterRegions === state.segmentation) this.#fitter.setContours(state.contours);
    else this.#fitter.setRegions(state.contours);
    this.#fitterRegions = state.segmentation;
  }

  #smooth(settings,onProgress) {
    const state = this.#state;
    if (this.#regionPalette === null) throw new Error('Document smoothing needs completed segmentation.');
    const parameters = this.#vectorParameters(settings,'smoothing');
    state.cancelled = false;state.progress = 0;
    this.#applyProfile(settings,parameters);
    const key = documentSmoothingKey(settings,state.stageKeys.segmentationEdits),needsWork = key !== state.stageKeys.smoothing;
    state.stageKeys.smoothing = key;
    if (needsWork) {
      const geometry = extractContours({...state.segmentation,palette:this.#regionPalette,antiAlias:Boolean(parameters['Shared::is_anti_aliased'])});
      state.fitting = null;
      // Geometry retains region creation colors. Coordinate initialization
      // samples the live palette that profile loading transferred above.
      try {
        const smoothed = this.#session.smooth({...state.working,...state.segmentation,geometry,parameters},(event,live) => {
          this.#retainSmoothing(live);
          state.progress = event.percent;
          onProgress?.(event);
          if (state.cancelled) throw new CancelTaskException('Contour smoothing was cancelled.');
        });
        this.#retainSmoothing(smoothed);
      } catch (error) {
        state.stageKeys.smoothing = '';
        state.stageKeys.fitting = '';
        if (state.contours !== null) this.#bindSmoothing();
        if (!(error instanceof CancelTaskException)) throw error;
        state.status = 2;state.progress = 100;
        return this.snapshot();
      }
      state.fitting = null;state.stageKeys.fitting = '';
      this.#bindSmoothing();
    }
    state.status = 0;state.progress = 100;state.settings = settings;
    return this.snapshot();
  }

  fit(input) {
    return this.#fit(readDocumentSettings(input),readProgress(input));
  }

  #syncFitting() {
    const {cubicAvailability,...geometry} = this.#fitter.snapshot(),state = this.#state;
    for (const [index,node] of geometry.nodes.entries()) state.contours.nodes[index].kind = node.kind;
    state.fitting = {complete:false,geometry,cubicAvailability,outputChains:state.fitting?.outputChains ?? null};
  }

  #fit(settings,onProgress) {
    const state = this.#state;
    if (state.contours === null) throw new Error('Document fitting needs completed contours.');
    const parameters = this.#vectorParameters(settings,'fitting');
    state.cancelled = false;state.progress = 0;
    this.#applyProfile(settings,parameters);
    const key = documentFittingKey(settings,state.stageKeys.segmentationEdits),needsWork = key !== state.stageKeys.fitting;
    state.stageKeys.fitting = key;
    if (needsWork) {
      try {
        const geometry = this.#fitter.run(parameters,event => {
          // Without an observer no host can read partial state during this
          // synchronous call. Cancellation/errors still synchronize below.
          if (onProgress !== undefined) this.#syncFitting();
          state.progress = event.percent;
          onProgress?.(event);
          if (state.cancelled) throw new CancelTaskException('Contour fitting was cancelled.');
        });
        for (const [index,node] of geometry.nodes.entries()) state.contours.nodes[index].kind = node.kind;
        const {children,holes} = assembleOutputShapes(geometry);
        state.fitting = {complete:true,geometry,cubicAvailability:this.#fitter.cubicAvailability,
          outputChains:{children,holes}};
        this.#background.acceptVectors(geometry.regions.length);
      } catch (error) {
        this.#syncFitting();
        state.stageKeys.fitting = '';
        if (!(error instanceof CancelTaskException)) throw error;
        state.status = 2;state.progress = 100;
        return this.snapshot();
      }
    }
    state.status = 0;state.progress = 100;state.settings = settings;
    return this.snapshot();
  }

  /** Complete a single controller request. Native scheduler skips happen
   * before profile loading; their commit rules differ from direct stage hits. */
  runTask(stage,input) {
    if (stage === 'classification') {
      const {state,analysis} = this.classify(input);
      return {didWork:true,settings:structuredClone(state.settings),state,analysis};
    }
    if (typeof stage !== 'string' || !Object.hasOwn(taskKeys,stage)) throw new Error('Document task stage must be classification, palette, segmentation, smoothing or fitting.');
    const settings = readDocumentSettings(input),state = this.#state;
    const controls = stage === 'palette' ? readPaletteControls(input) : undefined;
    const onProgress = stage === 'segmentation' || stage === 'smoothing' || stage === 'fitting' ? readProgress(input) : undefined;
    let didWork = taskKeys[stage](settings,state.stageKeys.segmentationEdits) !== state.stageKeys[stage];
    if (controls) for (let count = controls.lower; count <= Math.max(controls.lower,controls.upper); count++)
      didWork ||= !state.completed[count];
    let snapshot;
    if (!didWork) {
      // Original ThreadManager commits settings only for a skipped palette
      // request. Preserve the last core profile, pixels and cancellation flag.
      if (stage === 'palette') state.settings = settings;
      snapshot = this.snapshot();
    } else if (stage === 'palette') snapshot = this.#analyze(settings,controls);
    else if (stage === 'segmentation') snapshot = this.#segment(settings,onProgress);
    else if (stage === 'smoothing') snapshot = this.#smooth(settings,onProgress);
    else snapshot = this.#fit(settings,onProgress);
    return {didWork,settings:structuredClone(settings),state:snapshot};
  }

  /** Run the host's export operation against this document's progress sink.
   * The host owns file handles and serializers; this receiver owns task state.
   * A failed open returns before any output-walker progress callback. The host
   * serializes document operations until save resolves its uint32 result. */
  async runExport(save) {
    if (typeof save !== 'function') throw new TypeError('Document export requires a save operation.');
    const state = this.#state;
    if (!state.fitting?.complete) throw new Error('Document export needs completed fitted geometry.');
    state.progress = 0;state.cancelled = false;
    const status = await save(percent => {
      state.progress = percent;
      if (state.cancelled) {
        throw new CancelTaskException('Vector export was cancelled.');
      }
    });
    if (!Number.isInteger(status) || status < 0 || status > 0xffffffff)
      throw new Error('Document save must return a uint32 task result.');
    state.status = status;
    return this.snapshot();
  }
}
