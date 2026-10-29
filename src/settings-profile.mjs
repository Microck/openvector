import {defaultDocumentSettings,readDocumentSettings,advancedBatchControls} from './document-controls.mjs';
import {applyPaletteCommand,readPaletteRuntime} from './profile-palette.mjs';
import {CancelTaskException} from './task-cancellation.mjs';

export const defaultAdvancedProfileName = 'Default Advanced Settings';
const metadataFields = ['colorCount','basicColorCount','paletteColorCount','recommendedBackgroundMode'];

/** A profile owns both document controls and the GUI's distinct counters.
 * Palette length does not determine any of these bookkeeping values. */
export function readSettingsProfile(profile) {
  if (profile === null || typeof profile !== 'object' || Array.isArray(profile)) throw new Error('Expected a saved settings profile.');
  const settings = readDocumentSettings(profile.settings);
  const selected = {};
  for (const name of metadataFields) {
    const value = profile[name];
    if (!Number.isInteger(value) || value < -2147483648 || value > 2147483647)
      throw new Error(`Profile ${name} must be a signed int32 integer.`);
    selected[name] = value;
  }
  return {settings,...selected};
}

/** Native preference files keep scalar controls and QColor RGB, not palette
 * colors. QColor::rgba takes each rounded 16-bit channel's high byte; its
 * QRgb constructor restores opaque RGB even when the input had alpha. */
export function storedSettingsProfile(profile) {
  const {settings,...metadata}=profile;
  const background=Array.from(settings.background,(value,index)=>
    index===3 ? 1 : (Math.round(value*65535)>>>8)/255);
  return {...metadata,settings:{...settings,background,palette:[]}};
}

export function defaultAdvancedProfile() {
  return {settings:{...defaultDocumentSettings(),advanced:true},colorCount:2,basicColorCount:0,
    paletteColorCount:2,recommendedBackgroundMode:-1};
}

export function defaultSelectedProfile() {
  return {settings:{...defaultDocumentSettings(),background:Float32Array.of(0,0,0,1)},
    colorCount:2,basicColorCount:0,paletteColorCount:0,recommendedBackgroundMode:0};
}

export function defaultBatchProfile() {
  const profile = defaultSelectedProfile();
  profile.settings = {...profile.settings,category:3,quality:3,paletteMode:3,backgroundMode:1};
  profile.paletteColorCount = 3;
  return profile;
}

/** Processing controls are a projection of one requested profile. GUI counts
 * remain distinct even when their corresponding mode is disabled. */
export function batchProfileControls(profile) {
  const {advanced,category,quality,paletteMode,backgroundMode,palette} = profile.settings;
  return {advanced,category,quality,paletteMode,backgroundMode,palette:Array.from(palette),
    colorCount:advanced ? profile.colorCount : profile.basicColorCount,
    ...Object.fromEntries(advancedBatchControls.map(name => [name,profile.settings[name]]))};
}

/** Apply the native settings copy without running a document task. The owner
 * commits the stored controls before forcing the selected advanced flag. */
export function applyAdvancedProfile(current,selected,{batch = false} = {}) {
  const source = readSettingsProfile(selected);
  if (!batch) return {...source,settings:{...source.settings,advanced:true}};
  const target = readSettingsProfile(current);
  for (const name of advancedBatchControls) target.settings[name] = source.settings[name];
  target.settings.advanced = true;
  target.settings.paletteMode = source.settings.paletteMode;
  target.settings.palette = source.settings.palette;
  target.colorCount = source.colorCount;
  target.paletteColorCount = source.paletteColorCount;
  return target;
}

/** Execute the model's Fast Forward after its owner commits the stored
 * settings and selects advanced mode. MRU persistence belongs to that owner.
 * Return owned selection state so the CLI and GUI share one replay policy. */
export function runAdvancedProfile(document,input,{paletteRuntime = {backup:[],customized:false},onStage,onProgress} = {}) {
  if (onStage !== undefined && typeof onStage !== 'function') throw new Error('Profile stage observer must be a function.');
  if (onProgress !== undefined && typeof onProgress !== 'function') throw new Error('Profile progress observer must be a function.');
  const profile = applyAdvancedProfile(null,input),runtime = readPaletteRuntime(paletteRuntime);
  const {settings,colorCount,paletteColorCount} = profile;
  if (settings.paletteMode === 3) return null;
  if (colorCount < 2 || colorCount > 24)
    throw new Error('Number of colors must be an integer from 2 through 24.');
  const didWork = {},paletteRequested = settings.paletteMode !== 2;
  const publish = (stage,completed) => onStage?.(stage,{...completed,
    profile:structuredClone(profile),paletteRuntime:structuredClone(runtime)});
  if (paletteRequested) {
    const lower = settings.paletteMode === 0 ? 2 : paletteColorCount || 2;
    const upper = settings.paletteMode === 0 ? 2 : paletteColorCount || 12;
    const paletteTask = document.runTask('palette',{...settings,lower,upper,
      onProgress:onProgress === undefined ? undefined : event => onProgress({stage:'palette',...event})}),state = paletteTask.state;
    didWork.palette = paletteTask.didWork;
    let chosen = lower;
    for (let count=lower+1;count<=upper;count++) if (state.scores[count] < state.scores[chosen]) chosen = count;
    const argb = state.palettes[chosen],palette = new Float32Array(argb.length);
    for (let offset=0;offset<argb.length;offset+=4) for (let channel=0;channel<4;channel++)
      palette[offset+channel] = argb[offset+3-channel];
    applyPaletteCommand(profile,runtime,{action:'choose',palette});
    publish('palette',paletteTask);
  }
  for (const stage of ['segmentation','smoothing','fitting']) {
    const completed = document.runTask(stage,{...profile.settings,
      onProgress:onProgress === undefined ? undefined : event => onProgress({stage,...event})});
    if (completed.didWork && completed.state.status === 2) throw new CancelTaskException(`Profile ${stage} was cancelled.`);
    didWork[stage] = completed.didWork;
    publish(stage,completed);
  }
  // Observers own their task snapshots. The final result must remain intact
  // even when an observer edits the last stage's pixels or parameter arrays.
  return {profile,paletteRuntime:runtime,state:document.snapshot(),paletteRequested,didWork};
}
