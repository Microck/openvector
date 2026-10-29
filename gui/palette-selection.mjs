import {applyPaletteCommand,readPaletteRuntime} from '../src/profile-palette.mjs';
import {readSettingsProfile} from '../src/settings-profile.mjs';

/** Selected GUI colors and their revert copy have a separate lifetime from
 * document candidates. The requested count changes neither array until an
 * update completes; manual edits change colors without scheduling analysis. */
export class PaletteSelection {
  #profile = {settings:{palette:new Float32Array()},colorCount:2,paletteColorCount:0};
  #runtime = {backup:new Float32Array(),customized:false};

  constructor({count = 2,palette = [],paletteColorCount = 0,runtime} = {}) {
    this.setCount(count);this.replace(palette);
    if (!Number.isInteger(paletteColorCount) || paletteColorCount < -2147483648 || paletteColorCount > 2147483647)
      throw new Error('Profile paletteColorCount must be a signed int32 integer.');
    this.#profile.paletteColorCount = paletteColorCount;
    if (runtime !== undefined) this.#runtime = readPaletteRuntime(runtime);
  }

  snapshot() {return {count:this.#profile.colorCount,palette:Array.from(this.#profile.settings.palette),
    paletteColorCount:this.#profile.paletteColorCount,customized:this.#runtime.customized};}
  runtime() {return {backup:Array.from(this.#runtime.backup),customized:this.#runtime.customized};}

  setCount(count) {return applyPaletteCommand(this.#profile,this.#runtime,{action:'count',count});}

  #command(command) {applyPaletteCommand(this.#profile,this.#runtime,command);return this.snapshot();}
  replace(palette) {return this.#command({action:'replace',palette});}
  choose(palette) {return this.#command({action:'choose',palette});}
  color(index,color) {return this.#command({action:'color',index,color});}
  add() {return this.#command({action:'add'});}
  delete(index) {return this.#command({action:'delete',index});}
  revert() {return this.#command({action:'revert'});}
  sourceLoaded() {return this.#command({action:'source'});}

  applyProfile(input) {
    const profile = readSettingsProfile(input);
    this.#profile = {settings:{palette:profile.settings.palette},colorCount:profile.colorCount,paletteColorCount:profile.paletteColorCount};
    return this.snapshot();
  }
}
