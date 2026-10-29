import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,writeFile,readFile,rm,stat} from 'node:fs/promises';
import {dirname,join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {GuiEngine} from '../engine.mjs';
import {QuickSaveFile} from '../quick-save-file.mjs';
import {quickSaveFormats} from '../quick-save.mjs';
import {readEmfDevice} from '../native-paths.mjs';
import {startGui} from '../server.mjs';
import {gunzipSync} from 'node:zlib';

const encoded=Object.values(JSON.parse(await readFile(new URL('../../test/fixtures/review-actions-inputs.json',import.meta.url),'utf8')))[0];
const bitmap=Buffer.from(encoded,'base64');
const bitmapReferences=(await Promise.all(['bitmap-scenes-reference','bitmap-jpeg-scenes-reference'].map(async name=>
  gunzipSync(await readFile(new URL('../../test/fixtures/'+name+'.ndjson.gz',import.meta.url))).toString().trim().split('\n').map(JSON.parse)))).flat();
async function directoryFor(t) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-direct-save-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-direct-save-')));
    await rm(directory,{recursive:true,force:true});
  });return directory;
}
function vectorized(directory,name=join(directory,'source.bmp')) {
  const engine=new GuiEngine({workingDirectory:directory}),opened=engine.open({name,bytes:bitmap});
  engine.automatic({settings:opened.settings});return engine;
}

for(const [index,format] of ['bmp','jpg','png','tif'].entries())test('direct bitmap save writes native file bytes: '+format,async t=>{
  const directory=await directoryFor(t),reference=bitmapReferences.find(row=>Object.keys(row.expected).some(name=>name.endsWith('.'+format)));
  const engine=new GuiEngine({workingDirectory:directory}),opened=engine.open({name:reference.sourceFilename,bytes:Buffer.from(reference.sourceBytes,'base64')});
  engine.automatic({settings:opened.settings});const mode=index%2?'advanced':'basic';
  engine.bitmapSize({command:'commit',mode,field:'scale',value:reference.scale});
  engine.bitmapSave({mode,format,directory,filename:'native-'+format});
  const before=engine.state(),writer=new QuickSaveFile(engine);
  const saved=await writer.command({command:'save',kind:'bitmap',mode});
  assert.equal(saved.filename,join(directory,'native-'+format+'.'+format));assert.equal(saved.status,'saved');
  const expected=Object.entries(reference.expected).find(([name])=>name.endsWith('.'+format))[1];
  assert.deepEqual(await readFile(saved.filename),Buffer.from(expected.bytes,'base64'));assert.deepEqual(engine.state(),before);
});

test('bitmap directory and overwrite decisions precede its OK-only warning and freeze the output',async t=>{
  const directory=await directoryFor(t),reference=bitmapReferences.find(row=>row.id==='alpha-png');
  const engine=new GuiEngine({workingDirectory:directory}),opened=engine.open({name:reference.sourceFilename,bytes:Buffer.from(reference.sourceBytes,'base64')});
  engine.automatic({settings:opened.settings});const writer=new QuickSaveFile(engine),folder=join(directory,'missing','nested'),filename=join(folder,'alpha.jpg');
  engine.bitmapSave({mode:'basic',format:'jpg',directory:folder,filename:'alpha'});
  assert.equal((await writer.command({command:'save',kind:'bitmap',mode:'basic'})).stage,'directory');
  await assert.rejects(stat(folder),{code:'ENOENT'});
  assert.equal((await writer.command({command:'decide',accepted:false})).status,'cancelled');
  await assert.rejects(stat(folder),{code:'ENOENT'});
  await writer.command({command:'save',kind:'bitmap',mode:'basic'});
  // A real target appearing while the directory question is open must still
  // go through the overwrite question before the bitmap receiver's warning.
  await mkdir(folder,{recursive:true});await writeFile(filename,'keep');
  assert.equal((await writer.command({command:'decide',accepted:true})).stage,'overwrite');
  assert.equal((await writer.command({command:'decide',accepted:false})).status,'cancelled');
  assert.equal(await readFile(filename,'utf8'),'keep');
  assert.equal((await writer.command({command:'save',kind:'bitmap',mode:'basic'})).stage,'overwrite');
  assert.equal((await writer.command({command:'decide',accepted:true})).stage,'flatten');
  for(const input of [{command:'decide',accepted:false},{command:'decide',accepted:true},{command:'acknowledge',accepted:true}]) {
    await assert.rejects(writer.command(input));assert.equal(writer.pending,true);assert.equal(await readFile(filename,'utf8'),'keep');
  }
  const expected=Buffer.from(engine.export({format:'jpg',flatten:true}).bytes);
  // The worker normally prohibits this mutation. The transaction's owned
  // bytes also remain stable when its engine is used directly by a caller.
  engine.bitmapSize({command:'commit',mode:'basic',field:'scale',value:2});
  assert.equal((await writer.command({command:'acknowledge'})).status,'saved');assert.equal(writer.pending,false);
  assert.deepEqual(await readFile(filename),expected);
  await assert.rejects(writer.command({command:'acknowledge'}),/no transparency warning/);
});

