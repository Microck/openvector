import { CoreSession } from '../src/palette.mjs';
import { decodeImage, resolveImageFilename } from '../src/image-input.mjs';
import { readDocumentSettings } from '../src/document-controls.mjs';
import { defaultSelectedProfile, defaultBatchProfile, batchProfileControls, readSettingsProfile, runAdvancedProfile } from '../src/settings-profile.mjs';
import { applyPaletteCommand } from '../src/profile-palette.mjs';
import { BatchRunner } from '../src/batch-runner.mjs';
import { applyBackgroundCommand } from '../src/background-editor.mjs';
import { SourceHistory } from '../src/source-history.mjs';
import { wizardPageSettings } from './auto-update.mjs';
import { exportSvg } from '../src/svg.mjs';
import { exportEps } from '../src/eps.mjs';
import { exportPdf } from '../src/pdf.mjs';
import { exportAi } from '../src/ai.mjs';
import { exportDxf } from '../src/dxf.mjs';
import { exportEmf } from '../src/emf.mjs';
import {exportBitmap,bitmapFormats} from '../src/bitmap-export.mjs';
import {readExportOptions,defaultExportOptions} from '../src/export-options.mjs';
import {CancelTaskException} from '../src/image-document.mjs';
import {QuickSave} from './quick-save.mjs';
import {BitmapSize} from './bitmap-size.mjs';
import {resolve} from 'node:path';

const exporters = { svg: exportSvg, eps: exportEps, pdf: exportPdf, ai: exportAi, dxf: exportDxf, emf: exportEmf };
const wireSettings = settings => ({...settings,background:Array.from(settings.background),palette:Array.from(settings.palette)});
function readCount(count) {
  if (!Number.isInteger(count) || count < 2 || count > 24) throw new Error('Number of colors must be an integer from 2 through 24.');
  return count;
}

