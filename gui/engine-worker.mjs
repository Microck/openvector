import { parentPort, workerData } from 'node:worker_threads';
import { GuiEngine } from './engine.mjs';
import { BatchController } from './batch-controller.mjs';
import { ProfileController } from './profile-controller.mjs';
import { SourceController } from './source-controller.mjs';
import { ApplicationController } from './application-controller.mjs';
import {readProcessingPreferences,writeProcessingPreferences} from './processing-preferences.mjs';
import {defaultAutoUpdatePreferences,readAutoUpdatePreferences} from './auto-update.mjs';
import {readReviewPreferences} from './review-state.mjs';
import {QuickSaveFile} from './quick-save-file.mjs';
import {createNumericText} from './numeric-text.mjs';
import {readBitmapSizeText} from './bitmap-size.mjs';
import {readNumericLocale} from './native-paths.mjs';
import {ProcessingJob} from './processing-job.mjs';
import {CancelTaskException} from '../src/image-document.mjs';

const preferences=await readProcessingPreferences(workerData.preferencesFilename);
let autoUpdatePreferences={...(preferences?.autoUpdate ?? defaultAutoUpdatePreferences)};
let skipReview=preferences?.skipReview ?? false;
let numericText;
const processing = new ProcessingJob({cancellation:workerData.cancellation,
  publish:() => parentPort.postMessage({kind:'processing',value:processing.snapshot()})});
const engine = new GuiEngine({profile:preferences?.selected,outputOptions:preferences?.outputOptions,quickSave:preferences?.quickSave,bitmapSave:preferences?.bitmapSave,workingDirectory:workerData.workingDirectory,
  onProgress:event => {processing.progress(event,event.complete);return processing.signal.aborted;}});
const quickSaveFile=new QuickSaveFile(engine);
const batch = new BatchController({engine,...workerData,preferences:preferences?.batch,publish:(value,assets) => parentPort.postMessage({kind:'batch',value,assets})});
const profiles = await ProfileController.open({engine,batch,filename:workerData.profileFilename});
const sources = await SourceController.open({engine,batch,filename:workerData.sourceFilename,
  applicationDirectory:workerData.applicationDirectory,workingDirectory:workerData.workingDirectory});
