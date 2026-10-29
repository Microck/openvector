import {mkdir} from 'node:fs/promises';
import {dirname,isAbsolute} from 'node:path';
import {SavedAdvancedProfiles} from '../src/saved-profiles.mjs';

/** The worker owns one shared profile list. Settings come from its current
 * selection, never from a browser-supplied settings record or storage path. */
export class ProfileController {
  #engine;
  #batch;
  #profiles;
  #filename;

  constructor({engine,batch,profiles,filename}) {
    this.#engine = engine;this.#batch = batch;
    this.#profiles = profiles;this.#filename = filename;
  }

  static async open({engine,batch,filename}) {
    if (typeof filename !== 'string' || !isAbsolute(filename)) throw new Error('Choose an absolute profile filename.');
    return new ProfileController({engine,batch,filename,profiles:await SavedAdvancedProfiles.readFile(filename)});
  }

  names() {return this.#profiles.snapshot().map(entry => entry.name);}

  resetList() {this.#profiles.reset();}

  async command(input) {
    if (input === null || typeof input !== 'object' || Array.isArray(input)) throw new Error('Choose a profile command.');
    const {command,name,newName,force = false} = input;
    if (command === 'list') return {names:this.names()};
    if (command === 'reset') {
      if (this.#batch.active || this.#batch.running) throw new Error('Return to single-image mode after batch processing finishes.');
      this.resetList();
      await this.persist();
      return {names:this.names()};
    }
    const playback = command === 'play' || command === 'fast-forward';
    const management = command === 'rename' || command === 'delete';
    if (command !== 'save' && command !== 'select' && !playback && !management)
      throw new Error('Choose list, save, select, play, fast-forward, rename, delete or reset for profiles.');
    if (this.#batch.running) throw new Error('Wait for batch processing to finish.');
    if (typeof name !== 'string') throw new Error('Profile name must be a string.');
    if (typeof force !== 'boolean') throw new Error('Profile replacement must be true or false.');
    if (playback && (!this.#engine.hasSource || this.#batch.active))
      throw new Error('Load an image in the single-image workspace before replaying a profile.');
    let document,replay,mutation;
    if (management) {
      mutation = command === 'rename' ? this.#profiles.rename(name,newName,{force}) : {index:this.#profiles.remove(name)};
      if (mutation.index < 0) throw new Error('Saved profile not found.');
      if (command === 'rename' && !mutation.changed) return {names:this.names(),...mutation};
    } else if (command === 'save') {
      if (!force && this.names().includes(name)) throw new Error('A profile with this exact name already exists. Confirm replacement.');
      // The original one-argument save slot uses the selected single-image
      // settings even while batch mode owns a separate requested profile.
      this.#profiles.put(name,this.#engine.selectedProfile());
    } else {
      const selected = this.#profiles.select(name);
      if (!selected.profile) throw new Error('Saved profile not found.');
      if (this.#batch.active) await this.#batch.command({command:'apply-profile',profile:selected.profile});
      else this.#engine.applyProfile(selected.profile);
      if (command === 'play') replay = {mode:command,page:this.#engine.startProfileWizard()};
      else if (command === 'fast-forward') {
        document = this.#engine.fastForwardProfile();
        replay = {mode:command,page:document === null ? null : 'export'};
      }
    }
    // Persist after the native in-memory operation. A failed write preserves
    // the current session and any old file, and remains an explicit error.
    await this.persist();
    return command === 'save' || management ? {names:this.names(),...mutation} : {names:this.names(),
      selection:this.#engine.selectionState(),document:document ?? this.#engine.state(),batch:this.#batch.snapshot(),
      ...(replay ? {replay} : {})};
  }

  async persist() {
    await mkdir(dirname(this.#filename),{recursive:true});
    await this.#profiles.writeFile(this.#filename);
  }
}
