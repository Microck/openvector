import {readFile,writeFile,mkdir,rename,rm} from 'node:fs/promises';
import {dirname,isAbsolute,join} from 'node:path';
import {randomUUID} from 'node:crypto';
import {readSettingsProfile,storedSettingsProfile,batchProfileControls} from '../src/settings-profile.mjs';
import {readBatchConfiguration} from '../src/batch-runner.mjs';
import {readAutoUpdatePreferences} from './auto-update.mjs';
import {readReviewPreferences} from './review-state.mjs';
import {readQuickSavePreferences} from './quick-save.mjs';

function readBitmapSavePreferences(input) {
  if(!input || typeof input!=='object' || Array.isArray(input) || Object.keys(input).length!==3 ||
      !['basic','advanced','scale'].every(key=>Object.hasOwn(input,key)) || !Number.isFinite(input.scale))
    throw new Error('Bitmap save preferences require basic, advanced and a finite numeric scale.');
  return {...readQuickSavePreferences({basic:input.basic,advanced:input.advanced},'bitmap'),scale:input.scale};
}

/** Restore the processing owners at startup. Files and palette pixels belong
 * to their session owners and never enter this preference file. */
export async function readProcessingPreferences(filename) {
  if(typeof filename!=='string' || !isAbsolute(filename))throw new Error('Choose an absolute processing-preferences filename.');
  try {
    const stored=JSON.parse(await readFile(filename,'utf8'));
    if(!stored || typeof stored!=='object' || Array.isArray(stored) ||
        Object.keys(stored).length!==7 || !['selected','batch','outputOptions','autoUpdate','skipReview','quickSave','bitmapSave'].every(key=>Object.hasOwn(stored,key)))
      throw new Error('Processing preferences must contain selected, batch, outputOptions, autoUpdate, skipReview, quickSave and bitmapSave settings.');
    const selected=readSettingsProfile(stored.selected),batch=stored.batch;
    if(!batch || typeof batch!=='object' || Array.isArray(batch) || Object.keys(batch).length!==3 ||
        !['profile','exportDirectory','exportFormat'].every(key=>Object.hasOwn(batch,key)) ||
        typeof batch.exportDirectory!=='string' || !['svg','eps','pdf','ai','dxf'].includes(batch.exportFormat))
      throw new Error('Invalid batch processing preferences.');
    const profile=readSettingsProfile(batch.profile);
    if(selected.settings.palette.length || profile.settings.palette.length)
      throw new Error('Processing preferences cannot contain session palette colors.');
    if(!stored.outputOptions || typeof stored.outputOptions!=='object' || Array.isArray(stored.outputOptions) ||
        Object.keys(stored.outputOptions).length!==4 || !['layout','thinStroke','compact','curveMode'].every(key=>Object.hasOwn(stored.outputOptions,key)))
      throw new Error('Stored outputOptions must contain all four output choices.');
    const {options:outputOptions}=readBatchConfiguration(batchProfileControls(profile),stored.outputOptions);
    const autoUpdate=readAutoUpdatePreferences(stored.autoUpdate);
    const {skipReview}=readReviewPreferences({skipReview:stored.skipReview});
    const quickSave=readQuickSavePreferences(stored.quickSave);
    const bitmapSave=readBitmapSavePreferences(stored.bitmapSave);
    return {selected,batch:{profile,exportDirectory:batch.exportDirectory,exportFormat:batch.exportFormat},outputOptions,autoUpdate,skipReview,quickSave,bitmapSave};
  } catch(error) {
    if(error.code==='ENOENT')return null;
    throw new Error('Could not read processing preferences at '+filename+': '+error.message+
      ' Choose a new preferencesFilename or remove this local file.',{cause:error});
  }
}

/** The caller supplies validated owned profiles and queue/export snapshots. */
export async function writeProcessingPreferences(filename,{selected,batch,outputOptions,autoUpdate,skipReview,quickSave,bitmapSave}) {
  const stored={selected:storedSettingsProfile(selected),batch:{...batch,profile:storedSettingsProfile(batch.profile)},outputOptions,autoUpdate,skipReview,quickSave,bitmapSave};
  await mkdir(dirname(filename),{recursive:true});
  const temporary=join(dirname(filename),`.openvector-preferences-${randomUUID()}.tmp`);
  try {
    await writeFile(temporary,JSON.stringify(stored)+'\n',{flag:'wx'});
    await rename(temporary,filename);
  } finally {await rm(temporary,{force:true});}
}
