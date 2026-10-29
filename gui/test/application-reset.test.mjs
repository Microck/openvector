import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,mkdir,writeFile,readFile,rm,readdir} from 'node:fs/promises';
import {join,dirname,resolve,win32} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {BatchController} from '../batch-controller.mjs';
import {ProfileController} from '../profile-controller.mjs';
import {SourceController} from '../source-controller.mjs';
import {ApplicationController} from '../application-controller.mjs';
import {startGui} from '../server.mjs';
import {SavedAdvancedProfiles} from '../../src/saved-profiles.mjs';
import {profileFromFrame,compareDocumentState} from '../../test/helpers/native-gui.mjs';

const references=gunzipSync(readFileSync(new URL('../../test/fixtures/application-model-reset-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const parameters=JSON.parse(readFileSync(new URL('../../test/fixtures/processing-reset-parameters.json',import.meta.url),'utf8'));
const nativeInputs=JSON.parse(readFileSync(new URL('../../test/fixtures/application-model-reset-inputs.json',import.meta.url),'utf8'));
const inputBytes=Buffer.from(Object.values(nativeInputs)[0],'base64');

async function context(t) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-application-reset-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-application-reset-')));
    await rm(directory,{recursive:true,force:true});
  });
  const profileFilename=join(directory,'profiles.json'),sourceFilename=join(directory,'sources.json');
  const engine=new GuiEngine(),batch=new BatchController({engine,applicationDirectory:directory,workingDirectory:directory,
    cancellation:new Int32Array(new SharedArrayBuffer(4))});
  const profiles=await ProfileController.open({engine,batch,filename:profileFilename});
  const sources=await SourceController.open({engine,batch,filename:sourceFilename,applicationDirectory:directory,workingDirectory:directory});
  const application=new ApplicationController({engine,batch,profiles,sources});
  return {directory,profileFilename,sourceFilename,engine,batch,profiles,sources,application};
}

// Only phase-zero model behavior belongs to this coordinator. Native restart
// preferences and the window handler remain separate unfinished requirements.
for(const reference of references.filter(row=>row.phases[0].some(action=>action[0]==='reset-application-model')))
test('native application model reset: '+reference.id,async t=>{
  const {directory,engine,batch,profiles,sources,application}=await context(t),local=new Map();
  for(const [filename,bytes] of Object.entries(nativeInputs)) {
    const target=join(directory,win32.basename(win32.dirname(filename)),win32.basename(filename));
    await mkdir(dirname(target),{recursive:true});await writeFile(target,Buffer.from(bytes,'base64'));local.set(filename,target);
  }
  for(const [index,[kind,...values]] of reference.phases[0].entries()) {
    if(kind==='automatic') {
      await sources.command({command:'open',filename:local.get(values[0])});engine.automatic({settings:engine.selectedProfile().settings});
    } else if(kind==='save')await profiles.command({command:'save',name:values[0]});
    else if(kind==='batch')await batch.command({command:values[0]?'enter':'leave'});
    else if(kind==='set') {
      const [receiver,slot,value]=values;
      if(receiver===0) {
        assert.equal(slot,'setWizardPage(WizardPageIdType)');assert.ok([1,2].includes(value));
        await batch.command({command:'page',page:value===1?'files':'settings'});
      } else {
        assert.equal(receiver,1);
        const key={'setUseAdvancedMode(bool)':'advanced','setAdvNumColors(int)':'colorCount',
          'setAdvSegmentationComplexity(int)':'complexity','setAdvContourSmoothness(int)':'smoothness','setFlattenImageMode(int)':'backgroundMode'}[slot];
        assert.ok(key,slot);await batch.command({command:'configure',settings:{[key]:key==='advanced'?Boolean(value):value}});
      }
    } else if(kind==='add-files')await batch.command({command:'add',filenames:values[0].map(name=>local.get(name))});
    else if(kind==='export-format')await batch.command({command:'configure',exportFormat:values[0]});
    else if(kind==='export-folder')await batch.command({command:'configure',exportDirectory:directory});
    else if(kind==='reset-application-model') {
      const before=batch.snapshot(),sourceDirectory=sources.snapshot().directory;
      const reset=await application.resetModel();assert.equal(reset.document,null);
      assert.equal(reset.batch.active,before.active);assert.equal(reset.batch.page,before.page);
      assert.equal(reset.batch.queue.exportDirectory,before.queue.exportDirectory);assert.deepEqual(reset.batch.exportOptions,before.exportOptions);
      assert.equal(reset.sources.directory,sourceDirectory);assert.deepEqual(reset.sources.files,[]);assert.deepEqual(reset.sources.directoryHistory.directories,[]);
      reset.selection.settings.background.fill(0);reset.profiles.names.push('mutated');
      assert.deepEqual(application.snapshot().selection.settings.background,[1,1,1,0]);assert.deepEqual(profiles.names(),['Default Advanced Settings']);
    } else assert.ok(['snapshot','document','persist'].includes(kind),kind);
    const native=reference.expected[0][index];
    if(native.processing) {
      assert.equal(batch.active,native.processing.isBatchMode,'workspace at '+index);
      if(batch.active && [1,2].includes(native.processing.wizardPage))assert.equal(batch.snapshot().page,native.processing.wizardPage===1?'files':'settings');
      assert.deepEqual(batch.snapshot().queue.queue.map(({filename,buttonIndex})=>({filename,buttonIndex})),
        native.queue.map(({filename,buttonIndex})=>({filename:local.get(filename),buttonIndex})),'native queue at '+index);
      assert.equal(batch.snapshot().queue.processingIndex,native.processingIndex);assert.equal(batch.snapshot().queue.exportFormat,native.exportFormat);
    }
    if(native.selected) {
      assert.deepEqual(engine.selectedProfile(),profileFromFrame(native.selected),'selected profile at '+index);
      assert.deepEqual(batch.selectedProfile(),profileFromFrame(native.batch),'batch profile at '+index);
      assert.deepEqual(profiles.names(),native.profiles.map(entry=>entry.name),'saved list at '+index);
      assert.deepEqual(sources.snapshot().files.map(({thumbnail,...entry})=>entry),native.sourceHistory.files.map(entry=>({...entry,filename:local.get(entry.filename)})));
    }
    if(kind==='document')compareDocumentState(engine.snapshot().document,native.document.state,parameters);
  }
});