// The native viewer retains its completed scene while advanced edits await
// processing. Current visibility applies to that scene, including alpha-zero
// outlines. This display geometry never supplies hit tests or file exports.
function vectorPreview(geometry, width, height, hiddenRegions) {
  const options = {geometry,width,height,layout:'cutouts',hiddenRegions};
  const svg = exportSvg(options);
  if (!hiddenRegions.length) return {svg,outlines:[]};
  const hidden = new Set(hiddenRegions);
  const outlineGeometry = {...geometry,regions:geometry.regions.map(region => ({...region,bgra:[...region.bgra.slice(0,3),255]}))};
  const onlyHidden = exportSvg({...options,geometry:outlineGeometry,
    hiddenRegions:geometry.regions.flatMap((_,id) => hidden.has(id) ? [] : [id])});
  return {svg,outlines:Array.from(onlyHidden.matchAll(/<path [^>]* d="([^"]*)"/g),match => match[1])};
}

// Canvas ImageData takes straight RGBA. The document's original backup already
// owns premultiplied BGRA, so only the display copy is unpremultiplied here.
function displayPixels({ width, height, bgra }) {
  const rgba = new Uint8Array(bgra.length);
  for (let offset = 0; offset < bgra.length; offset += 4) {
    const alpha = bgra[offset + 3];
    for (let channel = 0; channel < 3; channel++)
      rgba[offset + channel] = alpha ? Math.round(bgra[offset + 2 - channel] * 255 / alpha) : 0;
    rgba[offset + 3] = alpha;
  }
  return { width, height, rgba: Buffer.from(rgba).toString('base64') };
}

function displaySegments({ width, height, indices, colorIndices, palette }) {
  const bgra = new Uint8Array(width * height * 4);
  for (let pixel = 0; pixel < indices.length; pixel++) {
    const color = colorIndices[indices[pixel]] * 4;
    for (let channel = 0; channel < 4; channel++) bgra[pixel * 4 + channel] = Math.round(palette[color + channel] * 255);
  }
  // The view owns the last completed map. Later palette work mutates core state.
  return displayPixels({ width, height, bgra });
}

// Native smooth scaling uses 16-bit positions, 8-bit interpolation when
// enlarging, and 14-bit area weights when reducing. Integer weights keep a
// constant image constant, including alpha, without repairing output pixels.
function thumbnailAxis(source,target) {
  const up=target>=source,bits=up?8:14,step=Math.floor(source*65536/target);
  const origin=up?Math.floor(source*32768/target)-32768:0;
  const coverage=Math.floor(target*16384/source)+1;
  const samples=Array.from({length:target},(_,index)=>{
    const position=origin+index*step,first=Math.floor(position/65536);
    if(up) {
      const weight=first<0 || first>=source-1?0:Math.floor(position/256)&255;
      return weight ? [[first,256-weight],[first+1,weight]] : [[Math.max(0,first),256]];
    }
    const weight=Math.floor((256-(Math.floor(position/256)&255))*coverage/256);
    const span=[[first,weight]];
    let remaining=16384-weight,pixel=first+1;
    while(remaining>coverage) {span.push([pixel++,coverage]);remaining-=coverage;}
    if(remaining>0)span.push([pixel,remaining]);
    return span;
  });
  return {samples,bits};
}

// Cache the original file preview during the single decode. Document source
// preparation can change dimensions or pixels, so its bitmap is not a file icon.
function sourceThumbnail(image) {
  const [width,height]=SourceHistory.thumbnailDimensions(image.width,image.height);
  if(width===0)return null;
  const horizontal=thumbnailAxis(image.width,width),vertical=thumbnailAxis(image.height,height);
  // The native area-area path truncates each horizontal sum before the
  // vertical sum. Combining both divisions changes low bits in the thumbnail.
  const rowShift=horizontal.bits===14 && vertical.bits===14?5:0;
  const rowDivisor=2**rowShift,divisor=2**(horizontal.bits+vertical.bits-rowShift),bgra=new Uint8Array(width*height*4);
  for(let y=0;y<height;y++)for(let x=0;x<width;x++)for(let channel=0;channel<4;channel++) {
    let value=0;
    for(const [sy,wy] of vertical.samples[y]) {
      let row=0;
      for(const [sx,wx] of horizontal.samples[x]) {
        const offset=(sy*image.width+sx)*4;
        const component=channel===3?image.bgra[offset+3]:Math.floor(image.bgra[offset+channel]*image.bgra[offset+3]/255);
        row+=component*wx;
      }
      value+=Math.floor(row/rowDivisor)*wy;
    }
    bgra[(y*width+x)*4+channel]=Math.floor(value/divisor);
  }
  // QPixmap's cached PNG truncates alpha conversion in both directions. The
  // numerical document's display conversion has a different rounding contract.
  for(let offset=0;offset<bgra.length;offset+=4) {
    const alpha=bgra[offset+3],blue=bgra[offset],red=bgra[offset+2];
    bgra[offset]=alpha?Math.floor(red*255/alpha):0;
    bgra[offset+1]=alpha?Math.floor(bgra[offset+1]*255/alpha):0;
    bgra[offset+2]=alpha?Math.floor(blue*255/alpha):0;
  }
  return {width,height,rgba:Buffer.from(bgra).toString('base64')};
}

/** One application session, shared by every GUI operation. The host never
 * reads Vector Magic, invokes a native probe or substitutes another tracer. */
export class GuiEngine {
  #session = new CoreSession();
  #document = null;
  #file = null;
  #segments = null;
  #profile;
  #paletteRuntime = {backup:new Float32Array(),customized:false};
  #extension = null;
  #editor = null;
  #vectorScene = null;
  #committedCount = 2;
  #continuing = false;
  #outputOptions;
  #quickSave;
  #bitmapSave;
  #bitmapSize;
  #sourceFilename='';
  #workingDirectory;
  #onProgress;

  constructor({profile=defaultSelectedProfile(),outputOptions=defaultExportOptions,quickSave,bitmapSave,workingDirectory=process.cwd(),onProgress}={}) {
    if (onProgress !== undefined && typeof onProgress !== 'function') throw new TypeError('GUI progress observer must be a function.');
    this.#onProgress = onProgress;
    this.#workingDirectory=resolve(workingDirectory);
    // Startup receives validated preferences from the storage boundary. The
    // native selected palette counter starts fresh even when it was saved.
    this.#profile=structuredClone(profile);this.#profile.paletteColorCount=0;
    this.#outputOptions={...outputOptions};
    this.#quickSave=new QuickSave({workingDirectory,preferences:quickSave});
    this.#bitmapSave=new QuickSave({workingDirectory,kind:'bitmap',preferences:bitmapSave && {basic:bitmapSave.basic,advanced:bitmapSave.advanced}});
    this.#bitmapSize=new BitmapSize(bitmapSave?.scale);
    // Native MainModel commits its clean GUI controls before any source exists.
    // Later selected edits and first-source loading must not replace that copy.
    this.#document = this.#session.createDocument();
    this.#document.commitSettings(this.#profile.settings);
  }

  selectedProfile() {return structuredClone(this.#profile);}
  quickSave(input) {return input===undefined?this.#quickSave.snapshot():this.#quickSave.configure(input);}
  quickSavePreferences() {return this.#quickSave.preferences();}
  bitmapSave(input) {return input===undefined?this.#bitmapSave.snapshot():this.#bitmapSave.configure(input);}
  bitmapSavePreferences() {return {...this.#bitmapSave.preferences(),scale:this.#bitmapSize.scale};}
  bitmapSize(input) {
    if(input===undefined)return this.#bitmapSize.snapshot();
    this.#requireDocument();return this.#bitmapSize.command(input);
  }
  quickSaveTarget(mode,kind) {
    const owner=kind==='bitmap'?this.#bitmapSave:this.#quickSave;
    return {...owner.target(mode),sourceFilename:this.#sourceFilename};
  }
  get sourceFilename() {return this.#sourceFilename;}
  outputOptions(input) {
    if(input!==undefined)this.#outputOptions=readExportOptions(input,this.#outputOptions);
    return {...this.#outputOptions};
  }
  get hasSource() {return this.#file !== null;}
  get continuing() {return this.#continuing;}
  paletteRuntime() {return {backup:Array.from(this.#paletteRuntime.backup),customized:this.#paletteRuntime.customized};}
  selectionState() {
    return {settings:wireSettings(this.#profile.settings),count:this.#profile.colorCount,
      paletteColorCount:this.#profile.paletteColorCount,basicColorCount:this.#profile.basicColorCount,
      recommendedBackgroundMode:this.#profile.recommendedBackgroundMode,paletteRuntime:this.paletteRuntime()};
  }
  snapshot() {return {profile:this.selectedProfile(),paletteRuntime:this.paletteRuntime(),background:this.#document.backgroundState(),
    committedCount:this.#committedCount,document:this.#document.snapshot()};}

  palette(command) {
    applyPaletteCommand(this.#profile,this.#paletteRuntime,command);
    return this.selectionState();
  }

  configureProfile(patch) {
    if (patch === null || typeof patch !== 'object' || Array.isArray(patch)) throw new Error('Expected selected profile changes.');
    if (Object.hasOwn(patch,'settings') && (patch.settings === null || typeof patch.settings !== 'object' || Array.isArray(patch.settings)))
      throw new Error('Expected document controls.');
    const profile = readSettingsProfile({...this.#profile,...patch,settings:{...this.#profile.settings,...patch.settings}});
    this.#profile = profile;
    return this.selectedProfile();
  }

  select(patch) {
    this.configureProfile(patch);
    return this.selectionState();
  }

  applyProfile(input) {
    const profile = readSettingsProfile(input);
    // Preserve native ordering: the stored advanced flag is committed before
    // the selected GUI copy enters advanced mode. No numerical task runs here.
    this.#document.commitSettings(profile.settings);
    this.#committedCount = profile.colorCount;
    this.#profile = {...profile,settings:{...profile.settings,advanced:true}};
    return this.selectedProfile();
  }

  open({ bytes, name, settings, count = this.#profile.colorCount }) {
    readCount(count);
    if (typeof name !== 'string' || name.length === 0 || name.includes('\0')) throw new Error('Choose an image file.');
    const selected = settings === undefined ? this.#profile.settings : readDocumentSettings(settings);
    const decoded = decodeImage(bytes);
    // Finish decoding/preparation first so an invalid upload preserves work.
    // Native loading derives shadow acceptance from the source filename,
    // independently of the image encoding detected in the uploaded bytes.
    const {shadowControl, extension} = resolveImageFilename(name);
    this.#bitmapSize.sourceChanged(decoded);
    const retainedSource = this.#file !== null;
    this.#document.setSource({...decoded,shadowControl});
    // setSource validates before replacement and consumes no processing
    // controls. Only a successful replacement may commit the selected draft.
    if (retainedSource) {
      this.#document.commitSettings(selected);this.#committedCount = count;
    }
    this.#document.clearCommittedPalette();
    this.#profile.settings = selected;
    if (retainedSource) {
      applyPaletteCommand(this.#profile,this.#paletteRuntime,{action:'source'});
      this.#profile.settings = {...this.#profile.settings,backgroundMode:-1,background:Float32Array.of(1,1,1,0)};
      this.#profile.recommendedBackgroundMode = -1;
    }
    this.#extension = extension;
    this.#profile.colorCount = count;
    this.#segments = null;
    this.#editor = null;
    this.#vectorScene = null;
    this.#file = { name: name.split(/[\\/]/).at(-1), hasAlpha: decoded.hasAlpha, format: decoded.format,
      sourceWidth:decoded.width,sourceHeight:decoded.height,thumbnail:sourceThumbnail(decoded) };
    this.#sourceFilename=resolve(this.#workingDirectory,name);
    this.#quickSave.sourceChanged(name);
    this.#bitmapSave.sourceChanged(name.replace(/\.[^./\\]+$/,'')+'_vectorized');
    this.#continuing = false;
    return this.state(true);
  }

  /** Both native palette pages commit selected controls when they are left,
   * independently of whether navigation starts processing or returns to Load. */
  leavePalette() {
    this.#requireDocument();
    this.#document.commitSettings(this.#profile.settings);
    this.#committedCount = this.#profile.colorCount;
    return {...this.selectionState(),committedCount:this.#committedCount,committedSettings:wireSettings(this.#profile.settings)};
  }

  reset() {
    this.#document.reset();
    // Native full reset commits retained processing choices to the blank
    // document before clearing selected palette/transparency metadata.
    this.#document.commitSettings({...this.#profile.settings,palette:new Float32Array()});
    this.#committedCount = this.#profile.colorCount;
    applyPaletteCommand(this.#profile,this.#paletteRuntime,{action:'source'});
    this.#profile.settings = {...this.#profile.settings,backgroundMode:-1,background:Float32Array.of(1,1,1,0)};
    this.#profile.recommendedBackgroundMode = -1;
    this.#continuing = false;
    this.#resetSourceFile();
    this.#segments = null;
    this.#extension = null;
    this.#editor = null;
    this.#vectorScene = null;
    // Closing an image does not reset the application's retained random stream.
    return null;
  }

  /** Model source reset also notifies window metadata owners. The document
   * performs its own reset first, including the completed-load cancel path. */
  #resetSourceFile() {
    this.#file=null;
    this.#sourceFilename='';
    this.#quickSave.sourceChanged('');
    this.#bitmapSave.sourceChanged('');
  }

  /** The model's factory reset clears processing choices before source reset.
   * Its intermediate committed background is zero RGBA, unlike GUI startup. */
  resetProcessing() {
    this.#profile = defaultSelectedProfile();
    this.#profile.settings.background.fill(0);
    return this.reset();
  }

  state(includeBitmap = false) {
    if (!this.#document || !this.#file) return null;
    const state = this.#document.snapshot();
    const { width, height } = state.original;
    const background = this.#document.backgroundState();
    const fitting = state.fitting?.complete ? state.fitting : null;
    const curves = new Set();
    for (const region of fitting?.geometry.regions ?? [])
      for (const edge of region.edges) if (edge[1] !== 0) curves.add(edge[0]);
    return {
      file: this.#file, width, height,quickSave:this.quickSave(),bitmapSave:this.bitmapSave(),bitmapSize:this.bitmapSize(),
      stageKeys: state.stageKeys,
      ...this.selectionState(),committedCount:this.#committedCount,committedSettings:wireSettings(state.settings),
      segmentationHistory:this.#document.editingHistory(),
      palettes: state.palettes.flatMap((argb, count) => state.completed[count] ? [{
        count, score: state.scores[count],
        // Captured palette candidates use ARGB; document controls use BGRA.
        colors: Array.from({ length: count }, (_, color) => Array.from(argb.slice(color * 4, color * 4 + 4)).reverse()),
      }] : []),
      regionCount: state.segmentation?.counts.length ?? 0,
      curveCount: curves.size,
      stage: fitting ? 'fitting' : state.contours ? 'smoothing' : state.segmentation?.counts.length ? 'segmentation' : 'image',
      svg: fitting ? exportSvg({ geometry: fitting.geometry, width, height, hiddenRegions:background.hiddenRegions }) : null,
      preview: this.#vectorScene ? vectorPreview(this.#vectorScene,width,height,background.hiddenRegions) : null,
      background,
      bitmap: includeBitmap ? displayPixels(state.original) : undefined,
      segments: this.#segments,
      editor: this.#editor ? {undoCount:this.#editor.undo.length,redoCount:this.#editor.redo.length,key:this.#editor.key,
        tool:this.#editor.tool,color:Array.from(this.#editor.color)} : null,
    };
  }

  palettes({ settings, lower = 2, upper = 12, count = this.#profile.colorCount }) {
    this.#findPalettes({settings,lower,upper,count});
    return this.state();
  }

  #findPalettes({settings,lower,upper,count}) {
    readCount(count);
    const completed = this.#requireDocument().runTask('palette', { ...settings, lower, upper,
      onProgress:this.#progressObserver('palette') });
    this.#completedTask('palette',completed);
    this.#profile.settings = readDocumentSettings(settings);
    this.#profile.colorCount = count;this.#committedCount = count;
    return completed.state;
  }

  #progressObserver(stage) {
    if (this.#onProgress === undefined) return undefined;
    return event => this.#reportProgress({stage,...event});
  }

  #reportProgress(event) {
    if (this.#onProgress({status:null,...event,complete:false})) this.#document.requestCancel();
  }

  #completedTask(stage,{state,didWork}) {
    if (didWork && state.status === 2) throw new CancelTaskException(`${stage} was cancelled.`);
    if (this.#onProgress?.({stage,percent:state.progress,status:didWork ? state.status : null,complete:true})) {
      this.#document.requestCancel();
      throw new CancelTaskException(`${stage} was cancelled.`);
    }
  }

  /** Replay uses stored palette metadata for its search bounds. The advanced
   * spinner count is selected state, and changes only when colors are chosen. */
  fastForwardProfile() {
    const previousCount = this.#profile.colorCount;
    let smoothed = false;
    const replay = runAdvancedProfile(this.#requireDocument(),this.#profile,{paletteRuntime:this.#paletteRuntime,
      onProgress:this.#onProgress === undefined ? undefined : event => this.#reportProgress(event),onStage:(stage,completed)=>{
      this.#completedTask(stage,completed);
      this.#profile = completed.profile;this.#paletteRuntime = completed.paletteRuntime;
      if (stage === 'palette') this.#committedCount = previousCount;
      else if (stage === 'segmentation' && completed.didWork) {
        this.#committedCount = this.#profile.colorCount;
        this.#segments = displaySegments(completed.state.segmentation);this.#editor = null;
      } else if (stage === 'smoothing') smoothed = completed.didWork;
      else if (stage === 'fitting') {
        if (smoothed || completed.didWork) this.#committedCount = this.#profile.colorCount;
        if (completed.state.fitting?.complete) this.#vectorScene = completed.state.fitting.geometry;
      }
    }});
    if (replay === null) return null;
    return this.state();
  }

  startProfileWizard() {
    this.#requireDocument();
    this.#continuing = true;
    const page = this.#document.sourceAlphaType() !== 0 ? 'transparency' : 'palette';
    this.#profile.settings = wizardPageSettings(page,this.#profile.settings);
    return page;
  }

  automatic({ settings, count = this.#profile.colorCount }) {
    readCount(count);
    this.#requireDocument().runAutomatic({ ...settings, extension: this.#extension,
      onProgress:this.#onProgress === undefined ? undefined : event => this.#reportProgress(event)},(stage,state,selection) => {
      this.#completedTask(stage,{state,didWork:selection.didWork});
      this.#profile.settings = selection.settings;
      this.#profile.colorCount = selection.paletteCount ?? count;
      this.#profile.recommendedBackgroundMode = selection.recommendedBackgroundMode;
      if (stage === 'palette') applyPaletteCommand(this.#profile,this.#paletteRuntime,{action:'replace',palette:selection.settings.palette});
      if (stage === 'segmentation') {
        this.#committedCount = this.#profile.colorCount;
        this.#segments = displaySegments(state.segmentation);this.#editor = null;
      } else if (stage === 'fitting') {
        this.#committedCount = this.#profile.colorCount;this.#vectorScene = state.fitting.geometry;
      }
    });
    return this.state();
  }

  /** Review changes the existing selected profile. It must not reclassify the
   * source or reset controls through the Fully Automatic entry point. */
  review(input) {
    const actions=['artwork','high','medium','low','custom-colors','unlimited-colors','edit','hand-pick','background'];
    if(!input || typeof input!=='object' || Array.isArray(input) || Object.keys(input).length!==1 || !actions.includes(input.action))
      throw new Error('Choose artwork, high, medium, low, custom-colors, unlimited-colors, edit, hand-pick or background for review.');
    this.#requireDocument();
    const {action}=input;let page='review';
    if(action==='artwork' || action==='custom-colors') {
      const settings={...this.#profile.settings,paletteMode:1,...(action==='artwork'?{category:1}:{})};
      const candidates=this.#findPalettes({settings,lower:2,upper:12,count:this.#profile.colorCount});
      let chosen=2;
      for(let count=3;count<=12;count++)if(candidates.scores[count]<candidates.scores[chosen])chosen=count;
      const argb=candidates.palettes[chosen],palette=Float32Array.from(argb,(_,index)=>argb[Math.floor(index/4)*4+3-index%4]);
      applyPaletteCommand(this.#profile,this.#paletteRuntime,{action:'choose',palette});page='palette';
    } else if(['high','medium','low','unlimited-colors'].includes(action)) {
      const settings={...this.#profile.settings,...(action==='unlimited-colors'?{paletteMode:2}:{quality:['high','medium','low'].indexOf(action)})};
      this.process({settings,through:'fitting'});
    } else if(action==='edit') {this.edit({action:'begin'});page='editing';}
    else if(action==='background') {this.background({action:'begin'});page='background';}
    else page='type';
    return {document:this.state(),selection:this.selectionState(),page};
  }

  process({ settings, through, count = this.#profile.colorCount }) {
    readCount(count);
    if (through !== 'segmentation' && through !== 'fitting') throw new Error('Choose segmentation or fitting as the final stage.');
    const document = this.#requireDocument();
    const segmented = document.runTask('segmentation', {...settings,onProgress:this.#progressObserver('segmentation')});
    this.#profile.settings = readDocumentSettings(settings);
    this.#profile.colorCount = count;
    this.#completedTask('segmentation',segmented);
    if (segmented.didWork) {
      this.#committedCount = count;
      this.#segments = displaySegments(segmented.state.segmentation);
      this.#editor = null;
    }
    if (through === 'fitting') {
      this.#fit(settings);
    }
    return this.state();
  }

  vectors({settings,count = this.#profile.colorCount}) {
    readCount(count);this.#profile.colorCount = count;
    this.#requireDocument();
    this.#profile.settings = readDocumentSettings(settings);
    const didWork = this.#fit(settings);
    return {...this.state(),didWork};
  }

  #fit(settings) {
    const smoothed = this.#document.runTask('smoothing',{...settings,onProgress:this.#progressObserver('smoothing')});
    this.#completedTask('smoothing',smoothed);
    const fitted = this.#document.runTask('fitting',{...settings,onProgress:this.#progressObserver('fitting')});
    this.#completedTask('fitting',fitted);
    const {state,didWork} = fitted;
    if (state.fitting?.complete) this.#vectorScene = state.fitting.geometry;
    if (smoothed.didWork || didWork) this.#committedCount = this.#profile.colorCount;
    return smoothed.didWork || didWork;
  }

  background(command) {
    applyBackgroundCommand(this.#requireDocument(),command);
    if (command.action === 'begin') this.#editor = null;
    return this.state();
  }

  edit({action,command,events,settings}) {
    const document = this.#requireDocument();
    const selected = settings === undefined ? this.#profile.settings : readDocumentSettings(settings);
    if (action === 'begin') this.#editor = document.beginEditing();
    else if (action === 'leave') {document.leaveEditing();this.#editor = null;return this.state();}
    else if (action === 'restore') {
      const restored = document.restoreCommittedEditing();
      if (this.#editor) {this.#editor = restored;this.#segments = displayPixels(restored);}
      return this.state();
    }
    else {
      if (this.#editor === null) throw new Error('Open Edit segmentation before using its tools.');
      if (action === 'apply') this.#editor = document.edit(command);
      else if (action === 'gesture') {
        if (!Array.isArray(events) || !events.length) throw new Error('Submit a nonempty array of editor gesture events.');
        for (const event of events) {
          this.#editor = document.gesture(event);
        }
      }
      else if (action === 'undo') this.#editor = document.undoEdit();
      else if (action === 'redo') this.#editor = document.redoEdit();
      else if (action === 'revert') this.#editor = document.revertEdits();
      else if (action === 'finish') {
        if (selected.advanced) document.commitEditing();
        else {
          try {
            document.finishEditing({...selected,
              onProgress:this.#onProgress === undefined ? undefined : event => this.#reportProgress(event)},
              (stage,completed) => this.#completedTask(stage,completed));
          } catch (error) {
            // Commit ends input before numerical tasks. Cancellation retains
            // the editor and its committed history, with the actual Pan tool.
            this.#editor = document.editingState();
            throw error;
          }
          this.#vectorScene = document.snapshot().fitting?.geometry ?? this.#vectorScene;
        }
        this.#profile.settings = selected;
        this.#editor = null;
        return this.state();
      } else throw new Error('Choose begin, apply, gesture, undo, redo, revert or finish for editing.');
    }
    this.#segments = displayPixels(this.#editor);
    return this.state();
  }

  export({ format, ...options }) {
    const bitmap=bitmapFormats.includes(format);
    if (!bitmap && !Object.hasOwn(exporters, format)) throw new Error('Choose SVG, EPS, PDF, AI, DXF, EMF, BMP, JPEG, PNG or TIFF.');
    const document=this.#requireDocument();
    if (bitmap) {
      for (const name of Object.keys(options)) if (!['scale','flatten'].includes(name))
        throw new Error('Unknown bitmap export option: '+name);
      // The native bitmap filename field receives a suffixed basename, then
      // applies its first-dot rule, including names such as multi.part.bmp.
      const basename=(this.#file.name.replace(/\.[^.]+$/, '')+'_vectorized').split('.')[0];
      return {bytes:exportBitmap(document,{format,scale:this.#bitmapSize.scale,...options}),filename:`${basename}.${format}`};
    }
    if (Object.hasOwn(options,'scale') || Object.hasOwn(options,'flatten'))
      throw new Error('Scale and flatten apply only to bitmap export.');
    const state = document.snapshot();
    if (!state.fitting?.complete) throw new Error('Finish vectorizing the image before exporting.');
    const output = exporters[format]({ ...options, geometry: state.fitting.geometry,
      width: state.original.width, height: state.original.height, palette: state.segmentation.palette,
      hiddenRegions:this.#document.backgroundState().hiddenRegions });
    return { bytes: typeof output === 'string' ? Buffer.from(output, 'utf8') : output,
      filename: `${this.#file.name.replace(/\.[^.]+$/, '')}.${format}` };
  }

  /** The batch controller supplies queue/UI ownership; this application owner
   * supplies its existing document and random stream. The host serializes all
   * other document operations until this asynchronous job settles. */
  async runBatch(queue,{profile = defaultBatchProfile(),...options} = {}) {
    for (const name of Object.keys(options)) if (!['exportOptions','signal','decideOverwrite','onStage','onProgress'].includes(name))
      throw new Error('Unknown GUI batch option: '+name);
    if (options.onProgress !== undefined && typeof options.onProgress !== 'function')
      throw new TypeError('Batch progress observer must be a function.');
    const requested = readSettingsProfile(profile),settings = batchProfileControls(requested);
    const runner = new BatchRunner({queue,document:this.#document});
    let summary,document,loaded = false,lastStage,hadSource = this.#file !== null;
    try {
      summary = await runner.run({...options,settings,onProgress:event => {
        lastStage = event.stage;
        options.onProgress?.(event);
      },onStage:event => {
        lastStage = event.stage;
        if (event.stage === 'load') {
          loaded = true;
          this.#paletteRuntime.customized = false;
          this.#profile = structuredClone(requested);
          this.#profile.settings.background = Float32Array.of(1,1,1,1);
          if (hadSource || event.source) this.#committedCount = requested.colorCount;
          hadSource = event.source !== null;
          // The model's path advances on every attempted load, even failure.
          // Batch imageChanged events do not refresh the Quick Save widgets.
          this.#sourceFilename=resolve(queue.workingDirectory,event.filename);
          this.#file = event.source ? {name:event.filename.split(/[\\/]/).at(-1),...event.source} : null;
          this.#extension = event.source ? resolveImageFilename(event.filename).extension : null;
        } else if (['segmentation','smoothing','fitting'].includes(event.stage)) {
          this.#profile.settings = structuredClone(event.state.settings);
          if (event.paletteCount !== null) {
            this.#profile.colorCount = this.#profile.paletteColorCount = event.paletteCount;
            this.#committedCount = event.paletteCount;
            applyPaletteCommand(this.#profile,this.#paletteRuntime,{action:'replace',palette:event.state.settings.palette});
          }
        }
        options.onStage?.(event);
      }});
    } finally {
      document = runner.snapshot().document;
      if (loaded) {
        if (summary?.status === 'cancelled' && lastStage === 'load') {
          this.#profile.settings = {...this.#profile.settings,palette:new Float32Array(),
            backgroundMode:-1,background:Float32Array.of(1,1,1,0)};
          this.#profile.paletteColorCount = 2;this.#profile.recommendedBackgroundMode = -1;
          this.#resetSourceFile();
        }
        this.#editor = null;
        // Exported region records can survive a cancelled segmentation. Its
        // completed key, not allocated records, authorizes the display map.
        this.#segments = document.stageKeys.segmentation && document.segmentation?.counts.length ? displaySegments(document.segmentation) : null;
        this.#vectorScene = document.fitting?.complete ? structuredClone(document.fitting.geometry) : null;
        if (!document.original.width) {this.#file = null;this.#extension = null;}
      }
    }
    return {summary,document};
  }

  #requireDocument() {
    if (!this.#document || !this.#file) throw new Error('Open an image first.');
    return this.#document;
  }
}
