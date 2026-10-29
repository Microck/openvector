import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { BatchQueue, readBatchConfiguration } from '../src/batch-runner.mjs';
import { applyAdvancedProfile, defaultBatchProfile, batchProfileControls } from '../src/settings-profile.mjs';
import { decodeImage } from '../src/image-input.mjs';
import { prepareOriginalImage } from '../src/image-pipeline.mjs';
import { ProcessingJob } from './processing-job.mjs';

function thumbnail(bytes) {
  const original = prepareOriginalImage(decodeImage(bytes));
  const scale = Math.min(1,96/original.width,96/original.height);
  const width = Math.max(1,Math.round(original.width*scale)),height = Math.max(1,Math.round(original.height*scale));
  const rgba = new Uint8Array(width*height*4);
  for (let y=0;y<height;y++) for (let x=0;x<width;x++) {
    const source = (Math.min(original.height-1,Math.floor(y/scale))*original.width+
      Math.min(original.width-1,Math.floor(x/scale)))*4,target = (y*width+x)*4,alpha = original.bgra[source+3];
    for (let channel=0;channel<3;channel++) rgba[target+channel] = alpha ? Math.round(original.bgra[source+2-channel]*255/alpha) : 0;
    rgba[target+3] = alpha;
  }
  return {id:createHash('sha256').update(bytes).digest('hex'),width,height,sourceWidth:original.width,sourceHeight:original.height,
    rgba:Buffer.from(rgba).toString('base64')};
}

/** Own the GUI queue and job lifecycle, not another conversion engine. The
 * host serializes commands; only cancellation/overwrite may interrupt a job. */
export class BatchController {
  #engine;
  #publish;
  #queue;
  #active = false;
  #page = 'files';
  #profile;
  #thumbnails = new Map();
  #duplicates = [];
  #job;
  #decision;
  #completion = Promise.resolve();

  constructor({engine,applicationDirectory,workingDirectory,cancellation,preferences,publish = () => {}}) {
    this.#engine = engine;this.#publish = publish;
    this.#job = new ProcessingJob({cancellation,publish:() => this.#emit()});
    this.#profile=preferences?structuredClone(preferences.profile):defaultBatchProfile();
    this.#queue = new BatchQueue({applicationDirectory,workingDirectory});
    if(preferences) {
      this.#queue.exportDirectory=preferences.exportDirectory;this.#queue.exportFormat=preferences.exportFormat;
    }
  }

