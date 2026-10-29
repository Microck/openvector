import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {join,dirname,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {AutoUpdate,defaultAutoUpdatePreferences,readAutoUpdatePreferences} from '../auto-update.mjs';
import {GuiEngine} from '../engine.mjs';
import {BatchController} from '../batch-controller.mjs';
import {readProcessingPreferences,writeProcessingPreferences} from '../processing-preferences.mjs';
import {startGui} from '../server.mjs';

const references=gunzipSync(readFileSync(new URL('../../test/fixtures/auto-preferences-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const headers={'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};

async function directoryFor(t) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-auto-preferences-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-auto-preferences-')));
    await rm(directory,{recursive:true,force:true});
  });return directory;
}

for(const reference of references)test('native Auto-update startup choices: '+reference.id,async t=>{
  const directory=await directoryFor(t),filename=join(directory,'preferences.json');
  const engine=new GuiEngine(),batch=new BatchController({engine,applicationDirectory:directory,workingDirectory:directory,
    cancellation:new Int32Array(new SharedArrayBuffer(4))});
  const automatic=new AutoUpdate(reference.checked);
  assert.deepEqual(automatic.checkedPreferences(),reference.checked);
  assert.deepEqual(automatic.snapshot(),Object.fromEntries(Object.entries(reference.checked).map(([page,checked])=>
    [page,{checked,enabled:false,pending:false,active:false}])));
  const queue=batch.snapshot().queue;
  await writeProcessingPreferences(filename,{selected:engine.selectedProfile(),batch:{profile:batch.selectedProfile(),
    exportDirectory:queue.exportDirectory,exportFormat:queue.exportFormat},outputOptions:engine.outputOptions(),autoUpdate:automatic.checkedPreferences(),skipReview:false,quickSave:engine.quickSavePreferences(),bitmapSave:engine.bitmapSavePreferences()});
  const restored=await readProcessingPreferences(filename);
  assert.deepEqual(new AutoUpdate(restored.autoUpdate).snapshot(),automatic.snapshot());
  assert.deepEqual(restored.autoUpdate,reference.checked);
  restored.autoUpdate.palette=!restored.autoUpdate.palette;
  assert.deepEqual(automatic.checkedPreferences(),reference.checked,'owned startup projection');
  const snapshot=automatic.checkedPreferences();snapshot.smoothing=!snapshot.smoothing;
  assert.deepEqual(automatic.checkedPreferences(),reference.checked,'owned returned projection');
});

test('Auto-update preferences validate the whole projection and reject older storage shapes',async t=>{
  const directory=await directoryFor(t),filename=join(directory,'preferences.json');
  assert.deepEqual(readAutoUpdatePreferences(defaultAutoUpdatePreferences),defaultAutoUpdatePreferences);
  for(const invalid of [undefined,null,[],{},true,{palette:true,segmentation:false},
    {palette:true,segmentation:false,smoothing:1},{...defaultAutoUpdatePreferences,deadline:500}])
    assert.throws(()=>readAutoUpdatePreferences(invalid),/Boolean choices/);
  await writeFile(filename,JSON.stringify({selected:{},batch:{},outputOptions:{}}));
  const before=await readFile(filename,'utf8');
  await assert.rejects(readProcessingPreferences(filename),/autoUpdate.*Choose a new preferencesFilename/);
  assert.equal(await readFile(filename,'utf8'),before);
});

test('HTTP Auto-update choices survive workspace changes, model reset and clean host restart',async t=>{
  const directory=await directoryFor(t),profileFilename=join(directory,'profiles.json');
  let host=await startGui({port:0,workingDirectory:directory,profileFilename});t.after(()=>host?.close());
  const get=async()=>{
    const response=await fetch(host.url+'/api/autoUpdatePreferences',{headers});assert.equal(response.status,200);return response.json();
  };
  const post=async(action,input,status=200)=>{
    const response=await fetch(host.url+'/api/'+action,{method:'POST',headers:{...headers,Origin:host.url,'Content-Type':'application/json'},body:JSON.stringify(input)});
    assert.equal(response.status,status);return response.json();
  };
  assert.deepEqual(await get(),defaultAutoUpdatePreferences);
  const checked={palette:false,segmentation:true,smoothing:false};
  assert.deepEqual(await post('autoUpdatePreferences',checked),checked);
  for(const invalid of [null,[],{palette:true},{...checked,smoothing:0},{...checked,active:true}]) {
    await post('autoUpdatePreferences',invalid,400);assert.deepEqual(await get(),checked,'failed mutation is atomic');
  }
  await post('batch',{command:'enter'});assert.deepEqual(await get(),checked);
  await post('autoUpdatePreferences',{palette:false,segmentation:false,smoothing:false});
  await post('resetApplicationModel',null);assert.deepEqual(await get(),{palette:false,segmentation:false,smoothing:false});
  await post('batch',{command:'leave'});
  await post('autoUpdatePreferences',checked);
  await assert.rejects(readFile(join(directory,'processing-preferences.json')),{code:'ENOENT'});
  await host.close();host=null;
  const stored=JSON.parse(await readFile(join(directory,'processing-preferences.json'),'utf8'));
  assert.deepEqual(Object.keys(stored).sort(),['autoUpdate','batch','bitmapSave','outputOptions','quickSave','selected','skipReview']);assert.deepEqual(stored.autoUpdate,checked);
  host=await startGui({port:0,workingDirectory:directory,profileFilename});assert.deepEqual(await get(),checked);
  const response=await fetch(host.url+'/api/state',{headers});assert.equal(await response.json(),null);
});
