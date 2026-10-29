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
import {startGui} from '../server.mjs';
import {SavedAdvancedProfiles} from '../../src/saved-profiles.mjs';
import {profileFromFrame} from '../../test/helpers/native-gui.mjs';

const references=gunzipSync(readFileSync(new URL('../../test/fixtures/factory-reset-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

async function context(t) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-factory-reset-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-factory-reset-')));
    await rm(directory,{recursive:true,force:true});
  });
  const applicationDirectory=join(directory,'app'),workingDirectory=join(directory,'inputs');
  await mkdir(applicationDirectory);await mkdir(workingDirectory);
  const options={applicationDirectory,workingDirectory,profileFilename:join(directory,'profiles.json'),sourceFilename:join(directory,'sources.json')};
  const open=async()=>{
    const engine=new GuiEngine(),batch=new BatchController({engine,...options,cancellation:new Int32Array(new SharedArrayBuffer(4))});
    const profileOwner=await SavedAdvancedProfiles.readFile(options.profileFilename);
    return {engine,batch,profileOwner,profiles:new ProfileController({engine,batch,filename:options.profileFilename,profiles:profileOwner}),
      sources:await SourceController.open({engine,batch,...options,filename:options.sourceFilename})};
  };
  return {directory,options,open};
}

// Compare only the list stage of both captured reset paths. The remaining model
// and window reset effects are explicit unfinished work in the reset contract.
for(const reference of references.filter(row=>!row.pending))test('native reset list ownership: '+reference.id,async t=>{
  const setup=await context(t),local=new Map();
  for(const [filename,bytes] of Object.entries(reference.inputs)) {
    const target=join(setup.options.workingDirectory,win32.basename(win32.dirname(filename)),win32.basename(filename));
    await mkdir(dirname(target),{recursive:true});await writeFile(target,Buffer.from(bytes,'base64'));local.set(filename,target);
  }
  const localDirectory=native=>{
    for(const [filename,target] of local)if(win32.dirname(filename).replaceAll('\\','/')===native)return dirname(target).replaceAll('\\','/');
    assert.equal(native,'D:/migracion/vector-magic-re/tools/samples');
    return join(setup.options.applicationDirectory,'samples').replaceAll('\\','/');
  };
  for(const [phase,actions] of reference.phases.entries()) {
    const {engine,batch,profileOwner,profiles,sources}=await setup.open();
    for(const [index,[kind,...values]] of actions.entries()) {
      if(kind==='open-wide')await sources.command({command:'open',filename:local.get(values[0])});
      else if(kind==='set') {
        const [receiver,signature,value]=values;
        if(receiver===1)await batch.command({command:'configure',settings:{colorCount:value}});
        else if(signature==='setAdvNumColors(int)')engine.configureProfile({colorCount:value});
        else {
          const key={'setAdvSegmentationComplexity(int)':'complexity','setAdvContourSmoothness(int)':'smoothness'}[signature];
          assert.ok(key,signature);engine.configureProfile({settings:{[key]:value}});
        }
      } else if(kind==='save')await profiles.command({command:'save',name:values[0]});
      else if(kind==='reset-lists' || kind==='reset-model') {
        const before=engine.snapshot(),queue=batch.snapshot();
        await profiles.command({command:'reset'});await sources.command({command:'reset-history'});
        assert.deepEqual(engine.snapshot(),before,'list reset does not apply defaults to the current document');
        assert.deepEqual(batch.snapshot(),queue,'list reset does not clear the batch queue');
      } else if(kind==='add-files')await batch.command({command:'add',filenames:values[0].map(filename=>local.get(filename))});
      else if(kind==='export-format')await batch.command({command:'configure',exportFormat:values[0]});
      else if(kind==='export-folder')await batch.command({command:'configure',exportDirectory:setup.directory});
      else assert.ok(['snapshot','document','persist'].includes(kind),kind);
      const expected=reference.expected[phase][index];
      if(expected.profiles) {
        assert.deepEqual(profileOwner.snapshot(),expected.profiles.map(row=>({name:row.name,profile:profileFromFrame(row)})),`in-memory profiles ${phase}/${index}`);
        assert.deepEqual(profiles.names(),expected.profiles.map(row=>row.name));
      }
      if(expected.sourceHistory) {
        const state=sources.snapshot(),native=expected.sourceHistory;
        assert.equal(state.capacity,native.capacity);assert.equal(state.directory,localDirectory(native.directory));
        assert.deepEqual(state.files.map(({thumbnail,...entry})=>entry),native.files.map(entry=>({...entry,filename:local.get(entry.filename)})));
        assert.deepEqual(state.directoryHistory,{capacity:expected.sourceDirectories.capacity,
          directories:expected.sourceDirectories.directories.map(localDirectory)});
      }
    }
  }
});