const application = new ApplicationController({engine,batch,profiles,sources});
const actions = new Set(['open', 'leavePalette', 'reset', 'state', 'select', 'palette', 'palettes', 'process', 'vectors', 'automatic', 'review', 'edit', 'background', 'export']);
const numerical = new Set(['palettes','process','vectors','automatic','review']);
const processImage = async operation => {
  if (batch.running) throw new Error('Wait for batch processing to finish.');
  processing.begin();
  try {
    const value = await operation();processing.finish('complete');return value;
  } catch (error) {
    const cancelled = error instanceof CancelTaskException && processing.signal.aborted;
    processing.finish(cancelled ? 'cancelled' : 'error',{error:cancelled ? null : error.message});
    if (cancelled) error.code = 'PROCESSING_CANCELLED';
    throw error;
  }
};
parentPort.on('message', async ({ id, action, input }) => {
  try {
    if (action === 'cancelProcessing') {processing.cancel();return;}
    if(action==='saveProcessingPreferences') {
      if(batch.running)throw new Error('Processing preferences cannot be saved while a batch is running.');
      const snapshot=batch.snapshot();
      await writeProcessingPreferences(workerData.preferencesFilename,{selected:engine.selectedProfile(),outputOptions:engine.outputOptions(),autoUpdate:autoUpdatePreferences,skipReview,quickSave:engine.quickSavePreferences(),bitmapSave:engine.bitmapSavePreferences(),batch:{profile:batch.selectedProfile(),
        exportDirectory:snapshot.queue.exportDirectory,exportFormat:snapshot.queue.exportFormat}});
      parentPort.postMessage({id,value:null});return;
    }
    const readOnly=action==='state' || input===undefined && ['quickSave','bitmapSave','bitmapSize','autoUpdatePreferences','reviewPreferences'].includes(action) ||
      ['sources','profiles'].includes(action) && input?.command==='list';
    if(quickSaveFile.pending && action!=='quickSaveFile' && !readOnly)
      throw new Error('Finish the pending Quick Save decision first.');
    if(action==='bitmapSize') {
      if(input!==undefined) {
        if(batch.running || batch.active)throw new Error('Return to single-image mode after batch processing finishes.');
        if(!engine.hasSource)throw new Error('Open an image first.');
        if(input?.command==='commit') {
          numericText??=createNumericText(await readNumericLocale());
          input=readBitmapSizeText(input,numericText);
        }
      }
      parentPort.postMessage({id,value:engine.bitmapSize(input)});return;
    }
    if(action==='bitmapSave') {
      if(input!==undefined && (batch.running || batch.active))throw new Error('Return to single-image mode after batch processing finishes.');
      parentPort.postMessage({id,value:engine.bitmapSave(input)});return;
    }
    if(action==='quickSaveFile') {
      if(batch.running || batch.active)throw new Error('Return to single-image mode after batch processing finishes.');
      parentPort.postMessage({id,value:await quickSaveFile.command(input)});return;
    }
    if(action==='quickSaveDirectory') {
      if(batch.running)throw new Error('Wait for batch processing to finish.');
      if(!input || Object.keys(input).length!==2 || !['basic','advanced'].includes(input.mode) || !['vector','bitmap'].includes(input.kind))
        throw new Error('Choose a vector or bitmap save directory and basic or advanced mode.');
      const choices=input.kind==='bitmap'?engine.bitmapSave():engine.quickSave();
      parentPort.postMessage({id,value:choices[input.mode].directory});return;
    }
    if(action==='autoUpdatePreferences') {
      if(input!==undefined) {
        if(batch.running)throw new Error('Wait for batch processing to finish.');
        autoUpdatePreferences=readAutoUpdatePreferences(input);
      }
      parentPort.postMessage({id,value:{...autoUpdatePreferences}});return;
    }
    if(action==='reviewPreferences') {
      if(input!==undefined) {
        if(batch.running)throw new Error('Wait for batch processing to finish.');
        skipReview=readReviewPreferences(input).skipReview;
      }
      parentPort.postMessage({id,value:{skipReview}});return;
    }
    if(action==='quickSave') {
      if(input!==undefined && batch.running)throw new Error('Wait for batch processing to finish.');
      parentPort.postMessage({id,value:engine.quickSave(input)});return;
    }
    if(action==='outputOptions') {
      if(batch.running)throw new Error('Wait for batch processing to finish.');
      const options=engine.outputOptions(input);
      parentPort.postMessage({kind:'batch',value:batch.snapshot(),assets:[]});
      parentPort.postMessage({id,value:options});return;
    }
    if (action === 'resetApplicationModel') {
      if (input!==null) throw new Error('Reset the application model with null input.');
      let value;
      try {value=await application.resetModel();}
      finally {parentPort.postMessage({kind:'batch',value:batch.snapshot(),assets:[]});}
      parentPort.postMessage({id,value});return;
    }
    if (action === 'sources') {
      parentPort.postMessage({id,value:await sources.command(input)});return;
    }
    if (action === 'profiles') {
      const value = input?.command === 'fast-forward' ? await processImage(() => profiles.command(input)) : await profiles.command(input);
      parentPort.postMessage({id,value});return;
    }
    if (action === 'batch') {
      const value = await batch.command(input);
      if (id !== null) parentPort.postMessage({id,value});
      return;
    }
    if (batch.running || batch.active && action !== 'state') throw new Error('Return to single-image mode after batch processing finishes.');
    if (!actions.has(action)) throw new Error('Unknown GUI operation.');
    const processingImage = numerical.has(action) || action === 'edit' && input?.action === 'finish';
    parentPort.postMessage({ id, value: processingImage ? await processImage(() => engine[action](input)) : engine[action](input) });
  } catch (error) {
    // Loading can succeed before saving recent-file history fails. Return the
    // newly loaded bitmap so the client can show the actual current session.
    parentPort.postMessage({ id, error: error.message, code:error.code, state: engine.state(action==='sources'),sourceResult:error.sourceResult,applicationResult:error.applicationResult });
  }
});
parentPort.postMessage({kind:'batch',value:batch.snapshot(),assets:[]});
parentPort.postMessage({kind:'processing',value:processing.snapshot()});
