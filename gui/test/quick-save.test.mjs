import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,mkdir,writeFile,readFile,rm} from 'node:fs/promises';
import {dirname,join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {QuickSave,defaultQuickSavePreferences,quickSaveFormats} from '../quick-save.mjs';
import {readProcessingPreferences} from '../processing-preferences.mjs';
import {readEmfDevice} from '../native-paths.mjs';
import {startGui} from '../server.mjs';

const references=gunzipSync(readFileSync(new URL('../../test/fixtures/window-export-preferences-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const inputs=JSON.parse(readFileSync(new URL('../../test/fixtures/review-actions-inputs.json',import.meta.url),'utf8'));
const root=resolve(dirname(Object.keys(inputs)[0]),'../../..');
const fieldsReferences=gunzipSync(readFileSync(new URL('../../test/fixtures/quick-save-fields-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const nameInputs=JSON.parse(readFileSync(new URL('../../test/fixtures/quick-save-name-inputs.json',import.meta.url),'utf8'));
const withFilenames=preferences=>Object.fromEntries(Object.entries(preferences).map(([mode,choice])=>[mode,{...choice,filename:''}]));
const withoutFilenames=snapshot=>Object.fromEntries(Object.entries(snapshot).map(([mode,{filename,...choice}])=>[mode,choice]));

// Native registry input is translated into the canonical validated JSON shape.
// This translation is only fixture setup; the product accepts no Qt keys.
function preferencesFor(reference,workingDirectory) {
  const preferences=defaultQuickSavePreferences(workingDirectory);
  for(const [mode,owner] of [['basic','classic_quick_save_widget'],['advanced','adv_quick_save_widget']]) {
    const seeded=reference.seed[owner]??{},format=seeded.extension?.toLowerCase();
    if(quickSaveFormats.includes(format))preferences[mode].format=format;
    preferences[mode].directory=seeded.directory??'';
    preferences[mode].expanded=reference.seed['VmQt::MainModel::ToggleGroupBox:'+owner]?.expanded==='true';
  }
  return preferences;
}

for(const reference of [...references,...fieldsReferences])test('native Quick Save projection: '+reference.id,()=>{
  // Reference paths describe the original capture context, not an image read.
  const workingDirectory=reference.workingDirectory??root;
  const engine=new GuiEngine({workingDirectory,quickSave:preferencesFor(reference,workingDirectory)});
  for(const [index,[action,...values]] of reference.phases[0].entries()) {
    if(action==='open-wide')engine.open({name:values[0],bytes:Buffer.from((nameInputs[values[0]]??inputs[values[0]]),'base64')});
    else if(action==='reset-application-model')engine.resetProcessing();
    else assert.equal(action,'snapshot');
    const widgets=reference.expected[0][index].windowResetState;
    const expected=Object.fromEntries([['basic',0x1b4],['advanced',0x1c4]].map(([mode,offset])=>{
      const widget=widgets.find(widget=>widget.fieldOffset===offset);
      return [mode,{format:widget.formats[widget.formatIndex],directory:widget.directory,expanded:widget.expanded}];
    }));
    const fields=reference.expected[0][index].quickSaveFields;
    if(fields)for(const [mode,offset] of [['basic',0x1b4],['advanced',0x1c4]])expected[mode].filename=fields.find(field=>field.fieldOffset===offset).filename;
    assert.deepEqual(fields?engine.quickSave():engine.quickSavePreferences(),expected,reference.id+' action '+index);
  }
});

async function directoryFor(t) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-quick-save-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-quick-save-')));
    await rm(directory,{recursive:true,force:true});
  });return directory;
}

test('Quick Save mutations and failed image loads preserve owned state atomically',async t=>{
  const directory=await directoryFor(t),owner=new QuickSave({workingDirectory:directory});
  owner.configure({mode:'basic',format:'svg',expanded:true});const before=owner.snapshot(),preferences=owner.preferences();
  for(const invalid of [null,[],{},true,{mode:'unknown'},{mode:'basic',format:'unknown'},
      {mode:'basic',format:'SVG'},{mode:'basic',directory:null},{mode:'basic',directory:'\0'},
      {mode:'basic',expanded:1},{mode:'advanced',format:'pdf',extra:1},{mode:'basic',filename:12},{mode:'basic',filename:'bad\0'}]) {
    assert.throws(()=>owner.configure(invalid));assert.deepEqual(owner.snapshot(),before);
  }
  const copy=owner.snapshot();copy.basic.format='eps';assert.deepEqual(owner.snapshot(),before);
  await writeFile(join(directory,'not-folder'),'keep');
  assert.equal(owner.configure({mode:'basic',directory:'not-folder'}).basic.directory,'not-folder');
  assert.equal(owner.configure({mode:'basic',directory:'missing-folder'}).basic.directory,'missing-folder');
  await mkdir(join(directory,'missing-folder'));
  assert.equal(owner.configure({mode:'basic',format:'pdf'}).basic.directory,'missing-folder');
  assert.equal(owner.configure({mode:'basic',directory:''}).basic.directory,directory);
  const engine=new GuiEngine({workingDirectory:directory,quickSave:preferences});
  assert.throws(()=>engine.open({name:'broken.bmp',bytes:Buffer.from('broken')}));
  assert.throws(()=>engine.open({name:'bad\0.bmp',bytes:Buffer.from(Object.values(inputs)[0],'base64')}));
  assert.deepEqual(engine.quickSave(),before);assert.equal(engine.state(),null);
});

const headers={'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
async function post(host,action,input,status=200) {
  const response=await fetch(host.url+'/api/'+action,{method:'POST',headers:{...headers,Origin:host.url,'Content-Type':'application/json'},body:JSON.stringify(input)});
  assert.equal(response.status,status);return response.json();
}
async function get(host,action) {
  const response=await fetch(host.url+'/api/'+action,{headers});assert.equal(response.status,200);return response.json();
}

test('HTTP Quick Save choices survive modes, model reset and clean restart without old-shape migration',async t=>{
  const directory=await directoryFor(t),profileFilename=join(directory,'profiles.json'),preferencesFilename=join(directory,'processing-preferences.json');
  let host=await startGui({port:0,workingDirectory:directory,profileFilename});t.after(()=>host?.close());
  assert.deepEqual(await get(host,'quickSave'),withFilenames(defaultQuickSavePreferences(directory)));
  const folder=join(directory,'images');await mkdir(folder);
  const filename=join(folder,'image.bmp');await writeFile(filename,Buffer.from(Object.values(inputs)[0],'base64'));
  await post(host,'quickSave',{mode:'basic',format:'svg',directory:folder,expanded:true});
  await post(host,'quickSave',{mode:'advanced',format:'emf',directory:directory});
  const before=await get(host,'quickSave');
  await post(host,'quickSave',{mode:'basic',format:'invalid'},400);assert.deepEqual(await get(host,'quickSave'),before);
  await post(host,'sources',{command:'open',filename});
  const loaded=await get(host,'quickSave');
  assert.equal(loaded.basic.directory,folder);assert.equal(loaded.advanced.directory,folder);
  assert.equal(loaded.basic.format,'svg');assert.equal(loaded.advanced.format,'emf');
  await post(host,'batch',{command:'enter'});assert.deepEqual(await get(host,'quickSave'),loaded);
  await post(host,'resetApplicationModel',null);
  const reset=await get(host,'quickSave');
  assert.deepEqual(reset,{basic:{...loaded.basic,directory:dirname(directory)},advanced:{...loaded.advanced,directory:dirname(directory)}});
  await post(host,'batch',{command:'leave'});
  await host.close();host=null;
  const stored=JSON.parse(await readFile(preferencesFilename,'utf8'));
  assert.deepEqual(Object.keys(stored).sort(),['autoUpdate','batch','bitmapSave','outputOptions','quickSave','selected','skipReview']);
  assert.deepEqual(stored.quickSave,withoutFilenames(reset));
  host=await startGui({port:0,workingDirectory:directory,profileFilename});assert.deepEqual(await get(host,'quickSave'),withFilenames(stored.quickSave));
  assert.equal(await get(host,'state'),null);await host.close();host=null;
  delete stored.quickSave;await writeFile(preferencesFilename,JSON.stringify(stored));
  await assert.rejects(readProcessingPreferences(preferencesFilename),/quickSave.*Choose a new preferencesFilename/);
});

test('HTTP exports all six vector formats and measures the real host device for EMF',async t=>{
  const directory=await directoryFor(t),host=await startGui({port:0,workingDirectory:directory,profileFilename:join(directory,'profiles.json')});
  t.after(()=>host.close());
  const encoded=Object.values(inputs)[0],engine=new GuiEngine({workingDirectory:directory});
  const opened=engine.open({name:'image.bmp',bytes:Buffer.from(encoded,'base64')});engine.automatic({settings:opened.settings});
  await post(host,'open',{name:'image.bmp',bytes:encoded});await post(host,'automatic',{settings:opened.settings});
  const referenceDevice=await readEmfDevice(),options={layout:'stacked',thinStroke:true,compact:true,curveMode:'sparse-lines'};
  for(const format of quickSaveFormats) {
    const response=await fetch(host.url+'/api/export?'+new URLSearchParams({format,...options}),{headers});
    assert.equal(response.status,200,format);assert.match(response.headers.get('Content-Disposition'),new RegExp('image\\.'+format+'$'));
    const bytes=Buffer.from(await response.arrayBuffer());
    assert.deepEqual(bytes,Buffer.from(engine.export({format,...options,referenceDevice}).bytes),format);
    if(format==='emf') {
      assert.deepEqual([bytes.readInt32LE(72),bytes.readInt32LE(76)],referenceDevice.pixels);
      assert.deepEqual([bytes.readInt32LE(100),bytes.readInt32LE(104)],referenceDevice.micrometers);
    }
  }
});
