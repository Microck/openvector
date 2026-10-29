import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,mkdir,readFile,writeFile,rm} from 'node:fs/promises';
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
import {profileFromFrame} from '../../test/helpers/native-gui.mjs';
import {exportSvg} from '../../src/svg.mjs';
import {exportEps} from '../../src/eps.mjs';
import {exportDxf} from '../../src/dxf.mjs';
import {defaultAutoUpdatePreferences} from '../auto-update.mjs';
import {decodeImage} from '../../src/image-input.mjs';
import {readNumericLocale} from '../native-paths.mjs';

const references=gunzipSync(readFileSync(new URL('../../test/fixtures/export-preferences-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const inputs=JSON.parse(readFileSync(new URL('../../test/fixtures/export-preferences-inputs.json',import.meta.url),'utf8'));
const nativeOptions=frame=>({layout:['stacked','cutouts','grouped-cutouts'][frame.layoutMode],compact:frame.epsCompatibility===0,
  thinStroke:frame.strokeMode===0,curveMode:['splines','sparse-lines','dense-lines'][frame.dxfMode]});
const slots={'setCakeStackMode(int)':['layout',['stacked','cutouts','grouped-cutouts']],
  'setEpsCompatibilityMode(int)':['compact',[true,false]],'setShapeStrokingMode(int)':['thinStroke',[true,false]],
  'setDxfMode(int)':['curveMode',['splines','sparse-lines','dense-lines']]};

async function directoryFor(t) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-output-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-output-')));
    await rm(directory,{recursive:true,force:true});
  });return directory;
}

async function owners(directory) {
  const filename=join(directory,'processing-preferences.json'),stored=await readProcessingPreferences(filename);
  const engine=new GuiEngine({profile:stored?.selected,outputOptions:stored?.outputOptions,quickSave:stored?.quickSave,bitmapSave:stored?.bitmapSave,workingDirectory:directory});
  const batch=new BatchController({engine,applicationDirectory:directory,workingDirectory:directory,
    cancellation:new Int32Array(new SharedArrayBuffer(4)),preferences:stored?.batch});
  const profiles=await ProfileController.open({engine,batch,filename:join(directory,'profiles.json')});
  const sources=await SourceController.open({engine,batch,filename:join(directory,'sources.json'),applicationDirectory:directory,workingDirectory:directory});
  return {engine,batch,profiles,application:new ApplicationController({engine,batch,profiles,sources}),filename};
}

for(const reference of references)test('native shared output preferences: '+reference.id,async t=>{
  const directory=await directoryFor(t),local=new Map();
  for(const [filename,encoded] of Object.entries(inputs)) {
    const path=join(directory,win32.basename(win32.dirname(filename)),win32.basename(filename));
    await mkdir(dirname(path),{recursive:true});await writeFile(path,Buffer.from(encoded,'base64'));local.set(filename,path);
  }
  for(const [phase,actions] of reference.phases.entries()) {
    const {engine,batch,profiles,application,filename}=await owners(directory);
    for(const [index,[kind,...values]] of actions.entries()) {
      if(kind==='set') {
        const [receiver,slot,value]=values;assert.equal(receiver,0);const [name,choices]=slots[slot];
        engine.outputOptions({[name]:choices[value]});
      } else if(kind==='batch')await batch.command({command:values[0]?'enter':'leave'});
      else if(kind==='add-files')await batch.command({command:'add',filenames:values[0].map(path=>local.get(path))});
      else if(kind==='reset-application-model')await application.resetModel();
      else if(kind==='persist') {
        const snapshot=batch.snapshot();await writeProcessingPreferences(filename,{selected:engine.selectedProfile(),outputOptions:engine.outputOptions(),autoUpdate:{...defaultAutoUpdatePreferences},skipReview:false,quickSave:engine.quickSavePreferences(),bitmapSave:engine.bitmapSavePreferences(),
          batch:{profile:batch.selectedProfile(),exportDirectory:snapshot.queue.exportDirectory,exportFormat:snapshot.queue.exportFormat}});
      } else assert.equal(kind,'snapshot');
      const native=reference.expected[phase][index],label=`${reference.id} phase ${phase} action ${index}`;
      if(native.exportPreferences) {
        assert.deepEqual(engine.outputOptions(),nativeOptions(native.exportPreferences),label);
        assert.deepEqual(batch.snapshot().exportOptions,nativeOptions(native.exportPreferences),label+' batch projection');
        assert.deepEqual(engine.selectedProfile(),profileFromFrame(native.selected),label+' selected');
        assert.deepEqual(batch.selectedProfile(),profileFromFrame(native.batch),label+' batch profile');
        assert.deepEqual(profiles.names(),native.profiles.map(entry=>entry.name),label+' saved profiles');
      }
      assert.deepEqual(batch.snapshot().queue.queue.map(({filename,buttonIndex})=>({filename,buttonIndex})),
        native.queue.map(({filename,buttonIndex})=>({filename:local.get(filename),buttonIndex})),label+' queue');
    }
  }
});

