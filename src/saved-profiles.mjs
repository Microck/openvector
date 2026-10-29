import {readFile,writeFile,rename,rm} from 'node:fs/promises';
import {dirname,join} from 'node:path';
import {randomUUID} from 'node:crypto';
import {defaultAdvancedProfileName,defaultAdvancedProfile,readSettingsProfile,storedSettingsProfile} from './settings-profile.mjs';

export {defaultAdvancedProfileName,defaultAdvancedProfile,applyAdvancedProfile,runAdvancedProfile} from './settings-profile.mjs';
const capacity = 200;

/** One list owns names, order and settings. Startup reconstructs the original
 * built-in entry even if the preceding session evicted or replaced its slot. */
export class SavedAdvancedProfiles {
  #entries = [];

  constructor(entries = []) {
    if (!Array.isArray(entries)) throw new Error('Saved profiles must be an array.');
    for (const entry of entries.toReversed()) this.put(entry.name,entry.profile);
    this.put(defaultAdvancedProfileName,defaultAdvancedProfile());
  }

  get size() {return this.#entries.length;}
  snapshot() {return structuredClone(this.#entries);}

  /** Factory reset recreates the built-in entry even after rename or eviction.
   * It does not select that profile or change any document controls. */
  reset() {
    this.#entries = [{name:defaultAdvancedProfileName,profile:defaultAdvancedProfile()}];
  }

  put(name,profile) {
    if (typeof name !== 'string') throw new Error('Profile name must be a string.');
    const selected = name === defaultAdvancedProfileName ? defaultAdvancedProfile() : readSettingsProfile(profile);
    const index = this.#entries.findIndex(entry => entry.name === name);
    if (index >= 0) this.#entries.splice(index,1);
    this.#entries.unshift({name,profile:selected});
    if (this.#entries.length > capacity) this.#entries.pop();
  }

  select(name) {
    if (typeof name !== 'string') throw new Error('Profile name must be a string.');
    const index = this.#entries.findIndex(entry => entry.name === name);
    if (index < 0) return {index,profile:null};
    const [entry] = this.#entries.splice(index,1);
    this.#entries.unshift(entry);
    return {index,profile:structuredClone(entry.profile)};
  }

  /** Renaming does not select or re-save a profile. In particular, replacing
   * the built-in label keeps the source settings until the next startup. */
  rename(name,newName,{force = false} = {}) {
    if (typeof name !== 'string' || typeof newName !== 'string') throw new Error('Profile names must be strings.');
    if (typeof force !== 'boolean') throw new Error('Profile replacement must be true or false.');
    const index = this.#entries.findIndex(entry => entry.name === name);
    if (index < 0 || newName === '' || newName === name) return {index,changed:false,replacedIndex:-1};
    const replacedIndex = this.#entries.findIndex(entry => entry.name === newName);
    if (replacedIndex >= 0 && !force) throw new Error('A profile with this exact name already exists. Confirm replacement.');
    this.#entries[index].name = newName;
    if (replacedIndex >= 0) this.#entries.splice(replacedIndex,1);
    return {index,changed:true,replacedIndex};
  }

  remove(name) {
    if (typeof name !== 'string') throw new Error('Profile name must be a string.');
    const index = this.#entries.findIndex(entry => entry.name === name);
    if (index >= 0) this.#entries.splice(index,1);
    return index;
  }

  static async readFile(filename) {
    let encoded;
    try {encoded = await readFile(filename,'utf8');}
    catch (error) {if (error.code === 'ENOENT') return new SavedAdvancedProfiles();throw error;}
    const stored = JSON.parse(encoded);
    if (stored === null || typeof stored !== 'object' || Array.isArray(stored) ||
        Object.keys(stored).length !== 1 || !Array.isArray(stored.profiles))
      throw new Error('Profile file must contain one profiles array.');
    return new SavedAdvancedProfiles(stored.profiles);
  }

  async writeFile(filename) {
    const profiles = this.#entries.map(({name,profile}) => ({name,profile:storedSettingsProfile(profile)}));
    const temporary = join(dirname(filename),`.openvector-profiles-${randomUUID()}.tmp`);
    try {
      await writeFile(temporary,JSON.stringify({profiles})+'\n',{flag:'wx'});
      await rename(temporary,filename);
    } finally {await rm(temporary,{force:true});}
  }
}
