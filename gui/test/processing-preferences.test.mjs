import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,mkdir,writeFile,readFile,rm,readdir} from 'node:fs/promises';
import {dirname,join,resolve,win32} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {BatchController} from '../batch-controller.mjs';
import {ProfileController} from '../profile-controller.mjs';
import {SourceController} from '../source-controller.mjs';
import {ApplicationController} from '../application-controller.mjs';
import {readProcessingPreferences,writeProcessingPreferences} from '../processing-preferences.mjs';
import {startGui} from '../server.mjs';
import {quantizeQColorChannels} from '../../src/pixel-input.mjs';
import {defaultAutoUpdatePreferences} from '../auto-update.mjs';
import {profileFromFrame,compareDocumentState} from '../../test/helpers/native-gui.mjs';

const references=gunzipSync(readFileSync(new URL('../../test/fixtures/preferences-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const inputs=JSON.parse(readFileSync(new URL('../../test/fixtures/preferences-inputs.json',import.meta.url),'utf8'));
const parameters=JSON.parse(readFileSync(new URL('../../test/fixtures/processing-reset-parameters.json',import.meta.url),'utf8'));
const slots={'setImageCategory(int)':'category','setImageQuality(int)':'quality','setUseAdvancedMode(bool)':'advanced',
  'setPaletteMode(int)':'paletteMode','setAdvColorSensitivity(int)':'sensitivity','setAdvUsePaletteAa(bool)':'antiAlias',
  'setAdvSegmentationComplexity(int)':'complexity','setAdvMinNumPixels(int)':'minimumPixels',
  'setAdvAntiAliasingRejection(int)':'artifactRejection','setAdvClusterColors(bool)':'clusterColors',
  'setAdvContourSmoothness(int)':'smoothness','setAdvDetectSharpCorners(bool)':'detectCorners',
  'setAdvCurveComplexity(int)':'curveComplexity','setAdvUseContourAa(bool)':'smoothingAntiAlias','setFlattenImageMode(int)':'backgroundMode'};

async function directoryFor(t) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-preferences-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-preferences-')));
    await rm(directory,{recursive:true,force:true});
  });return directory;
}

async function owners(directory) {
  const preferencesFilename=join(directory,'processing-preferences.json'),preferences=await readProcessingPreferences(preferencesFilename);
  const engine=new GuiEngine({profile:preferences?.selected,outputOptions:preferences?.outputOptions,quickSave:preferences?.quickSave,bitmapSave:preferences?.bitmapSave,workingDirectory:directory}),batch=new BatchController({engine,applicationDirectory:directory,
    workingDirectory:directory,cancellation:new Int32Array(new SharedArrayBuffer(4)),preferences:preferences?.batch});
  const profiles=await ProfileController.open({engine,batch,filename:join(directory,'profiles.json')});
  const sources=await SourceController.open({engine,batch,filename:join(directory,'sources.json'),applicationDirectory:directory,workingDirectory:directory});
  return {engine,batch,profiles,sources,application:new ApplicationController({engine,batch,profiles,sources}),preferencesFilename};
}

const preferenceSnapshot=({engine,batch})=>{
  const snapshot=batch.snapshot();return {selected:engine.selectedProfile(),outputOptions:engine.outputOptions(),autoUpdate:{...defaultAutoUpdatePreferences},skipReview:false,quickSave:engine.quickSavePreferences(),bitmapSave:engine.bitmapSavePreferences(),batch:{profile:batch.selectedProfile(),
    exportDirectory:snapshot.queue.exportDirectory,exportFormat:snapshot.queue.exportFormat}};
};