  selectedProfile() {return structuredClone(this.#profile);}

  /** The application model preserves workspace/page and advanced requests.
   * Its coordinator admits the operation before any owner mutates state. */
  resetSource() {
    this.#queue.clear();this.#queue.processingIndex = 0;this.#thumbnails.clear();
    this.#profile.settings = {...this.#profile.settings,category:3,quality:3,paletteMode:3,
      backgroundMode:1,background:Float32Array.of(1,1,1,1)};
    this.#profile.basicColorCount = 0;
  }

  get running() {return this.#job.running;}
  get active() {return this.#active;}
  get completion() {return this.#completion;}
  snapshot() {
    const snapshot = structuredClone({active:this.#active,page:this.#page,queue:this.#queue.snapshot(),settings:batchProfileControls(this.#profile),
      colorCounts:{basic:this.#profile.basicColorCount,advanced:this.#profile.colorCount},exportOptions:this.#engine.outputOptions(),
      thumbnails:Array.from(this.#thumbnails,([filename,{rgba,...image}]) => ({filename,...image})),
      duplicates:this.#duplicates});
    snapshot.job = this.#job.snapshot();return snapshot;
  }
  #emit(assets = []) {this.#publish(this.snapshot(),assets);}

  async command(input) {
    if (input === null || typeof input !== 'object' || Array.isArray(input)) throw new Error('Choose a batch command.');
    const {command} = input;
    const assets = [];
    if (this.running && !['decide','cancel'].includes(command)) throw new Error('Wait for batch processing to finish.');
    switch (command) {
      case 'enter': this.#active = true;break;
      case 'leave': this.#active = false;break;
      case 'add': {
        this.#duplicates = this.#queue.addFiles(input.filenames);
        this.#emit();
        for (const {filename} of this.#queue.snapshot().queue) if (!this.#thumbnails.has(filename)) {
          try {
            const image = thumbnail(await readFile(resolve(this.#queue.workingDirectory,filename)));
            this.#thumbnails.set(filename,image);assets.push(image);
          }
          catch (error) {this.#thumbnails.set(filename,{error:error.message});}
        }
        break;
      }
      case 'select': this.#queue.select(input.index,input.selected);break;
      case 'remove': this.#queue.remove(input.index);break;
      case 'remove-selected': this.#queue.removeSelected();break;
      case 'clear': this.#queue.clear();break;
      case 'reset-source':
        if (this.#active) throw new Error('Return to single-image mode after batch processing finishes.');
        this.resetSource();
        break;
      case 'dismiss-duplicates': this.#duplicates = [];break;
      case 'configure': {
        for (const name of ['settings','exportOptions']) if (Object.hasOwn(input,name) &&
            (input[name] === null || typeof input[name] !== 'object' || Array.isArray(input[name])))
          throw new Error('Batch '+name+' must be an object.');
        const selected = {...batchProfileControls(this.#profile),...input.settings};
        if (input.settings?.advanced !== undefined && !Object.hasOwn(input.settings,'colorCount'))
          selected.colorCount = this.#profile[selected.advanced ? 'colorCount' : 'basicColorCount'];
        const {controls,options} = readBatchConfiguration(selected,{...this.#engine.outputOptions(),...input.exportOptions});
        if (input.exportDirectory !== undefined && typeof input.exportDirectory !== 'string') throw new Error('Choose an export folder.');
        if (input.exportFormat !== undefined && !['svg','eps','pdf','ai','dxf'].includes(input.exportFormat)) throw new Error('Choose SVG, EPS, PDF, AI or DXF.');
        const {colorCount,...settings} = controls;
        this.#profile.settings = {...this.#profile.settings,...settings,palette:Float32Array.from(settings.palette)};
        this.#profile[controls.advanced ? 'colorCount' : 'basicColorCount'] = colorCount;
        this.#engine.outputOptions(options);
        if (input.exportDirectory !== undefined) this.#queue.exportDirectory = input.exportDirectory;
        if (input.exportFormat !== undefined) this.#queue.exportFormat = input.exportFormat;
        break;
      }
      case 'apply-profile': {
        const profile = applyAdvancedProfile(this.#profile,input.profile,{batch:true});
        // The original loader forces the selected single-image flag too, but
        // does not commit that image's document while batch mode is active.
        this.#engine.configureProfile({settings:{advanced:true}});
        this.#profile = profile;
        break;
      }
      case 'page':
        if (!['files','settings','summary'].includes(input.page) || input.page === 'summary' && !this.#job.hasSummary)
          throw new Error('Choose an available batch page.');
        this.#page = input.page;break;
      case 'start': this.#start();break;
      case 'decide':
        if (!this.#decision || !['overwrite','skip','cancel'].includes(input.decision)) throw new Error('Choose Overwrite, Skip or Cancel for the pending files.');
        this.#job.observeCancellation();this.#decision(input.decision);this.#decision = null;break;
      case 'cancel':
        if (this.running) {this.#job.cancel();this.#decision?.('cancel');this.#decision = null;}
        break;
      default: throw new Error('Unknown batch command.');
    }
    const members = new Set(this.#queue.snapshot().queue.map(record => record.filename));
    for (const filename of this.#thumbnails.keys()) if (!members.has(filename)) this.#thumbnails.delete(filename);
    if (['remove','remove-selected','clear'].includes(command)) this.#page = 'files';
    this.#emit(assets);return this.snapshot();
  }

  #start() {
    this.#active = true;
    this.#page = 'progress';
    this.#queue.setAcceptingEdits(false);
    this.#job.begin();
    this.#completion = this.#run();
  }

  async #run() {
    let status,summary,error;
    try {
      ({summary} = await this.#engine.runBatch(this.#queue,{profile:this.#profile,exportOptions:this.#engine.outputOptions(),
        signal:this.#job.signal,decideOverwrite:outputs => {
          this.#job.observeCancellation();
          if (this.#job.signal.aborted) return 'cancel';
          const decision = new Promise(resolveDecision => {this.#decision = resolveDecision;});
          this.#job.waitForDecision(outputs);
          return decision.then(value => {this.#job.resume();return value;});
        },onProgress:event => {
          this.#job.progress(event);
        },onStage:event => {
          this.#job.progress({...event,percent:event.state.progress,status:event.state.status},true);
        }}));
      status = summary.status;
      this.#page = summary.status === 'complete' ? 'summary' : summary.status === 'empty' ? 'files' : 'settings';
    } catch (failure) {status = 'error';error = failure.message;this.#page = 'settings';}
    finally {
      this.#decision = null;this.#queue.setAcceptingEdits(true);
      this.#job.finish(status,{summary,error});
    }
    return this.snapshot();
  }
}
