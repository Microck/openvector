import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,rm} from 'node:fs/promises';
import {join,dirname,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {BatchController} from '../batch-controller.mjs';
import {ProfileController} from '../profile-controller.mjs';
import {suggestProfileName} from '../profile-name.mjs';
import {AutoUpdate} from '../auto-update.mjs';
import {startGui} from '../server.mjs';
import {SavedAdvancedProfiles} from '../../src/saved-profiles.mjs';
import {compareDocumentState,profileFromFrame} from '../../test/helpers/native-gui.mjs';

const [provenance,...references] = gunzipSync(readFileSync(new URL('../../test/fixtures/profile-reset-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const slots = {'setAdvNumColors(int)':'colorCount','setPaletteMode(int)':'paletteMode',
  'setAdvSegmentationComplexity(int)':'complexity','setAdvContourSmoothness(int)':'smoothness'};
const pages = {palette:12,transparency:6,export:17};

for (const reference of references) test('native Start Over and settings-name lifetime: '+reference.id,async t=>{
  const directory = await mkdtemp(join(tmpdir(),'openvector-profile-reset-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-profile-reset-')));
    await rm(directory,{recursive:true,force:true});
  });
  const engine = new GuiEngine(),batch = new BatchController({engine,applicationDirectory:directory,
    workingDirectory:directory,cancellation:new Int32Array(new SharedArrayBuffer(4))});
  const profiles = await ProfileController.open({engine,batch,filename:join(directory,'profiles.json')});
  const automatic = new AutoUpdate();
  let name=suggestProfileName(profiles.names()),page=0,now=0;
  const changePage = (next,automaticPage=null) => {
    if (page === 12 && next !== page)engine.leavePalette();
    page=next;automatic.enterPage(automaticPage,engine.selectedProfile().settings,engine.snapshot().document.stageKeys,now);
  };
  for (const [index,action] of reference.actions.entries()) {
    const [kind,...values] = action,expected = reference.expected[index];
    if (kind === 'open') {
      const source=values[0];engine.open({name:source,bytes:Buffer.from(reference.inputs[source],'base64')});
      changePage(4);name=suggestProfileName(profiles.names());
    } else if (kind === 'set') {
      const [receiver,signature,value] = values;assert.equal(receiver,2);
      if (signature === 'addColor()')engine.palette({action:'add'});
      else {
        const key=slots[signature];assert.ok(key,signature);
        engine.configureProfile(key === 'colorCount' ? {colorCount:value} : {settings:{[key]:value}});
      }
    } else if (kind === 'load') await profiles.command({command:'select',name:values[0]});
    else if (kind === 'save') await profiles.command({command:'save',name:values[0],force:true});
    else if (kind === 'replay') {
      const next=await profiles.command({command:values[0],name:values[1]});changePage(pages[next.replay.page],next.replay.page);
    } else if (kind === 'wait') {
      now+=values[0];
      for (const stage of automatic.advance(now)) {
        assert.equal(stage,'palette');const profile=engine.selectedProfile(),count=profile.colorCount;
        const next=engine.palettes({settings:profile.settings,count,lower:count,upper:count});
        engine.palette({action:'replace',palette:next.palettes.find(candidate=>candidate.count===count).colors.flat()});
      }
    } else if (kind === 'start-over') {
      if (page === 0) {engine.reset();name=suggestProfileName(profiles.names());}
      changePage(0);
    } else assert.ok(['show','snapshot','document'].includes(kind),kind);
    assert.equal(page,expected.processing.wizardPage,'wizard page at '+index);
    if (expected.selected) {
      assert.deepEqual(engine.selectedProfile(),profileFromFrame(expected.selected),'selected controls at '+index);
      assert.deepEqual(profiles.names(),expected.profiles.map(row=>row.name),'exact list order at '+index);
      assert.equal(engine.continuing,expected.replay.continuing,'continuing flag at '+index);
      assert.equal(name,expected.suggestedName,'reset-driven name suggestion at '+index);
      const runtime=expected.paletteRuntime,bytes=Buffer.from(runtime.backupArgbBits,'hex');
      const backup=Array.from({length:bytes.length/16},(_,color)=>Array.from({length:4},(_,channel)=>bytes.readFloatLE(color*16+(3-channel)*4))).flat();
      assert.deepEqual(engine.paletteRuntime(),{backup,customized:runtime.customized},'retained palette runtime at '+index);
    }
    if (kind === 'document') {
      compareDocumentState(engine.snapshot().document,expected.document.state,provenance.parameters);
      assert.equal(engine.snapshot().committedCount,Buffer.from(expected.document.state.settingsBits,'hex').readInt32LE(8));
    }
  }
});

test('HTTP palette exit commits without processing and full reset retains the selected controls/count',async t=>{
  const directory=await mkdtemp(join(tmpdir(),'openvector-profile-reset-http-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-profile-reset-http-')));
    await rm(directory,{recursive:true,force:true});
  });
  const reference=references[0],source=reference.actions[0][1],filename=join(directory,'profiles.json');
  const stored=new SavedAdvancedProfiles();stored.put('Settings 1',profileFromFrame(reference.expected[5].selected));
  await stored.writeFile(filename);
  const host=await startGui({port:0,workingDirectory:directory,profileFilename:filename});t.after(()=>host.close());
  const headers={'Content-Type':'application/json','User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const post=async(action,input)=>{
    const response=await fetch(host.url+'/api/'+action,{method:'POST',headers:{...headers,Origin:host.url},body:JSON.stringify(input)});
    assert.equal(response.status,200);return response.json();
  };
  const state=async()=>{const response=await fetch(host.url+'/api/state',{headers});assert.equal(response.status,200);return response.json();};
  const upload={name:source,bytes:reference.inputs[source]};
  await post('open',upload);
  const played=await post('profiles',{command:'play',name:'Settings 1'});
  const prepared=await post('palettes',{settings:played.selection.settings,count:7,lower:7,upper:7});
  await post('palette',{action:'replace',palette:prepared.palettes.find(candidate=>candidate.count===7).colors.flat()});
  const before=await state();assert.deepEqual(before.committedSettings.palette,[]);
  const committed=await post('leavePalette',null),after=await state();
  assert.deepEqual(committed.committedSettings,after.settings);assert.equal(committed.committedCount,7);
  assert.deepEqual(after.committedSettings,after.settings);assert.equal(after.settings.palette.length,28);
  assert.deepEqual(after.stageKeys,before.stageKeys);assert.equal(after.regionCount,0);assert.equal(after.svg,null);
  assert.equal(await post('reset',null),null);assert.equal(await state(),null);
  const reopened=await post('open',upload);
  assert.equal(reopened.settings.advanced,true);assert.equal(reopened.count,7);assert.equal(reopened.paletteColorCount,2);
  assert.deepEqual(reopened.settings.palette,[]);assert.equal(reopened.settings.backgroundMode,-1);
  assert.deepEqual(reopened.settings.background,[1,1,1,0]);
});
