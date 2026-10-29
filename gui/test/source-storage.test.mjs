import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,mkdir,writeFile,readFile,rm,readdir,rmdir} from 'node:fs/promises';
import {join,dirname,resolve,win32} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {SourceController} from '../source-controller.mjs';
import {GuiEngine} from '../engine.mjs';
import {BatchController} from '../batch-controller.mjs';
import {decodeImage} from '../../src/image-input.mjs';
import {startGui} from '../server.mjs';

const references=['source-storage','source-thumbnails'].flatMap(name=>
  gunzipSync(readFileSync(new URL('../../test/fixtures/'+name+'-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse));

async function context(t) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-source-storage-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-source-storage-')));
    await rm(directory,{recursive:true,force:true});
  });
  const applicationDirectory=join(directory,'app'),workingDirectory=join(directory,'inputs'),filename=join(directory,'sources.json');
  await mkdir(applicationDirectory);await mkdir(workingDirectory);
  const open=async()=>{
    const engine=new GuiEngine(),batch=new BatchController({engine,applicationDirectory,workingDirectory,
      cancellation:new Int32Array(new SharedArrayBuffer(4))});
    return {engine,batch,sources:await SourceController.open({engine,batch,filename,applicationDirectory,workingDirectory})};
  };
  return {directory,applicationDirectory,workingDirectory,filename,open};
}

function nativePixels(value) {
  const variant=Buffer.from(Buffer.from(value.value,'hex').toString('utf16le').slice(9,-1),'latin1');
  const start=variant.indexOf(Buffer.from([137,80,78,71,13,10,26,10]));
  if(start<0)return null;
  const image=decodeImage(variant.subarray(start)),rgba=Buffer.alloc(image.bgra.length);
  for(let pixel=0;pixel<rgba.length;pixel+=4)for(let channel=0;channel<4;channel++)
    rgba[pixel+channel]=image.bgra[pixel+[2,1,0,3][channel]];
  return {width:image.width,height:image.height,rgba:rgba.toString('base64')};
}

for(const reference of references)test('source history across native restarts: '+reference.id,async t=>{
  const setup=await context(t),filename=original=>join(setup.workingDirectory,win32.basename(original));
  for(const [original,bytes] of Object.entries(reference.inputs))await writeFile(filename(original),Buffer.from(bytes,'base64'));
  const compare=(sources,native)=>{
    const actual=sources.snapshot();assert.equal(actual.capacity,native.capacity);
    assert.equal(actual.directory,setup.workingDirectory.replaceAll('\\','/'));
    assert.deepEqual(actual.files.map(({thumbnail,...entry})=>entry),native.files.map(entry=>({...entry,filename:filename(entry.filename)})));
  };
  const comparePixels=(sources,native,values)=>{
    compare(sources,native);
    for(const [index,entry] of sources.snapshot().files.entries()) {
      const native=nativePixels(values['/VmQt::MainModel/mru_file_icon_'+index]);
      if(native===null)assert.equal(entry.thumbnail,null);
      else {
        assert.deepEqual([entry.thumbnail.width,entry.thumbnail.height],[native.width,native.height]);
        assert.deepEqual(Buffer.from(entry.thumbnail.rgba,'base64'),Buffer.from(native.rgba,'base64'),'cached thumbnail '+index);
      }
    }
  };
  for(const [phase,actions] of reference.phases.entries()) {
    const {engine,sources}=await setup.open();assert.equal(engine.hasSource,false,'restart does not reopen the source');
    if(phase>0)comparePixels(sources,reference.expected[phase][0].sourceHistory,reference.privateValues[phase-1]);
    for(const [index,action] of actions.entries()) {
      const [kind,value]=action;
      if(kind==='open' || kind==='open-wide')await sources.command({command:'open',filename:filename(value)});
      else if(kind==='set') {assert.equal(action[2],'clearMruFiles()');await sources.command({command:'clear'});}
      else assert.ok(['snapshot','persist'].includes(kind),kind);
      const native=reference.expected[phase][index].sourceHistory;
      if(native)compare(sources,native);
    }
    const native=reference.expected[phase].findLast(frame=>frame.sourceHistory).sourceHistory;
    comparePixels(sources,native,reference.privateValues[phase]);
    for(const transition of reference.transitions)if(transition.afterPhase===phase) {
      if(transition.bytes===null)await rm(filename(transition.filename));
      else await writeFile(filename(transition.filename),Buffer.from(transition.bytes,'base64'));
    }
  }
});

test('source loading preserves the current image and history when reading or decoding fails',async t=>{
  const setup=await context(t),{engine,sources}=await setup.open(),reference=references[0];
  const bytes=Buffer.from(Object.values(reference.inputs)[0],'base64');
  await writeFile(join(setup.workingDirectory,'a.bmp'),bytes);
  await sources.command({command:'open',filename:'a.bmp'});
  const before=engine.state(true),history=sources.snapshot(),stored=await readFile(setup.filename);
  await assert.rejects(sources.command({command:'open',filename:'missing.bmp'}),{code:'ENOENT'});
  await writeFile(join(setup.workingDirectory,'bad.bmp'),'not an image');
  await assert.rejects(sources.command({command:'open',filename:'bad.bmp'}));
  assert.deepEqual(engine.state(true),before);assert.deepEqual(sources.snapshot(),history);
  assert.deepEqual(await readFile(setup.filename),stored);
  await assert.rejects(sources.command({command:'reopen',index:5}),/Recent image not found/);
});

test('a source-history write failure exposes the loaded session and preserves the blocking filesystem entry',async t=>{
  const setup=await context(t),{engine,sources}=await setup.open(),bytes=Buffer.from(Object.values(references[0].inputs)[0],'base64');
  await writeFile(join(setup.workingDirectory,'a.bmp'),bytes);
  await mkdir(setup.filename);await writeFile(join(setup.filename,'keep.txt'),'keep');
  await assert.rejects(sources.command({command:'open',filename:'a.bmp'}));
  assert.equal(engine.hasSource,true);assert.equal(sources.snapshot().files.length,1);
  assert.equal(await readFile(join(setup.filename,'keep.txt'),'utf8'),'keep');
  assert.deepEqual((await readdir(setup.directory)).sort(),['app','inputs','sources.json']);
});

test('source storage rejects corrupt metadata and pixel payloads without replacing the file',async t=>{
  const setup=await context(t),{sources}=await setup.open(),bytes=Buffer.from(Object.values(references[0].inputs)[0],'base64');
  await writeFile(join(setup.workingDirectory,'a.bmp'),bytes);await sources.command({command:'open',filename:'a.bmp'});
  const original=JSON.parse(await readFile(setup.filename,'utf8'));
  for(const change of [state=>{state.files[0].rgba='bad';},state=>{state.files.push({...state.files[0]});},state=>{state.files[0].thumbnailDimensions=[0,1];}]) {
    const state=structuredClone(original);change(state);const encoded=JSON.stringify(state);await writeFile(setup.filename,encoded);
    await assert.rejects(setup.open());assert.equal(await readFile(setup.filename,'utf8'),encoded);
  }
});

test('HTTP real-path sources persist through worker restart and byte uploads do not create disk history',async t=>{
  const setup=await context(t),options={port:0,workingDirectory:setup.workingDirectory,
    profileFilename:join(setup.directory,'profiles.json'),sourceFilename:setup.filename};
  let host=await startGui(options);t.after(()=>host.close());
  const headers={'Content-Type':'application/json','User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const get=async action=>{const response=await fetch(host.url+'/api/'+action,{headers});assert.equal(response.status,200);return response.json();};
  const post=(action,input,extra={})=>fetch(host.url+'/api/'+action,{method:'POST',headers:{...headers,Origin:host.url,...extra},body:JSON.stringify(input)});
  assert.deepEqual((await get('sources')).files,[]);
  const bytes=Object.values(references[0].inputs)[0],filename=join(setup.workingDirectory,'naïve image.bmp');
  await writeFile(filename,Buffer.from(bytes,'base64'));
  let response=await post('sources',{command:'open',filename:'naïve image.bmp'});assert.equal(response.status,200);
  const opened=await response.json();assert.equal(opened.history.files[0].filename,filename);
  assert.ok(opened.document.bitmap);assert.equal(opened.document.file.name,'naïve image.bmp');
  assert.equal((await post('sources',{command:'clear'},{Origin:'https://example.com'})).status,403);
  assert.deepEqual(await get('sources'),opened.history);
  response=await post('open',{name:'upload.bmp',bytes});assert.equal(response.status,200);
  assert.deepEqual(await get('sources'),opened.history,'an uploaded basename is not a real source path');
  await rm(filename);await host.close();host=await startGui(options);
  assert.equal(await get('state'),null);assert.deepEqual(await get('sources'),opened.history,'missing file retains cached metadata and pixels');
  response=await post('sources',{command:'reopen',index:0});assert.equal(response.status,400);
  assert.equal(Object.hasOwn(await response.json(),'sourceResult'),false,'a failed read did not load a source');
  assert.deepEqual(await get('sources'),opened.history);assert.equal(await get('state'),null);
  assert.equal((await post('batch',{command:'enter'})).status,200);
  assert.deepEqual(await get('sources'),opened.history,'history remains readable in batch mode');
  assert.equal((await post('sources',{command:'clear'})).status,409);
  assert.equal((await post('batch',{command:'leave'})).status,200);
  assert.equal((await post('sources',{command:'clear'})).status,200);
  await host.close();host=await startGui(options);assert.deepEqual((await get('sources')).files,[]);
});

test('HTTP source-history save errors return the actual loaded bitmap and leave storage intact',async t=>{
  const setup=await context(t),filename=join(setup.workingDirectory,'a.bmp');
  await writeFile(filename,Buffer.from(Object.values(references[0].inputs)[0],'base64'));
  const host=await startGui({port:0,workingDirectory:setup.workingDirectory,
    profileFilename:join(setup.directory,'profiles.json'),sourceFilename:setup.filename});t.after(()=>host.close());
  await mkdir(setup.filename);await writeFile(join(setup.filename,'keep.txt'),'keep');
  const headers={'Content-Type':'application/json',Origin:host.url,'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const response=await fetch(host.url+'/api/sources',{method:'POST',headers,body:JSON.stringify({command:'open',filename})});
  assert.equal(response.status,400);const failed=await response.json();assert.ok(failed.error);
  assert.equal(failed.state.file.name,'a.bmp');assert.ok(failed.state.bitmap.rgba);
  assert.equal(failed.sourceResult.loaded,true);assert.equal(failed.sourceResult.history.files[0].filename,filename);
  const current=await (await fetch(host.url+'/api/state',{headers})).json();assert.deepEqual(failed.state,current);
  const history=await (await fetch(host.url+'/api/sources',{headers})).json();assert.equal(history.files[0].filename,filename);
  assert.equal(await readFile(join(setup.filename,'keep.txt'),'utf8'),'keep');
});

const directoryReferences=gunzipSync(readFileSync(new URL('../../test/fixtures/source-directories-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
for(const reference of directoryReferences)test('disk directory history matches native across restart: '+reference.id,async t=>{
  const setup=await context(t),nativeBase='D:/migracion/vector-magic-re/evidence/source-directories-inputs';
  // Translate only the owned fixture root. Keep case and Unicode below it.
  const filename=path=>join(setup.workingDirectory,win32.relative(nativeBase,path));
  for(const [original,bytes] of Object.entries(reference.inputs)) {
    const path=filename(original);await mkdir(dirname(path),{recursive:true});await writeFile(path,Buffer.from(bytes,'base64'));
  }
  for(const [phase,actions] of reference.phases.entries()) {
    const {engine,sources}=await setup.open();assert.equal(engine.hasSource,false);
    for(const [index,[kind,value]] of actions.entries()) {
      if(kind==='open' || kind==='open-wide')await sources.command({command:'open',filename:filename(value)});
      else if(kind==='set') {assert.equal(actions[index][2],'clearMruFiles()');await sources.command({command:'clear'});}
      else assert.ok(['snapshot','persist'].includes(kind),kind);
      const native=reference.expected[phase][index];
      if(native.sourceDirectories) {
        assert.deepEqual(sources.snapshot().directoryHistory,{capacity:native.sourceDirectories.capacity,
          directories:native.sourceDirectories.directories.map(path=>filename(path).replaceAll('\\','/'))});
        assert.equal(sources.snapshot().files.length,native.sourceHistory.files.length);
      } else assert.equal(kind,'set','only the generic slot frame lacks the directory observer');
    }
    if(reference.removeDirectoryAfterPhase?.phase===phase) {
      const path=filename(reference.removeDirectoryAfterPhase.filename);
      assert.ok(path.startsWith(setup.workingDirectory+'\\'));await rm(path);await rmdir(dirname(path));
    }
  }
});

test('source storage rejects the old local format instead of inventing a directory list',async t=>{
  const setup=await context(t),stored=JSON.stringify({directory:setup.workingDirectory,files:[]});
  await writeFile(setup.filename,stored);
  await assert.rejects(setup.open(),/Invalid source-history format/);
  assert.equal(await readFile(setup.filename,'utf8'),stored);
});

test('HTTP directory actions validate cached indexes and missing folders without opening native UI',async t=>{
  const setup=await context(t),filename=join(setup.workingDirectory,'a.bmp');
  await writeFile(filename,Buffer.from(Object.values(references[0].inputs)[0],'base64'));
  const host=await startGui({port:0,workingDirectory:setup.workingDirectory,
    sourceFilename:setup.filename,profileFilename:join(setup.directory,'profiles.json')});t.after(()=>host.close());
  const headers={'Content-Type':'application/json',Origin:host.url,'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const post=input=>fetch(host.url+'/api/sources',{method:'POST',headers,body:JSON.stringify(input)});
  let response=await post({command:'open',filename});assert.equal(response.status,200);
  const loaded=await response.json();assert.deepEqual(loaded.history.directoryHistory,
    {capacity:4,directories:[setup.workingDirectory.replaceAll('\\','/')]});
  for(const command of ['pick-directory','reveal-directory'])for(const index of [-1,1,'0',null]) {
    response=await post({command,index});assert.equal(response.status,400);
    assert.equal((await response.json()).error,'Recent directory not found.');
  }
  await rm(filename);await rmdir(setup.workingDirectory);
  response=await post({command:'reveal-directory',index:0});assert.equal(response.status,400);
  assert.equal((await response.json()).error,'The folder does not exist.');
  assert.deepEqual(await (await fetch(host.url+'/api/sources',{headers})).json(),loaded.history);
  assert.equal((await fetch(host.url+'/api/sources',{method:'POST',headers:{...headers,Origin:'https://example.com'},
    body:JSON.stringify({command:'reveal-directory',index:0})})).status,403);
});