for(const failedStores of [['profiles'],['sources'],['profiles','sources']])
test('application reset preserves session state and attempts both stores: '+failedStores.join('+'),async t=>{
  const setup=await context(t),filename=join(setup.directory,'image.bmp');await writeFile(filename,inputBytes);
  await setup.sources.command({command:'open',filename});await setup.profiles.command({command:'save',name:'custom'});
  await setup.batch.command({command:'enter'});await setup.batch.command({command:'page',page:'settings'});
  const directory=setup.sources.snapshot().directory,targets={profiles:setup.profileFilename,sources:setup.sourceFilename};
  for(const name of failedStores) {await rm(targets[name]);await mkdir(targets[name]);await writeFile(join(targets[name],'keep.txt'),'keep');}
  await assert.rejects(setup.application.resetModel(),error=>{
    for(const name of failedStores)assert.match(error.message,name==='profiles'?/saved profiles:/ : /source history:/);
    assert.deepEqual(error.applicationResult,setup.application.snapshot());return true;
  });
  const after=setup.application.snapshot();assert.equal(after.document,null);assert.equal(after.batch.active,true);assert.equal(after.batch.page,'settings');
  assert.deepEqual(after.profiles.names,['Default Advanced Settings']);assert.equal(after.sources.directory,directory);assert.deepEqual(after.sources.files,[]);
  for(const name of failedStores)assert.equal(await readFile(join(targets[name],'keep.txt'),'utf8'),'keep');
  if(!failedStores.includes('profiles'))assert.deepEqual((await SavedAdvancedProfiles.readFile(targets.profiles)).snapshot().map(entry=>entry.name),['Default Advanced Settings']);
  if(!failedStores.includes('sources')) {
    const stored=JSON.parse(await readFile(targets.sources,'utf8'));assert.deepEqual(stored.files,[]);assert.deepEqual(stored.directories,[]);
  }
  assert.ok(!(await readdir(setup.directory)).some(name=>name.startsWith('.openvector-')));
});