test('bitmap direct save protects its source and rejects incomplete save-kind commands',async t=>{
  const directory=await directoryFor(t),filename=join(directory,'original.png');await writeFile(filename,bitmap);
  const engine=vectorized(directory,filename),writer=new QuickSaveFile(engine);
  engine.bitmapSave({mode:'basic',format:'png',filename});
  await assert.rejects(writer.command({command:'save',kind:'bitmap',mode:'basic'}),/source file/);
  assert.deepEqual(await readFile(filename),bitmap);assert.equal(writer.pending,false);
  for(const input of [{command:'save',mode:'basic'},{command:'save',kind:'wrong',mode:'basic'},
    {command:'save',kind:'bitmap',mode:'basic',flatten:true},{command:'save',kind:'bitmap',mode:'basic',referenceDevice:{}}])
    await assert.rejects(writer.command(input));
  engine.bitmapSave({mode:'basic',filename:'recovered'});
  assert.equal((await writer.command({command:'save',kind:'bitmap',mode:'basic'})).status,'saved');
});

test('direct Quick Save writes all six existing exporters to the chosen folder',async t=>{
  const directory=await directoryFor(t),engine=vectorized(directory),writer=new QuickSaveFile(engine);
  const referenceDevice=await readEmfDevice();
  engine.outputOptions({layout:'stacked',thinStroke:true,compact:true,curveMode:'sparse-lines'});
  for(const format of quickSaveFormats) {
    engine.quickSave({mode:'basic',format,filename:'vector-'+format});
    const saved=await writer.command({command:'save',kind:'vector',mode:'basic',referenceDevice});
    assert.equal(saved.status,'saved');assert.equal(writer.pending,false);
    assert.equal(saved.filename,join(directory,'vector-'+format+'.'+format));
    const actual=await readFile(saved.filename);
    assert.equal(actual.length,saved.bytes);
    assert.deepEqual(actual,Buffer.from(engine.export({format,...engine.outputOptions(),referenceDevice}).bytes));
  }
});

test('direct Quick Save freezes pending bytes, cancels cleanly and handles real filesystem errors',async t=>{
  const directory=await directoryFor(t),engine=vectorized(directory),writer=new QuickSaveFile(engine);
  const folder=join(directory,'missing','nested');
  engine.quickSave({mode:'advanced',format:'svg',directory:folder,filename:'sp ace_日本'});
  let outcome=await writer.command({command:'save',kind:'vector',mode:'advanced'});
  assert.deepEqual(outcome,{status:'confirm',stage:'directory',filename:join(folder,'sp ace_日本.svg')});
  assert.equal(writer.pending,true);await assert.rejects(stat(folder),{code:'ENOENT'});
  await assert.rejects(writer.command({command:'save',kind:'vector',mode:'basic'}),/pending/);
  await assert.rejects(writer.command({command:'decide',accepted:'yes'}),/Yes or No/);
  assert.deepEqual(await writer.command({command:'decide',accepted:false}),{status:'cancelled'});
  await assert.rejects(stat(folder),{code:'ENOENT'});
  await writer.command({command:'save',kind:'vector',mode:'advanced'});
  outcome=await writer.command({command:'decide',accepted:true});assert.equal(outcome.status,'saved');
  const firstBytes=await readFile(outcome.filename);await writeFile(outcome.filename,'keep existing');
  assert.equal((await writer.command({command:'save',kind:'vector',mode:'advanced'})).stage,'overwrite');
  await writer.command({command:'decide',accepted:false});assert.equal(await readFile(outcome.filename,'utf8'),'keep existing');
  await writer.command({command:'save',kind:'vector',mode:'advanced'});
  await writer.command({command:'decide',accepted:true});assert.deepEqual(await readFile(outcome.filename),firstBytes);
  engine.quickSave({mode:'advanced',filename:''});await assert.rejects(writer.command({command:'save',kind:'vector',mode:'advanced'}),/Enter a filename/);
  assert.equal(writer.pending,false);
  // An actual directory at the target name produces an OS write error.
  engine.quickSave({mode:'advanced',filename:'directory-target'});await mkdir(join(folder,'directory-target.svg'));
  assert.equal((await writer.command({command:'save',kind:'vector',mode:'advanced'})).stage,'overwrite');
  await assert.rejects(writer.command({command:'decide',accepted:true}));assert.equal(writer.pending,false);
  engine.quickSave({mode:'advanced',filename:'recovered'});assert.equal((await writer.command({command:'save',kind:'vector',mode:'advanced'})).status,'saved');
});

test('direct Quick Save protects the actual source even when its suffix selects a vector format',async t=>{
  const directory=await directoryFor(t),filename=join(directory,'original.svg');await writeFile(filename,bitmap);
  const engine=vectorized(directory,filename),writer=new QuickSaveFile(engine);
  assert.equal(engine.quickSave().basic.format,'svg');
  await assert.rejects(writer.command({command:'save',kind:'vector',mode:'basic'}),/source file/);
  assert.deepEqual(await readFile(filename),bitmap);assert.equal(writer.pending,false);
});