test('shared output settings validate atomically and own all snapshots',async t=>{
  const directory=await directoryFor(t),{engine,batch}=await owners(directory);
  await batch.command({command:'configure',exportOptions:{layout:'cutouts',compact:true,curveMode:'sparse-lines'}});
  const before=engine.outputOptions();const snapshot=batch.snapshot();snapshot.exportOptions.layout='stacked';
  assert.deepEqual(engine.outputOptions(),before);
  for(const input of [null,[],{format:'svg'},{layout:'invalid'},{thinStroke:1},{compact:'true'},{curveMode:'invalid'}]) {
    assert.throws(()=>engine.outputOptions(input));assert.deepEqual(engine.outputOptions(),before);
  }
  await assert.rejects(batch.command({command:'configure',exportOptions:{layout:'stacked'},exportFormat:'invalid'}));
  assert.deepEqual(engine.outputOptions(),before);
});

test('single-image writer receives the shared selection explicitly and per-call overrides stay local',async t=>{
  const directory=await directoryFor(t),{engine,batch}=await owners(directory);
  engine.open({name:'image.bmp',bytes:Buffer.from(Object.values(inputs)[0],'base64')});engine.automatic({settings:engine.selectedProfile().settings});
  await batch.command({command:'configure',exportOptions:{layout:'cutouts',thinStroke:true,compact:true,curveMode:'sparse-lines'}});
  const options=engine.outputOptions(),state=engine.snapshot().document;
  const common={...options,geometry:state.fitting.geometry,palette:state.segmentation.palette,width:state.original.width,height:state.original.height};
  assert.equal(Buffer.from(engine.export({format:'svg',...options}).bytes).toString(),exportSvg(common));
  assert.equal(Buffer.from(engine.export({format:'eps',...options,creationDate:'test'}).bytes).toString(),exportEps({...common,creationDate:'test'}));
  assert.equal(Buffer.from(engine.export({format:'dxf',...options}).bytes).toString(),exportDxf(common));
  assert.equal(Buffer.from(engine.export({format:'svg',...options,layout:'stacked',thinStroke:false}).bytes).toString(),exportSvg({...common,layout:'stacked',thinStroke:false}));
  assert.deepEqual(engine.outputOptions(),options);
});