async function httpContext(t) {
  const setup=await context(t),filename=join(setup.directory,'image.bmp');await writeFile(filename,inputBytes);
  const host=await startGui({port:0,workingDirectory:setup.directory,profileFilename:setup.profileFilename,sourceFilename:setup.sourceFilename});
  t.after(()=>host.close());
  const headers={'Content-Type':'application/json','User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const post=(action,input,extra={})=>fetch(host.url+'/api/'+action,{method:'POST',headers:{...headers,Origin:host.url,...extra},body:JSON.stringify(input)});
  const get=async action=>{const response=await fetch(host.url+'/api/'+action,{headers});assert.equal(response.status,200);return response.json();};
  assert.equal((await post('sources',{command:'open',filename})).status,200);
  assert.equal((await post('profiles',{command:'save',name:'custom'})).status,200);
  return {...setup,host,filename,post,get};
}

test('HTTP application model reset preserves batch page, rejects invalid origins/input and publishes current state',async t=>{
  const {post,get,filename}=await httpContext(t);
  await post('batch',{command:'enter'});await post('batch',{command:'page',page:'settings'});
  await post('batch',{command:'add',filenames:[filename]});await post('batch',{command:'page',page:'settings'});
  await post('batch',{command:'configure',settings:{advanced:true,colorCount:9,complexity:11}});
  const before=await get('batch'),history=await get('sources');
  assert.equal((await post('resetApplicationModel',null,{Origin:'https://example.com'})).status,403);
  assert.equal((await post('resetApplicationModel',{sourceFilename:'ignored'})).status,400);
  assert.deepEqual(await get('batch'),before);assert.deepEqual(await get('sources'),history);
  const response=await post('resetApplicationModel',null);assert.equal(response.status,200);const reset=await response.json();
  assert.equal(reset.document,null);assert.equal(reset.batch.active,true);assert.equal(reset.batch.page,'settings');
  assert.equal(reset.batch.settings.complexity,11);assert.equal(reset.batch.colorCounts.advanced,9);assert.deepEqual(reset.batch.queue.queue,[]);
  assert.deepEqual(reset.batch,await get('batch'));assert.ok(reset.batch.revision>before.revision);
  assert.deepEqual(reset.sources,await get('sources'));assert.equal(reset.sources.directory,history.directory);
  assert.deepEqual(reset.sources.files,[]);assert.deepEqual(reset.sources.directoryHistory.directories,[]);
  assert.deepEqual(reset.profiles,await get('profiles'));assert.deepEqual(reset.profiles.names,['Default Advanced Settings']);assert.equal(await get('state'),null);
});

test('HTTP failed reset write returns actual reset state and current batch revision',async t=>{
  const {post,get,profileFilename,sourceFilename}=await httpContext(t);
  await post('batch',{command:'enter'});await post('batch',{command:'page',page:'settings'});
  await rm(profileFilename);await mkdir(profileFilename);await writeFile(join(profileFilename,'keep.txt'),'keep');
  const response=await post('resetApplicationModel',null);assert.equal(response.status,400);const failed=await response.json();
  assert.match(failed.error,/saved profiles:/);assert.equal(failed.applicationResult.document,null);
  assert.deepEqual(failed.applicationResult.batch,await get('batch'));assert.equal(failed.applicationResult.batch.active,true);
  assert.deepEqual(failed.applicationResult.sources,await get('sources'));assert.deepEqual(failed.applicationResult.profiles,await get('profiles'));
  assert.deepEqual(JSON.parse(await readFile(sourceFilename,'utf8')).files,[]);assert.equal(await readFile(join(profileFilename,'keep.txt'),'utf8'),'keep');
});

test('HTTP application reset rejects a real pending batch before mutation',async t=>{
  const {post,get,filename,directory}=await httpContext(t),output=join(directory,'output');await mkdir(output);
  await writeFile(join(output,'image.svg'),'existing');await post('batch',{command:'add',filenames:[filename]});
  await post('batch',{command:'configure',exportDirectory:output,exportFormat:'svg'});await post('batch',{command:'start'});
  let batch;const deadline=performance.now()+5000;
  do {batch=await get('batch');if(batch.job.status==='overwrite')break;await new Promise(resolve=>setTimeout(resolve,10));} while(performance.now()<deadline);
  assert.equal(batch.job.status,'overwrite');
  const profiles=await get('profiles'),sources=await get('sources');assert.equal((await post('resetApplicationModel',null)).status,409);
  assert.deepEqual(await get('profiles'),profiles);assert.deepEqual(await get('sources'),sources);assert.equal(await readFile(join(output,'image.svg'),'utf8'),'existing');
  await post('batch',{command:'decide',decision:'cancel'});
  do {batch=await get('batch');if(!batch.job.running)break;await new Promise(resolve=>setTimeout(resolve,10));} while(performance.now()<deadline);
  assert.equal(batch.job.status,'cancelled');
});
