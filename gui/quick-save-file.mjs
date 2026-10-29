import {mkdir,stat,writeFile} from 'node:fs/promises';
import {dirname} from 'node:path';

/** A pending decision owns its rendered bytes and path. The worker prevents
 * other mutations between decisions, so a later image cannot change this save. */
export class QuickSaveFile {
  #engine;
  #pending=null;

  constructor(engine) {this.#engine=engine;}
  get pending() {return this.#pending!==null;}

  async command(input) {
    if(!input || typeof input!=='object' || Array.isArray(input))throw new Error('Choose a Quick Save command.');
    if(input.command==='save') {
      if(this.pending)throw new Error('Finish the pending Quick Save decision first.');
      if(Object.keys(input).some(key=>!['command','kind','mode','referenceDevice'].includes(key)) || !['vector','bitmap'].includes(input.kind))
        throw new Error('Choose vector or bitmap Quick Save settings.');
      if(input.kind==='bitmap' && Object.hasOwn(input,'referenceDevice'))throw new Error('Bitmap saving does not use display metrics.');
      const target=this.#engine.quickSaveTarget(input.mode,input.kind);
      const options=input.kind==='vector'?{...this.#engine.outputOptions(),referenceDevice:input.referenceDevice}:{};
      let rendered,warning=false;
      try {rendered=this.#engine.export({format:target.format,...options});}
      catch(error) {
        if(error.code!=='BITMAP_FLATTEN_REQUIRED')throw error;
        // Freeze the same independent output before any decision. The warning
        // stays pending until the common directory/overwrite checks finish.
        rendered=this.#engine.export({format:target.format,flatten:true});warning=true;
      }
      this.#pending={...target,bytes:rendered.bytes,warning,stage:null};
    } else if(input.command==='decide') {
      if(Object.keys(input).length!==2 || typeof input.accepted!=='boolean')throw new Error('Choose Yes or No for the pending save.');
      if(!this.#pending?.stage)throw new Error('There is no pending Quick Save decision.');
      if(this.#pending.stage==='flatten')throw new Error('Acknowledge the transparency warning with OK.');
      if(!input.accepted) {this.#pending=null;return {status:'cancelled'};}
    } else if(input.command==='acknowledge') {
      if(Object.keys(input).length!==1 || this.#pending?.stage!=='flatten')throw new Error('There is no transparency warning to acknowledge.');
    } else throw new Error('Unknown Quick Save command.');

    try {
      const pending=this.#pending,directory=dirname(pending.filename);
      if(pending.stage===null || pending.stage==='directory') {
        if(pending.stage==='directory')await mkdir(directory,{recursive:true});
        else {
          const parent=await this.#stat(directory);
          if(!parent) {pending.stage='directory';return this.#confirmation();}
          if(!parent.isDirectory())throw new Error('The save directory is a file: '+directory);
        }
        if(pending.filename===pending.sourceFilename)throw new Error('Cannot overwrite the source file.');
        if(await this.#stat(pending.filename)) {pending.stage='overwrite';return this.#confirmation();}
      }
      if(pending.stage!=='flatten' && pending.warning) {pending.stage='flatten';return this.#confirmation();}
      await writeFile(pending.filename,pending.bytes);
      const saved={status:'saved',filename:pending.filename,bytes:pending.bytes.length};
      this.#pending=null;return saved;
    } catch(error) {this.#pending=null;throw error;}
  }

  #confirmation() {return {status:'confirm',stage:this.#pending.stage,filename:this.#pending.filename};}

  async #stat(filename) {
    try {return await stat(filename);}
    catch(error) {if(error.code==='ENOENT')return null;throw error;}
  }
}