test('HTTP output choices survive workspace changes, reset and restart through one owner',async t=>{
  const directory=await directoryFor(t),profileFilename=join(directory,'profiles.json');
  let host=await startGui({port:0,workingDirectory:directory,profileFilename});t.after(()=>host?.close());
  const headers={'Content-Type':'application/json','User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const post=(action,input,extra={})=>fetch(host.url+'/api/'+action,{method:'POST',headers:{...headers,Origin:host.url,...extra},body:JSON.stringify(input)});
  const batch=async()=>{const response=await fetch(host.url+'/api/batch',{headers});assert.equal(response.status,200);return response.json();};
  const options={layout:'cutouts',thinStroke:true,compact:true,curveMode:'sparse-lines'};
  let response=await post('outputOptions',options);assert.equal(response.status,200);assert.deepEqual(await response.json(),options);
  assert.deepEqual((await batch()).exportOptions,options);
  await post('batch',{command:'enter'});response=await post('outputOptions',{layout:'stacked'});assert.equal(response.status,200);
  const shared=await response.json();assert.deepEqual((await batch()).exportOptions,shared);
  const before=await batch();assert.equal((await post('outputOptions',{compact:false},{Origin:'https://example.com'})).status,403);
  assert.equal((await post('outputOptions',{compact:'invalid'})).status,400);assert.deepEqual(await batch(),before);
  await post('resetApplicationModel',null);assert.deepEqual((await batch()).exportOptions,shared);
  await host.close();host=null;
  const stored=JSON.parse(await readFile(join(directory,'processing-preferences.json'),'utf8'));
  assert.deepEqual(stored.outputOptions,shared);assert.equal(Object.hasOwn(stored.batch,'exportOptions'),false);
  host=await startGui({port:0,workingDirectory:directory,profileFilename});assert.deepEqual((await batch()).exportOptions,shared);
});

const bitmapScenes=['bitmap-scenes-reference','bitmap-jpeg-scenes-reference','bitmap-visibility-reference'].flatMap(name=>
  gunzipSync(readFileSync(new URL('../../test/fixtures/'+name+'.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse));
const downloadHeaders={'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};

const viewScaleScenes=gunzipSync(readFileSync(new URL('../../test/fixtures/bitmap-view-scenes-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
for(const reference of viewScaleScenes)test('GUI HTTP view-scale matches native bitmap file: '+reference.id,async t=>{
  const directory=await directoryFor(t),host=await startGui({port:0,workingDirectory:directory,profileFilename:join(directory,'profiles.json')});
  t.after(()=>host.close());
  const post=async(action,input)=>{
    const response=await fetch(host.url+'/api/'+action,{method:'POST',headers:{...downloadHeaders,Origin:host.url,'Content-Type':'application/json'},body:JSON.stringify(input)});
    const value=await response.json();assert.equal(response.status,200,value.error);return value;
  };
  const opened=await post('open',{name:reference.sourceFilename,bytes:reference.sourceBytes});await post('automatic',{settings:opened.settings});
  const state=await post('bitmapSize',{command:'view-scale',mode:'basic',value:reference.scale});assert.equal(state.scale,reference.scale);
  const response=await fetch(host.url+'/api/export?format=png',{headers:downloadHeaders});assert.equal(response.status,200);
  assert.deepEqual(Buffer.from(await response.arrayBuffer()),Buffer.from(reference.expected['result.png'].bytes,'base64'));
});

// Transport integration owns worker serialization, attachment bytes and local
// option lifetime. Core bitmap tests own the underlying raster/codec rules.
for(const reference of bitmapScenes)test('GUI HTTP bitmap matches native files: '+reference.id,async t=>{
  const directory=await directoryFor(t),host=await startGui({port:0,workingDirectory:directory,profileFilename:join(directory,'profiles.json')});
  t.after(()=>host.close());
  const post=async(action,input)=>{
    const response=await fetch(host.url+'/api/'+action,{method:'POST',headers:{...downloadHeaders,'Content-Type':'application/json',Origin:host.url},body:JSON.stringify(input)});
    const value=await response.json();assert.equal(response.status,200,value.error);return value;
  };
  const opened=await post('open',{name:reference.sourceFilename,bytes:reference.sourceBytes});
  await post('automatic',{settings:opened.settings});
  if(reference.actions?.length) {
    await post('background',{action:'begin'});
    for(const [,action,...point] of reference.actions)
      await post('background',action===0?{action:'toggle',point}:{action:['toggle','undo','redo','revert'][action]});
  }
  const state=async()=>{const response=await fetch(host.url+'/api/state',{headers:downloadHeaders});assert.equal(response.status,200);return response.json();};
  const before=await state();
  for(const [filename,expected] of Object.entries(reference.expected)) {
    const format=filename.slice(filename.lastIndexOf('.')+1);
    const response=await fetch(host.url+'/api/export?'+new URLSearchParams({format,scale:reference.scale}),{headers:downloadHeaders});
    assert.equal(response.status,200);
    assert.equal(response.headers.get('Content-Type'),'application/octet-stream');
    assert.equal(response.headers.get('Cache-Control'),'no-store');
    assert.match(response.headers.get('Content-Disposition'),/^attachment; filename\*=UTF-8''/);
    assert.deepEqual(Buffer.from(await response.arrayBuffer()),Buffer.from(expected.bytes,'base64'),reference.id+' '+filename);
  }
  assert.deepEqual(await state(),before);
  const changed=await post('bitmapSize',{command:'commit',mode:'basic',field:'percent',text:String(reference.scale*100)});
  assert.equal(changed.scale,reference.scale);
  const {bitmapSize:beforeSize,...beforeDocument}=before,{bitmapSize:afterSize,...afterDocument}=await state();
  assert.deepEqual(afterDocument,beforeDocument);assert.deepEqual(afterSize.advanced,beforeSize.advanced);
  for(const [filename,expected] of Object.entries(reference.expected)) {
    const format=filename.slice(filename.lastIndexOf('.')+1);
    const response=await fetch(host.url+'/api/export?'+new URLSearchParams({format}),{headers:downloadHeaders});
    assert.equal(response.status,200);
    assert.deepEqual(Buffer.from(await response.arrayBuffer()),Buffer.from(expected.bytes,'base64'),reference.id+' shared size '+filename);
  }
});

test('Windows numeric locale matches the recorded native Qt system context',async()=>{
  const [reference]=gunzipSync(readFileSync(new URL('../../test/fixtures/qt-locale-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const symbols=await readNumericLocale();
  for(const name of ['decimal','group','zero','negative','exponent'])assert.equal(symbols[name].charCodeAt(0),reference.locales.system[name]);
  assert.equal(symbols.positive,'+');
});

test('HTTP bitmap sizing validates commands, respects workspace locks and preserves fields on close',async t=>{
  const directory=await directoryFor(t),host=await startGui({port:0,workingDirectory:directory,profileFilename:join(directory,'profiles.json')});
  t.after(()=>host.close());
  const post=(action,input,origin=host.url)=>fetch(host.url+'/api/'+action,{method:'POST',headers:{...downloadHeaders,'Content-Type':'application/json',Origin:origin},body:JSON.stringify(input)});
  const size=async()=>{const response=await fetch(host.url+'/api/bitmapSize',{headers:downloadHeaders});assert.equal(response.status,200);return response.json();};
  assert.equal((await size()).scale,1);
  assert.equal((await post('bitmapSize',{command:'commit',mode:'basic',field:'percent',text:'150%'})).status,400);
  const reference=bitmapScenes[0];let response=await post('open',{name:reference.sourceFilename,bytes:reference.sourceBytes});assert.equal(response.status,200);
  const opened=await response.json();await post('automatic',{settings:opened.settings});
  response=await post('bitmapSize',{command:'commit',mode:'advanced',field:'percent',text:'150,0%'});assert.equal(response.status,200);
  assert.equal((await response.json()).scale,1.5);
  response=await post('bitmapSize',{command:'preset',mode:'basic',value:4});assert.equal(response.status,200);
  const preset=await response.json();assert.deepEqual(preset.basic,{width:64,height:48,percent:'400.0'});
  assert.deepEqual(preset.advanced,{width:24,height:18,percent:'150.0'});
  response=await post('bitmapSize',{command:'view-scale',mode:'basic',value:2.5});assert.equal(response.status,200);
  assert.equal((await response.json()).scale,2.5);const before=await size();
  assert.equal((await post('bitmapSize',{command:'step',mode:'advanced',field:'width',direction:1,size:'small'},'https://example.com')).status,403);
  for(const input of [null,{command:'commit',mode:'basic',field:'percent',text:1},
    {command:'commit',mode:'basic',field:'percent',text:'100',value:1},{command:'step',mode:'bad',field:'width',direction:1,size:'small'},
    {command:'preset',mode:'basic',value:3},{command:'view-scale',mode:'basic',value:0}])
    assert.equal((await post('bitmapSize',input)).status,400);
  assert.deepEqual(await size(),before);
  await post('batch',{command:'enter'});
  assert.equal((await post('bitmapSize',{command:'commit',mode:'basic',field:'width',text:'20'})).status,409);
  assert.deepEqual(await size(),before);await post('batch',{command:'leave'});
  await post('reset',null);assert.deepEqual(await size(),before);
});

test('GUI HTTP bitmap confirmation is coded, explicit and leaves rejected exports without bytes or state changes',async t=>{
  const directory=await directoryFor(t),host=await startGui({port:0,workingDirectory:directory,profileFilename:join(directory,'profiles.json')});
  t.after(()=>host.close());
  const post=(action,input)=>fetch(host.url+'/api/'+action,{method:'POST',headers:{...downloadHeaders,'Content-Type':'application/json',Origin:host.url},body:JSON.stringify(input)});
  const reference=bitmapScenes.find(row=>row.id==='alpha-png');
  let response=await post('open',{name:'multi.part.png',bytes:reference.sourceBytes});assert.equal(response.status,200);
  const opened=await response.json();response=await post('automatic',{settings:opened.settings});assert.equal(response.status,200);
  const lightweightBefore=await response.json();
  const before=await (await fetch(host.url+'/api/state',{headers:downloadHeaders})).json();
  for(const format of ['bmp','jpg','jpeg','tif','tiff']) {
    response=await fetch(host.url+'/api/export?'+new URLSearchParams({format}),{headers:downloadHeaders});
    assert.equal(response.status,400);assert.equal(response.headers.get('Content-Disposition'),null);
    const rejected=await response.json();assert.equal(rejected.code,'BITMAP_FLATTEN_REQUIRED');assert.deepEqual(rejected.state,lightweightBefore);
    response=await fetch(host.url+'/api/export?'+new URLSearchParams({format,flatten:true}),{headers:downloadHeaders});
    assert.equal(response.status,200);assert.equal(response.headers.get('Content-Disposition'),`attachment; filename*=UTF-8''multi.${format}`);
    const decoded=decodeImage(Buffer.from(await response.arrayBuffer()));
    assert.ok(decoded.bgra.every((channel,index)=>index%4!==3 || channel===255));
  }
  for(const query of ['format=png&flatten=true','format=png&flatten=1','format=png&scale=0','format=png&scale=-1',
    'format=png&scale=null','format=png&scale=%22two%22','format=png&scale=Infinity','format=svg&scale=2','format=svg&flatten=false','format=png&layout=cutouts']) {
    response=await fetch(host.url+'/api/export?'+query,{headers:downloadHeaders});assert.equal(response.status,400,query);
    assert.equal(response.headers.get('Content-Disposition'),null);assert.ok((await response.json()).error);
  }
  response=await fetch(host.url+'/api/export?format=png',{headers:{...downloadHeaders,Origin:'https://example.com'}});assert.equal(response.status,403);
  assert.deepEqual(await (await fetch(host.url+'/api/state',{headers:downloadHeaders})).json(),before);
  await post('batch',{command:'enter'});
  response=await fetch(host.url+'/api/export?format=png',{headers:downloadHeaders});assert.equal(response.status,409);
});

const bitmapControls=gunzipSync(readFileSync(new URL('../../test/fixtures/bitmap-controls-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
for(const reference of bitmapControls.filter(row=>['wide','tall','multi-name'].includes(row.id)))
  test('GUI bitmap default filename matches native field: '+reference.id,()=>{
    const filename=reference.actions[0][1],engine=new GuiEngine();
    const opened=engine.open({name:filename,bytes:Buffer.from(bitmapScenes[0].sourceBytes,'base64')});engine.automatic({settings:opened.settings});
    const fields=reference.expected.at(-1).bitmapControls.map(widget=>widget.controls.find(control=>control.kind===1 && control.name==='').text);
    assert.equal(fields[0],fields[1]);assert.equal(engine.export({format:'png'}).filename,fields[0]+'.png');
    assert.equal(engine.export({format:'svg'}).filename,win32.basename(filename).replace(/\.[^.]+$/,'')+'.svg');
  });