test('reset recreates an evicted, renamed or removed built-in profile with owned defaults',()=>{
  const expected=references.find(row=>row.id==='fresh-business-reset').expected[0][1].profiles
    .map(row=>({name:row.name,profile:profileFromFrame(row)}));
  for(const prepare of [profiles=>profiles.remove(expected[0].name),profiles=>profiles.rename(expected[0].name,'renamed'),
    profiles=>{for(let i=0;i<200;i++)profiles.put('custom '+i,expected[0].profile);}]) {
    const profiles=new SavedAdvancedProfiles();prepare(profiles);const previous=profiles.snapshot();
    profiles.reset();assert.deepEqual(profiles.snapshot(),expected);
    previous[0]?.profile.settings.background.fill(0);assert.deepEqual(profiles.snapshot(),expected);
    profiles.reset();assert.deepEqual(profiles.snapshot(),expected);
  }
});

test('HTTP list resets preserve loaded work, reject batch/foreign origins and survive restart',async t=>{
  const setup=await context(t),reference=references[1],filename=join(setup.options.workingDirectory,'image.bmp');
  await writeFile(filename,Buffer.from(Object.values(reference.inputs)[0],'base64'));
  let host=await startGui({port:0,...setup.options});t.after(()=>host.close());
  const headers={'Content-Type':'application/json','User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const post=(action,input,extra={})=>fetch(host.url+'/api/'+action,{method:'POST',headers:{...headers,Origin:host.url,...extra},body:JSON.stringify(input)});
  const get=async action=>{const response=await fetch(host.url+'/api/'+action,{headers});assert.equal(response.status,200);return response.json();};
  assert.equal((await post('sources',{command:'open',filename})).status,200);
  assert.equal((await post('profiles',{command:'save',name:'custom'})).status,200);
  const document=await get('state'),history=await get('sources');
  assert.equal((await post('profiles',{command:'reset'},{Origin:'https://example.com'})).status,403);
  assert.equal((await post('sources',{command:'reset-history'},{Origin:'https://example.com'})).status,403);
  assert.equal((await post('batch',{command:'enter'})).status,200);
  assert.equal((await post('profiles',{command:'reset'})).status,400);
  assert.equal((await post('sources',{command:'reset-history'})).status,409);
  assert.deepEqual(await get('sources'),history);assert.deepEqual(await get('profiles'),{names:['custom','Default Advanced Settings']});
  assert.equal((await post('batch',{command:'leave'})).status,200);
  for(const [action,command] of [['profiles','reset'],['sources','reset-history']])assert.equal((await post(action,{command})).status,200);
  assert.deepEqual(await get('state'),document);
  const empty={...history,files:[],directoryHistory:{capacity:4,directories:[]}};
  assert.deepEqual(await get('sources'),empty);assert.deepEqual(await get('profiles'),{names:['Default Advanced Settings']});
  await host.close();host=await startGui({port:0,...setup.options});
  assert.equal(await get('state'),null);assert.deepEqual(await get('sources'),empty);
  assert.deepEqual(await get('profiles'),{names:['Default Advanced Settings']});
});

test('failed list-reset writes preserve session effects and existing filesystem targets',async t=>{
  const setup=await context(t),{engine,batch,profiles,sources}=await setup.open();
  const filename=join(setup.options.workingDirectory,'image.bmp');
  await writeFile(filename,Buffer.from(Object.values(references[1].inputs)[0],'base64'));
  await sources.command({command:'open',filename});await profiles.command({command:'save',name:'custom'});
  const document=engine.snapshot(),directory=sources.snapshot().directory;
  for(const target of [setup.options.profileFilename,setup.options.sourceFilename]) {
    await rm(target);await mkdir(target);await writeFile(join(target,'keep.txt'),'keep');
  }
  await assert.rejects(profiles.command({command:'reset'}));assert.deepEqual(profiles.names(),['Default Advanced Settings']);
  await assert.rejects(sources.command({command:'reset-history'}),error=>{
    assert.equal(error.sourceResult.loaded,false);assert.deepEqual(error.sourceResult.history,sources.snapshot());return true;
  });
  assert.deepEqual(sources.snapshot(),{directory,capacity:3,files:[],directoryHistory:{capacity:4,directories:[]}});
  assert.deepEqual(engine.snapshot(),document);assert.equal(batch.active,false);
  for(const target of [setup.options.profileFilename,setup.options.sourceFilename]) {
    assert.equal(await readFile(join(target,'keep.txt'),'utf8'),'keep');assert.deepEqual(await readdir(target),['keep.txt']);
  }
  assert.deepEqual((await readdir(setup.directory)).sort(),['app','inputs','profiles.json','sources.json']);
});