for(const reference of references)test('native preference save/restart: '+reference.id,async t=>{
  const directory=await directoryFor(t),local=new Map();
  for(const [filename,encoded] of Object.entries(inputs)) {
    const target=join(directory,win32.basename(win32.dirname(filename)),win32.basename(filename));
    await mkdir(dirname(target),{recursive:true});await writeFile(target,Buffer.from(encoded,'base64'));local.set(filename,target);
  }
  for(const [phase,actions] of reference.phases.entries()) {
    const context=await owners(directory),{engine,batch,profiles,sources,application,preferencesFilename}=context;
    assert.equal(engine.state(),null);assert.equal(batch.active,false);assert.deepEqual(batch.snapshot().queue.queue,[]);
    for(const [index,[kind,...values]] of actions.entries()) {
      if(kind==='open-wide' || kind==='automatic') {
        await sources.command({command:'open',filename:local.get(values[0])});
        if(kind==='automatic')engine.automatic({settings:engine.selectedProfile().settings});
      } else if(kind==='set') {
        const [receiver,signature,value]=values;assert.ok([1,2].includes(receiver));
        if(signature==='setAdvNumColors(int)' || signature==='setBatchNumColors(int)') {
          const advanced=signature==='setAdvNumColors(int)';
          if(receiver===2)engine.configureProfile({[advanced?'colorCount':'basicColorCount']:value});
          else {
            const prior=batch.selectedProfile().settings.advanced;
            await batch.command({command:'configure',settings:{advanced,colorCount:value}});
            await batch.command({command:'configure',settings:{advanced:prior}});
          }
        } else {
          const key=slots[signature];assert.ok(key,signature);
          const current=receiver===2?engine.selectedProfile():batch.selectedProfile();
          const settings={[key]:typeof current.settings[key]==='boolean'?Boolean(value):value};
          if(receiver===2)engine.configureProfile({settings});else await batch.command({command:'configure',settings});
        }
      } else if(kind==='color')engine.configureProfile({settings:{background:quantizeQColorChannels(Float32Array.from(values[0]).reverse())}});
      else if(kind==='save')await profiles.command({command:'save',name:values[0]});
      else if(kind==='load')await profiles.command({command:'select',name:values[0]});
      else if(kind==='batch')await batch.command({command:values[0]?'enter':'leave'});
      else if(kind==='export-format')await batch.command({command:'configure',exportFormat:values[0]});
      else if(kind==='export-folder')await batch.command({command:'configure',exportDirectory:join(directory,'output')});
      else if(kind==='reset-application-model')await application.resetModel();
      else if(kind==='persist')await writeProcessingPreferences(preferencesFilename,preferenceSnapshot(context));
      else assert.ok(['snapshot','document'].includes(kind),kind);
      const native=reference.expected[phase][index],label=`${reference.id} phase ${phase} action ${index}`;
      if(native.selected) {
        assert.deepEqual(engine.selectedProfile(),profileFromFrame(native.selected),label+' selected');
        assert.deepEqual(batch.selectedProfile(),profileFromFrame(native.batch),label+' batch');
        assert.deepEqual(profiles.names(),native.profiles.map(entry=>entry.name),label+' profiles');
        assert.deepEqual(batch.snapshot().queue.queue,[],label+' queue');assert.equal(batch.snapshot().queue.exportFormat,native.exportFormat);
        const backup=Buffer.from(native.paletteRuntime.backupArgbBits,'hex');
        assert.deepEqual(engine.paletteRuntime(),{customized:native.paletteRuntime.customized,
          backup:Array.from({length:backup.length/4},(_,i)=>backup.readFloatLE((Math.floor(i/4)*4+3-i%4)*4))},label+' runtime');
      }
      if(kind==='document')compareDocumentState(engine.snapshot().document,native.document.state,parameters);
    }
  }
});

test('processing preferences reject corrupt shapes and session palettes without replacing files',async t=>{
  const directory=await directoryFor(t),context=await owners(directory),filename=context.preferencesFilename;
  await writeProcessingPreferences(filename,preferenceSnapshot(context));const valid=JSON.parse(await readFile(filename,'utf8'));
  const missingOptions=structuredClone(valid);delete missingOptions.outputOptions;missingOptions.other={};
  const palette=structuredClone(valid);palette.selected.settings.palette=[1,1,1,1];
  const badControls=structuredClone(valid);badControls.selected.settings.complexity='invalid';
  const badBitmap=structuredClone(valid);badBitmap.bitmapSave.basic.format='svg';
  const oldShape=structuredClone(valid);delete oldShape.bitmapSave;
  const badScale=structuredClone(valid);badScale.bitmapSave.scale='2';
  for(const stored of ['{',JSON.stringify({}),...[missingOptions,palette,badControls,badBitmap,oldShape,badScale].map(JSON.stringify)]) {
    await writeFile(filename,stored);await assert.rejects(readProcessingPreferences(filename));assert.equal(await readFile(filename,'utf8'),stored);
  }
  await assert.rejects(readProcessingPreferences('relative.json'),/absolute/);
});

