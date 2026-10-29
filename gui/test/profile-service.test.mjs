import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,mkdir,readFile,readdir,rm,writeFile} from 'node:fs/promises';
import {join,dirname,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {request as httpRequest} from 'node:http';
import {GuiEngine} from '../engine.mjs';
import {BatchController} from '../batch-controller.mjs';
import {ProfileController} from '../profile-controller.mjs';
import {startGui} from '../server.mjs';
import {SavedAdvancedProfiles,defaultAdvancedProfileName} from '../../src/saved-profiles.mjs';
import {quantizeQColorChannels} from '../../src/pixel-input.mjs';
import {compareDocumentState,profileFromFrame} from '../../test/helpers/native-gui.mjs';

const slots = {'setUseAdvancedMode(bool)':'advanced','setPaletteMode(int)':'paletteMode',
  'setAdvSegmentationComplexity(int)':'complexity','setAdvContourSmoothness(int)':'smoothness',
  'setImageCategory(int)':'category','setImageQuality(int)':'quality','setFlattenImageMode(int)':'backgroundMode'};
const corpora = ['profile-document','profile-lifecycle','profile-palettes'].map(name =>
  gunzipSync(readFileSync(new URL('../../test/fixtures/'+name+'-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse));

async function context(t) {
  const directory = await mkdtemp(join(tmpdir(),'openvector-profile-service-'));
  t.after(async () => {
    assert.equal(dirname(directory),resolve(tmpdir()));
    assert.ok(directory.startsWith(join(tmpdir(),'openvector-profile-service-')));
    await rm(directory,{recursive:true,force:true});
  });
  const engine = new GuiEngine(),batch = new BatchController({engine,applicationDirectory:directory,
    workingDirectory:directory,cancellation:new Int32Array(new SharedArrayBuffer(4))});
  const filename = join(directory,'settings','profiles.json');
  return {directory,filename,engine,batch,profiles:await ProfileController.open({engine,batch,filename})};
}

for (const [provenance,...references] of corpora) for (const reference of references)
test('profile service matches native selection lifetime: '+reference.id,async t => {
  const {engine,batch,profiles} = await context(t);
  for (const [index,action] of reference.actions.entries()) {
    const [kind,...values] = action,expected = reference.expected[index];
    if (kind === 'open' || kind === 'automatic') {
      const source = values[0];engine.open({bytes:Buffer.from(reference.inputs[source],'base64'),name:source});
      if (kind === 'automatic') engine.automatic({settings:engine.selectedProfile().settings});
    } else if (kind === 'set') {
      const [receiver,signature,value] = values;
      if (receiver === 1) {
        assert.equal(signature,'setBatchNumColors(int)');await batch.command({command:'configure',settings:{colorCount:value}});
      } else if (signature === 'setAdvNumColors(int)') engine.configureProfile({colorCount:value});
      else if (signature === 'addColor()') engine.palette({action:'add'});
      else if (signature === 'deleteColor(int)') engine.palette({action:'delete',index:value});
      else if (signature === 'revertPaletteEdits()') engine.palette({action:'revert'});
      else if (signature === 'setPaletteFromList(int)') engine.palette({action:'choose',
        palette:engine.state().palettes.find(candidate => candidate.count === value+1).colors.flat()});
      else {
        const name = slots[signature];assert.ok(name,signature);
        engine.configureProfile({settings:{[name]:name === 'advanced' ? Boolean(value) : value}});
      }
    } else if (kind === 'color') engine.configureProfile({settings:{
      background:quantizeQColorChannels(Float32Array.from(values[0]).reverse())}});
    else if (kind === 'save') await profiles.command({command:'save',name:values[0],force:true});
    else if (kind === 'batch') await batch.command({command:values[0] ? 'enter' : 'leave'});
    else if (kind === 'load') {
      if (profiles.names().includes(values[0])) await profiles.command({command:'select',name:values[0]});
      else await assert.rejects(profiles.command({command:'select',name:values[0]}),/not found/);
    } else assert.ok(['snapshot','document'].includes(kind),kind);
    if (expected.selected) {
      assert.deepEqual(engine.selectedProfile(),profileFromFrame(expected.selected),'full selected profile at '+index);
      assert.deepEqual(batch.selectedProfile(),profileFromFrame(expected.batch),'full requested batch profile at '+index);
      assert.deepEqual(profiles.names(),expected.profiles.map(row => row.name),'native list order at '+index);
    }
    if (kind === 'document') {
      compareDocumentState(engine.snapshot().document,expected.document.state,provenance.parameters);
      assert.equal(engine.snapshot().committedCount,Buffer.from(expected.document.state.settingsBits,'hex').readInt32LE(8));
    }
  }
});

test('profile service preserves exact names, owns selected buffers and restores native persisted values',async t => {
  const {directory,filename,engine,batch,profiles} = await context(t);
  assert.deepEqual(await readdir(directory),[],'startup creates no directory or file');
  assert.deepEqual(await profiles.command({command:'list'}),{names:[defaultAdvancedProfileName]});
  const names = ['', ' ', 'Mixed', 'mixed', 'é', 'e\u0301', '設定 🖋'];
  engine.configureProfile({colorCount:7,paletteColorCount:5,settings:{complexity:11,
    background:[.123,.456,.789,.5],palette:[0,0,0,1,1,1,1,1]}});
  const saved = engine.selectedProfile();
  for (const name of names) await profiles.command({command:'save',name,profile:null,filename:join(directory,'ignored.json')});
  assert.deepEqual(profiles.names(),[...names.toReversed(),defaultAdvancedProfileName]);
  assert.deepEqual((await readdir(directory)).sort(),['settings'],'browser input cannot redirect storage');
  const bytes = await readFile(filename),before = engine.snapshot(),order = profiles.names();
  for (const input of [null,[],{command:'unknown'},{command:'save',name:12},{command:'save',name:'Mixed',force:'yes'},
    {command:'save',name:'Mixed'},{command:'select',name:'missing'}]) {
    await assert.rejects(profiles.command(input));assert.deepEqual(engine.snapshot(),before);
    assert.deepEqual(profiles.names(),order);assert.deepEqual(await readFile(filename),bytes);
  }
  engine.configureProfile({settings:{complexity:3},paletteColorCount:2});
  const selected = await profiles.command({command:'select',name:'Mixed'});
  assert.deepEqual(engine.selectedProfile(),{...saved,settings:{...saved.settings,advanced:true}});
  assert.equal(engine.snapshot().document.settings.advanced,saved.settings.advanced);
  selected.selection.settings.palette.fill(0);assert.deepEqual(engine.selectedProfile().settings.palette,saved.settings.palette);
  const restarted = await ProfileController.open({engine,batch,filename});
  assert.equal(restarted.names()[0],defaultAdvancedProfileName);
  await restarted.command({command:'select',name:'Mixed'});
  const restored = engine.selectedProfile();assert.deepEqual(restored.settings.palette,new Float32Array());
  assert.equal(restored.colorCount,7);assert.equal(restored.paletteColorCount,5);
  assert.equal(restored.settings.background[3],1);
  assert.deepEqual(profiles.names()[0],'Mixed','restart cannot mutate an earlier session owner');
});

test('profile service reports malformed storage and failed writes without repairing or discarding prior files',async t => {
  const {directory,engine,batch} = await context(t),filename = join(directory,'invalid.json');
  await writeFile(filename,'{broken');
  await assert.rejects(ProfileController.open({engine,batch,filename}));assert.equal(await readFile(filename,'utf8'),'{broken');
  await assert.rejects(ProfileController.open({engine,batch,filename:'relative.json'}),/absolute/);
  const target = join(directory,'target.json'),profiles = await ProfileController.open({engine,batch,filename:target});
  await mkdir(target);await writeFile(join(target,'keep.txt'),'keep');
  await assert.rejects(profiles.command({command:'save',name:'retained-in-session'}));
  assert.equal(await readFile(join(target,'keep.txt'),'utf8'),'keep');
  assert.deepEqual(profiles.names(),['retained-in-session',defaultAdvancedProfileName]);
  assert.deepEqual((await readdir(directory)).sort(),['invalid.json','target.json']);
  assert.deepEqual((await SavedAdvancedProfiles.readFile(join(directory,'absent.json'))).size,1);
});

test('HTTP profile service serializes writes, respects origin checks and persists through actual worker restart',async t => {
  const {filename,directory} = await context(t);
  let host = await startGui({port:0,workingDirectory:directory,profileFilename:filename});
  t.after(() => host.close());
  const headers = {'Content-Type':'application/json','User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const post = (action,input,extra = {}) => fetch(host.url+'/api/'+action,{method:'POST',
    headers:{...headers,Origin:host.url,...extra},body:JSON.stringify(input)});
  const get = action => fetch(host.url+'/api/'+action,{headers});
  assert.deepEqual(await (await get('profiles')).json(),{names:[defaultAdvancedProfileName]});
  assert.equal((await post('select',{colorCount:7,paletteColorCount:5,settings:{complexity:11}})).status,200);
  const body = JSON.stringify({command:'save',name:'Setting 🖋'});
  const started = Promise.withResolvers(),finished = Promise.withResolvers();
  const held = httpRequest(host.url+'/api/profiles',{method:'POST',headers:{...headers,Origin:host.url,
    'Content-Length':Buffer.byteLength(body)}},response => {
    const chunks=[];response.on('data',chunk => chunks.push(chunk));
    response.on('end',() => finished.resolve({status:response.statusCode,value:JSON.parse(Buffer.concat(chunks))}));
  });
  held.on('error',finished.reject);t.after(() => held.destroy());held.write(body.slice(0,5),started.resolve);await started.promise;
  assert.equal((await get('profiles')).status,200,'read acknowledges that the host has accepted the held mutation');
  assert.equal((await post('profiles',{command:'save',name:'second'})).status,409);
  held.end(body.slice(5));assert.equal((await finished.promise).status,200);
  assert.equal((await post('profiles',{command:'save',name:'forbidden'},{Origin:'https://example.com'})).status,403);
  assert.equal((await post('profiles',{command:'save',name:'Setting 🖋'})).status,400);
  await host.close();host = await startGui({port:0,workingDirectory:directory,profileFilename:filename});
  assert.deepEqual(await (await get('profiles')).json(),{names:[defaultAdvancedProfileName,'Setting 🖋']});
  const applied = await post('profiles',{command:'select',name:'Setting 🖋'});assert.equal(applied.status,200);
  const selected = await applied.json();assert.equal(selected.selection.count,7);
  assert.equal(selected.selection.paletteColorCount,5);assert.equal(selected.selection.settings.complexity,11);
  assert.equal(selected.document,null,'cold selection does not invent an image');
  await post('batch',{command:'enter'});
  const batch = await post('profiles',{command:'select',name:'Setting 🖋'});assert.equal(batch.status,200);
  const batchSelection = (await batch.json()).batch;
  assert.equal(batchSelection.settings.colorCount,7);
  assert.deepEqual(batchSelection,await (await get('batch')).json(),'profile replies use the authoritative host revision and thumbnail projection');
  assert.equal((await post('select',{settings:{complexity:3}})).status,409,'other image mutations remain blocked in batch mode');
});

test('HTTP profile startup reports malformed storage without creating a listening application or repairing the file',async t => {
  const {filename,directory} = await context(t);
  await mkdir(dirname(filename));await writeFile(filename,'{broken');
  await assert.rejects(startGui({port:0,workingDirectory:directory,profileFilename:filename}));
  assert.equal(await readFile(filename,'utf8'),'{broken');
});

test('HTTP profile reads remain available during a real paused batch while all profile mutations are rejected',async t => {
  const {directory,filename,engine,batch,profiles} = await context(t);
  await profiles.command({command:'save',name:'saved'});
  const reference = corpora[0][1],source = reference.actions[0][1];
  const input = join(directory,'image.png'),output = join(directory,'vectors');
  await writeFile(input,Buffer.from(reference.inputs[source],'base64'));await mkdir(output);
  await writeFile(join(output,'image.svg'),'preserved');
  const host = await startGui({port:0,workingDirectory:directory,profileFilename:filename});t.after(() => host.close());
  const headers = {'Content-Type':'application/json',Origin:host.url,'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const post = (action,input) => fetch(host.url+'/api/'+action,{method:'POST',headers,body:JSON.stringify(input)});
  await post('batch',{command:'enter'});await post('batch',{command:'add',filenames:[input]});
  await post('batch',{command:'configure',exportDirectory:output,exportFormat:'svg'});
  assert.equal((await post('batch',{command:'start'})).status,200);
  const deadline = Date.now()+5000;
  let state;
  do {
    assert.ok(Date.now()<deadline,'batch reaches overwrite decision');
    state = await (await fetch(host.url+'/api/batch',{headers})).json();
    if (state.job.status !== 'overwrite') await new Promise(resolve => setTimeout(resolve,10));
  } while (state.job.status !== 'overwrite');
  const before = await readFile(filename);
  for (const command of ['save','select','rename','delete'])
    assert.equal((await post('profiles',{command,name:'saved',newName:'renamed'})).status,409);
  assert.equal((await fetch(host.url+'/api/profiles',{headers})).status,200);
  assert.deepEqual(await readFile(filename),before);assert.equal(await readFile(join(output,'image.svg'),'utf8'),'preserved');
  await post('batch',{command:'cancel'});
  // Exercise the owner guard against the same real queue lifecycle, too.
  await batch.command({command:'add',filenames:[input]});
  await batch.command({command:'configure',exportDirectory:output,exportFormat:'svg'});await batch.command({command:'start'});
  await assert.rejects(profiles.command({command:'select',name:'saved'}),/Wait for batch/);
  assert.deepEqual(await profiles.command({command:'list'}),{names:['saved',defaultAdvancedProfileName]});
  await batch.command({command:'cancel'});await batch.completion;
  assert.equal(engine.snapshot().document.original.width,0,'preflight cancellation never loads an image');
});
