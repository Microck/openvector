import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,readFile,rm} from 'node:fs/promises';
import {join,dirname,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {BatchController} from '../batch-controller.mjs';
import {ProfileController} from '../profile-controller.mjs';
import {startGui} from '../server.mjs';
import {SavedAdvancedProfiles,defaultAdvancedProfileName} from '../../src/saved-profiles.mjs';
import {compareDocumentState,profileFromFrame} from '../../test/helpers/native-gui.mjs';

const [provenance,...references]=gunzipSync(readFileSync(new URL('../../test/fixtures/profile-management-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

async function context(t) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-profile-management-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-profile-management-')));
    await rm(directory,{recursive:true,force:true});
  });
  const engine=new GuiEngine(),batch=new BatchController({engine,applicationDirectory:directory,workingDirectory:directory,
    cancellation:new Int32Array(new SharedArrayBuffer(4))}),list=new SavedAdvancedProfiles(),filename=join(directory,'profiles.json');
  return {directory,filename,engine,batch,list,profiles:new ProfileController({engine,batch,profiles:list,filename})};
}

// The oracle composes original post-dialog primitives from prepared decisions.
// These cases do not establish native dialog focus, validation or acceptance.
for (const reference of references) test('prepared native profile management: '+reference.id,async t=>{
  const {engine,batch,list,profiles}=await context(t);
  for (const [index,action] of reference.actions.entries()) {
    const [kind,...values]=action,expected=reference.expected[index];
    if (kind==='automatic') {
      engine.open({bytes:Buffer.from(reference.inputs[values[0]],'base64'),name:values[0]});
      engine.automatic({settings:engine.selectedProfile().settings});
    } else if (kind==='set') {
      assert.deepEqual(values.slice(0,2),[2,'setAdvNumColors(int)']);engine.configureProfile({colorCount:values[2]});
    } else if (kind==='save' || kind==='save-many') {
      for (const name of kind==='save' ? [values[0]] : values[0]) await profiles.command({command:'save',name,force:true});
    } else if (kind==='batch') await batch.command({command:values[0]?'enter':'leave'});
    else if (kind==='load') await profiles.command({command:'select',name:values[0]});
    else if (kind==='rename' || kind==='delete') {
      const before=engine.snapshot(),requested=batch.snapshot(),[name]=values;
      const accepted=kind==='rename'?values[2]:values[1];
      if (accepted) {
        const input={command:kind,name,...(kind==='rename'?{newName:values[1],force:values[3]}:{})};
        if (!profiles.names().includes(name) || kind==='rename' && values[1]!==name && values[1]!=='' && profiles.names().includes(values[1]) && !values[3])
          await assert.rejects(profiles.command(input));
        else {
          const reply=await profiles.command(input);assert.deepEqual(reply.names,profiles.names());
          assert.ok(!Object.hasOwn(reply,'selection') && !Object.hasOwn(reply,'document') && !Object.hasOwn(reply,'batch'));
        }
      }
      assert.deepEqual(engine.snapshot(),before,'management leaves every selected/document field alone');
      assert.deepEqual(batch.snapshot(),requested,'management leaves the batch owner alone');
    } else assert.ok(['snapshot','document'].includes(kind),kind);
    if (expected.selected) {
      assert.deepEqual(engine.selectedProfile(),profileFromFrame(expected.selected),'selected profile at '+index);
      assert.deepEqual(batch.selectedProfile(),profileFromFrame(expected.batch),'batch profile at '+index);
      assert.deepEqual(list.snapshot(),expected.profiles.map(row=>({name:row.name,profile:profileFromFrame(row)})),
        'all exact labels, positions, settings, palette colors and counters at '+index);
    }
    if (kind==='document') compareDocumentState(engine.snapshot().document,expected.document.state,provenance.parameters);
  }
});

test('profile management service validates before mutation and does not persist rename no-ops',async t=>{
  const {engine,batch,list,profiles,filename}=await context(t);
  await profiles.command({command:'save',name:'source'});await profiles.command({command:'save',name:'target'});
  const bytes=await readFile(filename),snapshot=list.snapshot(),document=engine.snapshot(),requested=batch.snapshot();
  for (const input of [{command:'rename',name:'source',newName:42},{command:'rename',name:'source',newName:'target'},
    {command:'rename',name:'source',newName:'x',force:'yes'},{command:'delete',name:null},{command:'delete',name:'missing'}]) {
    await assert.rejects(profiles.command(input));assert.deepEqual(list.snapshot(),snapshot);assert.deepEqual(await readFile(filename),bytes);
  }
  for (const newName of ['','source']) {
    const reply=await profiles.command({command:'rename',name:'source',newName});assert.equal(reply.changed,false);
    assert.deepEqual(await readFile(filename),bytes);assert.deepEqual(list.snapshot(),snapshot);
  }
  await profiles.command({command:'rename',name:'source',newName:defaultAdvancedProfileName,force:true});
  assert.deepEqual(engine.snapshot(),document);assert.deepEqual(batch.snapshot(),requested);
  const restarted=await ProfileController.open({engine,batch,filename});assert.equal(restarted.names()[0],defaultAdvancedProfileName);
});

test('HTTP worker renames and deletes in single and batch workspaces without applying settings',async t=>{
  const {directory,filename}=await context(t),host=await startGui({port:0,workingDirectory:directory,profileFilename:filename});
  t.after(()=>host.close());
  const headers={'Content-Type':'application/json',Origin:host.url,'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const post=async(action,input)=>{
    const response=await fetch(host.url+'/api/'+action,{method:'POST',headers,body:JSON.stringify(input)});
    return {status:response.status,body:await response.json()};
  };
  const get=async action=>(await fetch(host.url+'/api/'+action,{headers})).json();
  await post('profiles',{command:'save',name:'old'});await post('profiles',{command:'save',name:'target'});
  const source=references[0].actions[0][1];await post('open',{name:source,bytes:references[0].inputs[source]});
  const image=await get('state'),batch=await get('batch');
  const renamed=await post('profiles',{command:'rename',name:'old',newName:'renamed'});assert.equal(renamed.status,200);
  assert.deepEqual(renamed.body.names,['target','renamed',defaultAdvancedProfileName]);assert.equal(renamed.body.changed,true);
  assert.deepEqual(await get('state'),image);assert.deepEqual(await get('batch'),batch);
  const bytes=await readFile(filename),blocked=await post('profiles',{command:'rename',name:'renamed',newName:'target'});
  assert.notEqual(blocked.status,200);assert.deepEqual(await readFile(filename),bytes);
  await post('batch',{command:'enter'});const entered=await get('batch');
  const removed=await post('profiles',{command:'delete',name:'renamed'});assert.equal(removed.status,200);
  assert.deepEqual(removed.body.names,['target',defaultAdvancedProfileName]);assert.deepEqual(await get('batch'),entered);
  const replaced=await post('profiles',{command:'rename',name:'target',newName:defaultAdvancedProfileName,force:true});
  assert.equal(replaced.status,200);assert.deepEqual(replaced.body.names,[defaultAdvancedProfileName]);
  const missing=await post('profiles',{command:'delete',name:'renamed'});assert.notEqual(missing.status,200);
});