test('HTTP bitmap choices and shared size survive restart while fields and filenames stay in the session',async t=>{
  const directory=await directoryFor(t),profileFilename=join(directory,'profiles.json');
  let host=await startGui({port:0,workingDirectory:directory,profileFilename});t.after(()=>host?.close());
  const headers={'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const get=async action=>(await fetch(host.url+'/api/'+action,{headers})).json();
  const post=async(action,input,status=200)=>{
    const response=await fetch(host.url+'/api/'+action,{method:'POST',headers:{...headers,Origin:host.url,'Content-Type':'application/json'},body:JSON.stringify(input)});
    assert.equal(response.status,status);return response.json();
  };
  const vector=await get('quickSave');
  const image=join(directory,'multi.part.bmp');await writeFile(image,Buffer.from(Object.values(inputs)[0],'base64'));
  await post('sources',{command:'open',filename:image});
  assert.equal((await get('bitmapSave')).basic.filename,'multi');
  const vectorAfterSource=await get('quickSave');assert.equal(vectorAfterSource.basic.format,vector.basic.format);
  await post('bitmapSave',{mode:'basic',format:'jpg',directory:'missing-output',expanded:true,filename:'chosen.png'});
  await post('bitmapSave',{mode:'advanced',format:'tif',expanded:false});
  const choices=await get('bitmapSave');assert.equal(choices.basic.format,'png');assert.equal(choices.basic.filename,'chosen');
  await post('bitmapSize',{command:'commit',mode:'basic',field:'percent',text:'150%'});
  const size=await get('bitmapSize');assert.equal(size.scale,1.5);assert.notDeepEqual(size.basic,size.advanced);
  const before=await get('bitmapSave');await post('bitmapSave',{mode:'basic',format:'svg'},400);assert.deepEqual(await get('bitmapSave'),before);
  await post('batch',{command:'enter'});await post('bitmapSave',{mode:'basic',format:'bmp'},409);
  assert.deepEqual(await get('bitmapSave'),before);await post('batch',{command:'leave'});
  assert.deepEqual(await get('quickSave'),vectorAfterSource);
  await host.close();host=null;
  const stored=JSON.parse(await readFile(join(directory,'processing-preferences.json'),'utf8'));
  assert.equal(stored.bitmapSave.scale,1.5);assert.equal(Object.hasOwn(stored.bitmapSave.basic,'filename'),false);
  host=await startGui({port:0,workingDirectory:directory,profileFilename});
  const restored=await get('bitmapSave'),restoredSize=await get('bitmapSize');
  assert.deepEqual(restored.basic,{...stored.bitmapSave.basic,filename:''});
  assert.deepEqual(restored.advanced,{...stored.bitmapSave.advanced,filename:''});
  assert.equal(restoredSize.scale,1.5);assert.deepEqual(restoredSize.basic,restoredSize.advanced);
  assert.equal(restoredSize.basic.width,225);assert.equal(restoredSize.basic.height,375);
  await post('sources',{command:'open',filename:image});
  assert.equal((await get('bitmapSave')).basic.directory,directory);
  assert.equal((await get('bitmapSave')).basic.filename,'multi');
  assert.equal((await get('bitmapSize')).scale,1.5);
});

test('preference writes preserve a blocking entry and clean their own temporary file',async t=>{
  const directory=await directoryFor(t),context=await owners(directory),filename=context.preferencesFilename;
  await mkdir(filename);await writeFile(join(filename,'keep.txt'),'keep');
  await assert.rejects(writeProcessingPreferences(filename,preferenceSnapshot(context)));
  assert.equal(await readFile(join(filename,'keep.txt'),'utf8'),'keep');
  assert.deepEqual((await readdir(directory)).filter(name=>name.startsWith('.openvector-')),[]);
});

test('loaded preferences and constructed owners do not share mutable buffers',async t=>{
  const directory=await directoryFor(t),context=await owners(directory);
  await writeProcessingPreferences(context.preferencesFilename,preferenceSnapshot(context));
  const preferences=await readProcessingPreferences(context.preferencesFilename);
  const engine=new GuiEngine({profile:preferences.selected,outputOptions:preferences.outputOptions}),batch=new BatchController({engine,applicationDirectory:directory,
    workingDirectory:directory,cancellation:new Int32Array(new SharedArrayBuffer(4)),preferences:preferences.batch});
  const before={selected:engine.selectedProfile(),batch:batch.snapshot()};preferences.selected.settings.background.fill(.5);
  preferences.batch.profile.settings.background.fill(.5);preferences.outputOptions.layout='cutouts';
  assert.deepEqual({selected:engine.selectedProfile(),batch:batch.snapshot()},before);
});

test('HTTP clean idle shutdown saves preferences and restart restores controls without the source or batch workspace',async t=>{
  const directory=await directoryFor(t),profileFilename=join(directory,'profiles.json'),preferencesFilename=join(directory,'processing-preferences.json');
  let host=await startGui({port:0,workingDirectory:directory,profileFilename});
  t.after(()=>host?.close());
  const post=async(action,input)=>{
    const response=await fetch(host.url+'/api/'+action,{method:'POST',headers:{Origin:host.url,'Content-Type':'application/json',
      'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'},body:JSON.stringify(input)});
    assert.equal(response.status,200);return response.json();
  };
  const image=join(directory,'image.bmp');await writeFile(image,Buffer.from(Object.values(inputs)[0],'base64'));
  await post('select',{settings:{advanced:true,complexity:11,background:[.2,.3,.4,.5]},colorCount:9});
  const loaded=await post('sources',{command:'open',filename:image});assert.equal(loaded.document.file.name,'image.bmp');
  await post('batch',{command:'enter'});await post('batch',{command:'add',filenames:[image]});await post('batch',{command:'page',page:'settings'});
  await post('batch',{command:'configure',settings:{advanced:true,colorCount:7,smoothness:10},exportFormat:'svg',exportDirectory:directory});
  await assert.rejects(readFile(preferencesFilename),{code:'ENOENT'});
  await host.close();host=null;
  const stored=JSON.parse(await readFile(preferencesFilename,'utf8'));assert.equal(stored.selected.colorCount,9);assert.deepEqual(stored.selected.settings.palette,[]);
  host=await startGui({port:0,workingDirectory:directory,profileFilename});
  const selection=await post('select',{});assert.equal(selection.count,9);assert.equal(selection.settings.complexity,11);assert.equal(selection.settings.background[3],1);
  const state=await fetch(host.url+'/api/state',{headers:{'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'}});assert.equal(await state.json(),null);
  const batch=await (await fetch(host.url+'/api/batch',{headers:{'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'}})).json();
  assert.equal(batch.active,false);assert.equal(batch.page,'files');assert.deepEqual(batch.queue.queue,[]);
  assert.equal(batch.colorCounts.advanced,7);assert.equal(batch.settings.smoothness,10);assert.equal(batch.queue.exportFormat,'svg');assert.equal(batch.queue.exportDirectory,directory);
});

test('HTTP shutdown reports a preference write error and releases the host',async t=>{
  const directory=await directoryFor(t),preferencesFilename=join(directory,'processing-preferences.json');
  await mkdir(preferencesFilename);await writeFile(join(preferencesFilename,'keep.txt'),'keep');
  await assert.rejects(startGui({port:0,profileFilename:join(directory,'profiles.json')}),/processing preferences/);
  assert.equal(dirname(preferencesFilename),directory);await rm(preferencesFilename,{recursive:true});
  const host=await startGui({port:0,profileFilename:join(directory,'profiles.json')});
  await mkdir(preferencesFilename);await writeFile(join(preferencesFilename,'keep.txt'),'keep');
  await assert.rejects(host.close());assert.equal(await readFile(join(preferencesFilename,'keep.txt'),'utf8'),'keep');
  await assert.rejects(fetch(host.url,{headers:{'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'}}));
});
