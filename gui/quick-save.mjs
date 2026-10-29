import {realpathSync,statSync} from 'node:fs';
import {basename,dirname,extname,resolve,sep} from 'node:path';

export const quickSaveFormats = ['ai','dxf','emf','eps','pdf','svg'];
export const bitmapSaveFormats = ['bmp','jpg','png','tif'];
const modes = ['basic','advanced'];
const formatsFor=kind=>kind==='bitmap'?bitmapSaveFormats:quickSaveFormats;

function readChoice(input,formats) {
  if(!input || typeof input!=='object' || Array.isArray(input) || Object.keys(input).length!==3 ||
      !['format','directory','expanded'].every(key=>Object.hasOwn(input,key)) ||
      !formats.includes(input.format) || typeof input.directory!=='string' || input.directory.includes('\0') ||
      typeof input.expanded!=='boolean')throw new Error('Quick Save requires format, directory and Boolean expanded choices.');
  return {...input};
}

export function readQuickSavePreferences(input,kind='vector') {
  if(!input || typeof input!=='object' || Array.isArray(input) || Object.keys(input).length!==2 ||
      !modes.every(mode=>Object.hasOwn(input,mode)))throw new Error('Quick Save preferences require basic and advanced choices.');
  return Object.fromEntries(modes.map(mode=>[mode,readChoice(input[mode],formatsFor(kind))]));
}

export function defaultQuickSavePreferences(workingDirectory,kind='vector') {
  return Object.fromEntries(modes.map(mode=>[mode,{format:kind==='bitmap'?'png':'ai',directory:resolve(workingDirectory),expanded:false}]));
}

/** Window choices survive documents and workspace changes. A source event
 * replaces both folders and nonempty filenames. Disclosure stays separate. */
export class QuickSave {
  #workingDirectory;
  #choices;
  #formats;
  #filenames={basic:'',advanced:''};

  constructor({workingDirectory=process.cwd(),kind='vector',preferences=defaultQuickSavePreferences(workingDirectory,kind)}={}) {
    if(!['vector','bitmap'].includes(kind))throw new Error('Choose vector or bitmap save settings.');
    this.#workingDirectory=resolve(workingDirectory);
    this.#formats=formatsFor(kind);
    this.#choices=readQuickSavePreferences(preferences,kind);
    for(const choice of Object.values(this.#choices))choice.directory=this.#directory(choice.directory);
  }

  #directory(directory) {
    const selected=directory===''?this.#workingDirectory:directory;
    // QDir canonicalizes existing folders and preserves missing/inaccessible
    // folder text. A missing target is diagnosed by the eventual file writer.
    try {
      const absolute=resolve(this.#workingDirectory,selected);
      if(statSync(absolute).isDirectory())return realpathSync(absolute);
    }
    catch {} // QDir reports a nonexistent/inaccessible folder without throwing.
    return selected.replaceAll('/',sep);
  }

  preferences() {return structuredClone(this.#choices);}
  snapshot() {return Object.fromEntries(modes.map(mode=>[mode,{...this.#choices[mode],filename:this.#filenames[mode]}]));}

  target(mode) {
    if(!modes.includes(mode))throw new Error('Choose basic or advanced Quick Save settings.');
    const choice=this.#choices[mode],filename=this.#filenames[mode];
    if(filename==='')throw new Error('Enter a filename before saving.');
    return {filename:resolve(this.#workingDirectory,choice.directory,filename)+'.'+choice.format,
      format:choice.format};
  }

  configure(input) {
    if(!input || typeof input!=='object' || Array.isArray(input) || !modes.includes(input.mode) ||
        Object.keys(input).some(key=>!['mode','format','directory','expanded','filename'].includes(key)))
      throw new Error('Choose basic or advanced Quick Save settings.');
    const {mode,filename,...patch}=input,choice=readChoice({...this.#choices[mode],...patch},this.#formats);
    if(Object.hasOwn(input,'filename') && (typeof filename!=='string' || filename.includes('\0')))
      throw new Error('Enter a Quick Save filename without NUL characters.');
    if(Object.hasOwn(patch,'directory'))choice.directory=this.#directory(choice.directory);
    if(Object.hasOwn(input,'filename'))this.#setFilename(choice,mode,filename);
    this.#choices[mode]=choice;
    return this.snapshot();
  }

  sourceChanged(filename) {
    const directory=this.#directory(dirname(resolve(this.#workingDirectory,filename)));
    for(const mode of modes) {
      const choice=this.#choices[mode];choice.directory=directory;
      if(filename!=='')this.#setFilename(choice,mode,basename(filename));
    }
  }

  #setFilename(choice,mode,filename) {
    if(filename==='') {this.#filenames[mode]='';return;}
    const absolute=resolve(this.#workingDirectory,choice.directory,filename);
    let directory=filename==='..' || /[\\/]$/.test(filename);
    // A typed existing folder navigates the filename widget's directory.
    try {directory ||= statSync(absolute).isDirectory();}catch {}
    choice.directory=this.#directory(directory?absolute:dirname(absolute));
    const leaf=directory?'':basename(absolute),extension=extname(leaf).slice(1).toLowerCase();
    if(this.#formats.includes(extension))choice.format=extension;
    this.#filenames[mode]=leaf.split('.')[0];
  }
}
