import {readFile,writeFile,rename,rm,mkdir} from 'node:fs/promises';
import {dirname,isAbsolute,join,resolve} from 'node:path';
import {randomUUID} from 'node:crypto';
import {SourceHistory,SourceDirectories} from '../src/source-history.mjs';

const directoryName=filename=>dirname(filename).replaceAll('\\','/');

/** Own real disk sources and cached history beside the existing image engine.
 * Restoring history never touches its source files, including missing files. */
export class SourceController {
  #engine;
  #batch;
  #filename;
  #history;
  #icons;
  #directory;
  #workingDirectory;
  #directories;

  constructor({engine,batch,filename,workingDirectory,directory,history,icons,directories}) {
    this.#engine=engine;this.#batch=batch;this.#filename=filename;
    this.#workingDirectory=workingDirectory;this.#directory=directory;this.#history=history;this.#icons=icons;this.#directories=directories;
  }

  static async open({engine,batch,filename,applicationDirectory,workingDirectory}) {
    if (typeof filename!=='string' || !isAbsolute(filename)) throw new Error('Choose an absolute source-history filename.');
    let stored;
    try {stored=JSON.parse(await readFile(filename,'utf8'));}
    catch(error) {
      if (error.code!=='ENOENT') throw new Error('Could not read source history at '+filename+': '+error.message,{cause:error});
      stored={directory:resolve(applicationDirectory,'samples').replaceAll('\\','/'),directories:[],files:[]};
    }
    if (typeof stored?.directory!=='string' || !isAbsolute(stored.directory)) throw new Error('Invalid source-history directory.');
    if(!Array.isArray(stored.directories))throw new Error('Invalid source-history format. Choose a new sourceFilename or remove the old local file at '+filename+'.');
    const directories=SourceDirectories.fromSnapshot({capacity:4,directories:stored.directories});
    if(stored.directories.some(directory=>!isAbsolute(directory) || directory.includes('\0')))
      throw new Error('Recent directories require absolute paths.');
    const history=SourceHistory.fromSnapshot({capacity:3,files:stored.files}),icons=new Map();
    for (const entry of stored.files) {
      if (!isAbsolute(entry.filename) || entry.filename.includes('\0')) throw new Error('Recent images require absolute filenames.');
      const [width,height]=entry.thumbnailDimensions;
      if (width===0) {
        if (entry.rgba!==null) throw new Error('An empty recent-file thumbnail must be null.');
      } else {
        if (typeof entry.rgba!=='string') throw new Error('Invalid recent-file thumbnail bytes.');
        const rgba=Buffer.from(entry.rgba,'base64');
        if (rgba.length!==width*height*4 || rgba.toString('base64')!==entry.rgba) throw new Error('Invalid recent-file thumbnail bytes.');
      }
      icons.set(entry.filename,entry.rgba);
    }
    return new SourceController({engine,batch,filename,workingDirectory,directory:stored.directory,history,icons,directories});
  }

  snapshot() {
    const {capacity,files}=this.#history.snapshot();
    return {directory:this.#directory,directoryHistory:this.#directories.snapshot(),capacity,files:files.map(entry=>({...entry,thumbnail:entry.thumbnailDimensions[0]===0 ? null :
      {width:entry.thumbnailDimensions[0],height:entry.thumbnailDimensions[1],rgba:this.#icons.get(entry.filename)}}))};
  }

  resetHistory() {this.#history.clear();this.#icons.clear();this.#directories.clear();}

  async persist() {
    const files=this.#history.snapshot().files.map(entry=>({...entry,rgba:this.#icons.get(entry.filename)}));
    await mkdir(dirname(this.#filename),{recursive:true});
    const temporary=join(dirname(this.#filename),`.openvector-sources-${randomUUID()}.tmp`);
    try {
      await writeFile(temporary,JSON.stringify({directory:this.#directory,directories:this.#directories.snapshot().directories,files})+'\n',{flag:'wx'});
      await rename(temporary,this.#filename);
    } finally {await rm(temporary,{force:true});}
  }

  async command(input) {
    if (input===null || typeof input!=='object' || Array.isArray(input)) throw new Error('Choose a source command.');
    const {command}=input;
    if (command==='list') return this.snapshot();
    if (this.#batch.active || this.#batch.running) throw new Error('Return to single-image mode after batch processing finishes.');
    let document;
    if (command==='clear') {this.#history.clear();this.#icons.clear();}
    else if (command==='reset-history') this.resetHistory();
    else if (command==='open' || command==='reopen') {
      let filename=input.filename;
      if (command==='reopen') {
        const files=this.#history.snapshot().files;
        if (!Number.isInteger(input.index) || input.index<0 || input.index>=files.length) throw new Error('Recent image not found.');
        filename=files[input.index].filename;
      }
      if (typeof filename!=='string' || filename.length===0) throw new Error('Choose an image filename.');
      filename=resolve(this.#workingDirectory,filename);
      const bytes=await readFile(filename);
      document=this.#engine.open({name:filename,bytes,settings:input.settings,count:input.count});
      const {sourceWidth:width,sourceHeight:height,thumbnail}=document.file;
      const next=this.#history.add({filename,width,height,bytes:bytes.length});
      this.#icons.set(filename,thumbnail?.rgba??null);
      const retained=new Set(next.files.map(entry=>entry.filename));
      for(const filename of this.#icons.keys())if(!retained.has(filename))this.#icons.delete(filename);
      this.#directory=directoryName(filename);
      this.#directories.add(this.#directory);
    } else throw new Error('Choose list, open, reopen, clear or reset-history for image sources.');
    // A successful load survives a failed history write. Tell the client
    // explicitly whether to adopt a new source or retain its current wizard.
    try {await this.persist();}
    catch(error) {error.sourceResult={history:this.snapshot(),loaded:document!==undefined};throw error;}
    return {history:this.snapshot(),...(document ? {document} : {})};
  }
}