for(const kind of ['vector','bitmap'])test('HTTP '+kind+' direct save serializes decisions against document, settings and queue mutations',async t=>{
  const directory=await directoryFor(t),host=await startGui({port:0,workingDirectory:directory,profileFilename:join(directory,'profiles.json')});
  t.after(()=>host.close());
  const post=async(action,input,status=200)=>{
    const response=await fetch(host.url+'/api/'+action,{method:'POST',headers:{Origin:host.url,'Content-Type':'application/json','User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'},body:JSON.stringify(input)});
    const value=await response.json();assert.equal(response.status,status,JSON.stringify(value));return value;
  };
  const opened=await post('open',{name:'source.bmp',bytes:encoded});await post('automatic',{settings:opened.settings});
  await post(kind==='bitmap'?'bitmapSave':'quickSave',{mode:'basic',format:kind==='bitmap'?'png':'svg',directory:join(directory,'missing')});
  assert.equal((await post('quickSaveFile',{command:'save',kind,mode:'basic'})).stage,'directory');
  for(const [action,input] of [['reset',null],['quickSave',{mode:'basic',filename:'changed'}],['batch',{command:'enter'}],['outputOptions',{thinStroke:true}],['bitmapSave',{mode:'basic',filename:'changed'}],['bitmapSize',{command:'step',mode:'basic',field:'width',direction:1,size:'small'}]])
    assert.match((await post(action,input,400)).error,/pending Quick Save/);
  const choices=await fetch(host.url+'/api/quickSave',{headers:{'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'}});assert.equal(choices.status,200);
  assert.equal((await post('quickSaveFile',{command:'decide',accepted:false})).status,'cancelled');
  await post('quickSaveFile',{command:'decide',accepted:true},400);
  await post('quickSaveFile',{command:'save',kind,mode:'basic',referenceDevice:{}},400);
  await post('quickSaveFile',{command:'save',kind,mode:'basic'});
  const saved=await post('quickSaveFile',{command:'decide',accepted:true});assert.equal(saved.status,'saved');assert.ok((await stat(saved.filename)).size>0);
  assert.equal((await post('quickSaveFile',{command:'save',kind,mode:'basic'})).stage,'overwrite');
  await post('quickSaveFile',{command:'decide',accepted:false});
  await post('reset',null);
});

test('HTTP bitmap warning keeps one pending save across kinds until OK is acknowledged',async t=>{
  const directory=await directoryFor(t),host=await startGui({port:0,workingDirectory:directory,profileFilename:join(directory,'profiles.json')});
  t.after(()=>host.close());const headers={'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const post=async(action,input,status=200)=>{
    const response=await fetch(host.url+'/api/'+action,{method:'POST',headers:{...headers,Origin:host.url,'Content-Type':'application/json'},body:JSON.stringify(input)});
    const value=await response.json();assert.equal(response.status,status,JSON.stringify(value));return value;
  };
  const reference=bitmapReferences.find(row=>row.id==='alpha-png');
  const opened=await post('open',{name:reference.sourceFilename,bytes:reference.sourceBytes});await post('automatic',{settings:opened.settings});
  await post('quickSaveDirectory',{mode:'basic'},400);await post('quickSaveDirectory',{kind:'wrong',mode:'basic'},400);
  await post('bitmapSave',{mode:'advanced',format:'jpg',directory,filename:'alpha'});
  const filename=join(directory,'alpha.jpg');await writeFile(filename,'keep');
  const before=await (await fetch(host.url+'/api/state',{headers})).json();
  assert.equal((await post('quickSaveFile',{command:'save',kind:'bitmap',mode:'advanced'})).stage,'overwrite');
  assert.equal((await post('quickSaveFile',{command:'decide',accepted:true})).stage,'flatten');
  await post('quickSaveFile',{command:'decide',accepted:false},400);
  await post('quickSaveFile',{command:'save',kind:'vector',mode:'basic'},400);
  await post('quickSaveDirectory',{kind:'bitmap',mode:'advanced'},400);
  await post('bitmapSave',{mode:'advanced',filename:'changed'},400);
  for(const action of ['bitmapSave','bitmapSize','quickSave','state']) {
    const response=await fetch(host.url+'/api/'+action,{headers});assert.equal(response.status,200);await response.json();
  }
  const rejectedDownload=await fetch(host.url+'/api/export?format=png',{headers});assert.equal(rejectedDownload.status,400);
  assert.equal(rejectedDownload.headers.get('Content-Disposition'),null);await rejectedDownload.json();
  assert.equal(await readFile(filename,'utf8'),'keep');
  const saved=await post('quickSaveFile',{command:'acknowledge'});assert.equal(saved.status,'saved');assert.equal(saved.filename,filename);
  const download=await fetch(host.url+'/api/export?format=jpg&flatten=true',{headers});assert.equal(download.status,200);
  assert.deepEqual(await readFile(filename),Buffer.from(await download.arrayBuffer()));
  assert.deepEqual(await (await fetch(host.url+'/api/state',{headers})).json(),before);
  await post('reset',null);
});
