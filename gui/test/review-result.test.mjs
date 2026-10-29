import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {profileFromFrame,compareDocumentState} from '../../test/helpers/native-gui.mjs';
import {startIsolatedGui} from './gui-host.mjs';

const references=['review-actions','review-alpha'].flatMap(name=>
  gunzipSync(readFileSync(new URL(`../../test/fixtures/${name}-reference.ndjson.gz`,import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse));
const inputs=Object.assign({},...['review-actions','review-alpha'].map(name=>
  JSON.parse(readFileSync(new URL(`../../test/fixtures/${name}-inputs.json`,import.meta.url),'utf8'))));
const parameters=JSON.parse(readFileSync(new URL('../../test/fixtures/processing-reset-parameters.json',import.meta.url),'utf8'));
const actions=['artwork','high','medium','low','custom-colors','unlimited-colors','edit','hand-pick','background'];
const pages={review:19,palette:11,editing:14,type:5,background:15};

for(const reference of references)test('native Review Result action: '+reference.id,()=>{
  const engine=new GuiEngine();let page=0;
  for(const [index,[kind,...values]] of reference.phases[0].entries()) {
    const expected=reference.expected[0][index],label=reference.id+' action '+index;
    if(kind==='automatic') {
      engine.open({name:values[0],bytes:Buffer.from(inputs[values[0]],'base64')});
      engine.automatic({settings:engine.selectedProfile().settings});page=19;
    } else if(kind==='review-action') page=pages[engine.review({action:actions[values[0]]}).page];
    else assert.ok(['show','snapshot','document'].includes(kind));
    assert.equal(page,expected.processing.wizardPage,label+' destination');
    if(expected.selected) {
      assert.deepEqual(engine.selectedProfile(),profileFromFrame(expected.selected),label+' selected profile');
      const backup=Buffer.from(expected.paletteRuntime.backupArgbBits,'hex');
      assert.deepEqual(engine.paletteRuntime(),{customized:expected.paletteRuntime.customized,
        backup:Array.from({length:backup.length/4},(_,i)=>backup.readFloatLE((Math.floor(i/4)*4+3-i%4)*4))},label+' palette runtime');
    }
    if(kind==='document')compareDocumentState(engine.snapshot().document,expected.document.state,parameters);
  }
});

test('Review Result rejects invalid commands before changing a loaded document',()=>{
  const engine=new GuiEngine(),[filename,encoded]=Object.entries(inputs)[0];
  engine.open({name:filename,bytes:Buffer.from(encoded,'base64')});engine.automatic({settings:engine.selectedProfile().settings});
  const before=engine.snapshot();
  for(const input of [undefined,null,[],{},true,{action:'unknown'},{action:'high',quality:2}]) {
    assert.throws(()=>engine.review(input),/Choose artwork/);assert.deepEqual(engine.snapshot(),before);
  }
});

test('HTTP Review Result reaches the worker, updates processing, and respects batch ownership',async t=>{
  const host=await startIsolatedGui({port:0});t.after(()=>host.close());
  const post=async(action,input,status=200)=>{
    const response=await fetch(host.url+'/api/'+action,{method:'POST',headers:{Origin:host.url,
      'Content-Type':'application/json','User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'},body:JSON.stringify(input)});
    assert.equal(response.status,status);return response.json();
  };
  const [name,bytes]=Object.entries(inputs)[0],opened=await post('open',{name,bytes});
  await post('automatic',{settings:opened.settings});
  const selected=await post('review',{action:'medium'});
  assert.equal(selected.page,'review');assert.equal(selected.selection.settings.quality,1);assert.ok(selected.document.svg);
  await post('review',{action:'medium',quality:0},400);
  const background=await post('review',{action:'background'});assert.equal(background.page,'background');assert.ok(background.document.background.active);
  await post('batch',{command:'enter'});await post('review',{action:'low'},409);
  await post('batch',{command:'leave'});assert.equal((await post('review',{action:'hand-pick'})).page,'type');
});
