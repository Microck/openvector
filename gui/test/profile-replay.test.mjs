import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,rm,readdir} from 'node:fs/promises';
import {join,dirname,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {BatchController} from '../batch-controller.mjs';
import {ProfileController} from '../profile-controller.mjs';
import {AutoUpdate,wizardPageSettings} from '../auto-update.mjs';
import {startGui} from '../server.mjs';
import {exportSvg} from '../../src/svg.mjs';
import {compareDocumentState,profileFromFrame} from '../../test/helpers/native-gui.mjs';

const [provenance,...references] = gunzipSync(readFileSync(new URL('../../test/fixtures/profile-replay-window-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const [navigationProvenance,...navigation] = gunzipSync(readFileSync(new URL('../../test/fixtures/profile-navigation-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
assert.deepEqual(navigationProvenance.parameters,provenance.parameters);
const pages = {palette:12,transparency:6,export:17};
const slots = {'setUseAdvancedMode(bool)':'advanced','setPaletteMode(int)':'paletteMode','setFlattenImageMode(int)':'backgroundMode'};

async function context(t) {
  const directory = await mkdtemp(join(tmpdir(),'openvector-profile-replay-'));
  t.after(async () => {
    assert.equal(dirname(directory),resolve(tmpdir()));
    assert.ok(directory.startsWith(join(tmpdir(),'openvector-profile-replay-')));await rm(directory,{recursive:true,force:true});
  });
  const engine = new GuiEngine(),batch = new BatchController({engine,applicationDirectory:directory,
    workingDirectory:directory,cancellation:new Int32Array(new SharedArrayBuffer(4))});
  const filename = join(directory,'profiles.json');
  return {directory,filename,engine,batch,profiles:await ProfileController.open({engine,batch,filename})};
}

// Hidden-window captures establish the native widget visibility precondition.
// The service is the explicit action boundary for an available GUI workspace.
for (const reference of [...references.filter(row => !row.id.startsWith('hidden-window')),...navigation])
test('saved-profile replay matches native GUI and numerical state: '+reference.id,async t => {
  const {engine,profiles} = await context(t),automatic = new AutoUpdate();
  let now=0,page=4;
  for (const [index,action] of reference.actions.entries()) {
    const [kind,...values] = action,expected = reference.expected[index];
    if (kind === 'open' || kind === 'automatic') {
      const source = values[0];engine.open({name:source,bytes:Buffer.from(reference.inputs[source],'base64')});
      page=4;automatic.enterPage(null,engine.selectedProfile().settings,engine.state().stageKeys,now);
      if (kind === 'automatic') {engine.automatic({settings:engine.selectedProfile().settings});page=19;}
    } else if (kind === 'set') {
      const [receiver,signature,value] = values;assert.equal(receiver,2);
      if (signature === 'setAdvNumColors(int)') engine.configureProfile({colorCount:value});
      else if (signature === 'addColor()') engine.palette({action:'add'});
      else if (signature === 'setPaletteFromList(int)') engine.palette({action:'choose',
        palette:engine.state().palettes.find(candidate => candidate.count === value+1).colors.flat()});
      else {const key=slots[signature];assert.ok(key,signature);engine.configureProfile({settings:{[key]:key === 'advanced' ? Boolean(value) : value}});}
    } else if (kind === 'white') engine.configureProfile({settings:{background:Float32Array.of(1,1,1,1)}});
    else if (kind === 'save') await profiles.command({command:'save',name:values[0],force:true});
    else if (kind === 'load') await profiles.command({command:'select',name:values[0]});
    else if (kind === 'replay') {
      const [command,name] = values;
      if (!profiles.names().includes(name)) await assert.rejects(profiles.command({command,name}),/not found/);
      else {
        const next = await profiles.command({command,name});
        if (next.replay.page !== null) page=pages[next.replay.page];
        automatic.enterPage(next.replay.page,engine.selectedProfile().settings,engine.state().stageKeys,now);
      }
    } else if (kind === 'next') {
      // These captures cover entry to Palette and Segmentation after Play,
      // with the previous page's work already complete before Next.
      assert.ok([6,12].includes(page));
      const destination = page === 6 ? 'palette' : 'segmentation';
      page = destination === 'palette' ? 12 : 13;
      engine.configureProfile({settings:wizardPageSettings(destination,engine.selectedProfile().settings)});
      automatic.enterPage(destination,engine.selectedProfile().settings,engine.state().stageKeys,now);
    } else if (kind === 'wait') {
      now+=values[0];
      for (const stage of automatic.advance(now)) {
        const profile=engine.selectedProfile(),count=profile.colorCount;
        if (stage === 'palette') {
          const next=engine.palettes({settings:profile.settings,count,lower:count,upper:count});
          engine.palette({action:'replace',palette:next.palettes.find(candidate => candidate.count === count).colors.flat()});
        } else {
          assert.equal(stage,'segmentation');engine.process({settings:profile.settings,count,through:'segmentation'});
        }
      }
    } else assert.ok(['show','snapshot','document'].includes(kind),kind);
    assert.equal(page,expected.processing.wizardPage,'native wizard page at action '+index);
    if (expected.selected) {
      assert.deepEqual(engine.selectedProfile(),profileFromFrame(expected.selected),'complete selected controls at '+index);
      assert.equal(engine.continuing,expected.replay.continuing,'native continuing flag at '+index);
      assert.deepEqual(profiles.names(),expected.profiles.map(row => row.name),'MRU order at '+index);
      const runtime=expected.paletteRuntime,bytes=Buffer.from(runtime.backupArgbBits,'hex');
      const backup=Array.from({length:bytes.length/16},(_,color) =>
        Array.from({length:4},(_,channel) => bytes.readFloatLE(color*16+(3-channel)*4))).flat();
      assert.deepEqual(engine.paletteRuntime(),{backup,customized:runtime.customized},'runtime palette at '+index);
    }
    if (kind === 'document') {
      compareDocumentState(engine.snapshot().document,expected.document.state,provenance.parameters);
      assert.equal(engine.snapshot().committedCount,Buffer.from(expected.document.state.settingsBits,'hex').readInt32LE(8));
    }
  }
});

test('profile replay validates its workspace before changing names, document or files',async t => {
  const {engine,batch,profiles,directory} = await context(t),before=engine.snapshot(),names=profiles.names();
  for (const command of ['play','fast-forward']) {
    await assert.rejects(profiles.command({command,name:names[0]}),/Load an image/);
    assert.deepEqual(engine.snapshot(),before);assert.deepEqual(profiles.names(),names);assert.deepEqual(await readdir(directory),[]);
  }
  const reference=references[0],source=reference.actions[0][1];
  engine.open({name:source,bytes:Buffer.from(reference.inputs[source],'base64')});
  await batch.command({command:'enter'});const selected=engine.snapshot();
  for (const command of ['play','fast-forward']) {
    await assert.rejects(profiles.command({command,name:names[0]}),/single-image workspace/);
    assert.deepEqual(engine.snapshot(),selected);assert.deepEqual(profiles.names(),names);
  }
});

test('HTTP replay uses the actual worker, stored palette metadata and retained completed scene',async t => {
  const {directory,filename} = await context(t),reference=references.find(row => row.id === 'fixed-empty-count-fast-forward');
  const source=reference.actions[0][1],host=await startGui({port:0,workingDirectory:directory,profileFilename:filename});
  t.after(() => host.close());
  const headers={'Content-Type':'application/json',Origin:host.url,'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const post=(action,input) => fetch(host.url+'/api/'+action,{method:'POST',headers,body:JSON.stringify(input)});
  assert.equal((await post('profiles',{command:'fast-forward',name:'Default Advanced Settings'})).status,400);
  await post('open',{name:source,bytes:reference.inputs[source]});
  await post('select',{colorCount:7,settings:{advanced:true,paletteMode:1}});
  await post('profiles',{command:'save',name:'empty-count'});
  let response=await post('profiles',{command:'fast-forward',name:'empty-count'});assert.equal(response.status,200);
  const completed=await response.json(),expected=profileFromFrame(reference.expected.at(-2).selected);
  assert.deepEqual(completed.replay,{mode:'fast-forward',page:'export'});
  assert.equal(completed.selection.count,expected.colorCount);assert.equal(completed.selection.paletteColorCount,expected.paletteColorCount);
  assert.equal(completed.document.stage,'fitting');
  const native = reference.expected.at(-1).document.state;
  assert.equal(completed.document.svg,exportSvg({geometry:native.fittedGeometry,
    width:native.sourceDimensions[0],height:native.sourceDimensions[1],hiddenRegions:[]}),
    'complete canonical SVG from the independently captured native geometry, including its XML declaration');
  response=await post('profiles',{command:'play',name:'empty-count'});assert.equal(response.status,200);
  const guided=await response.json();assert.deepEqual(guided.replay,{mode:'play',page:'palette'});
  assert.equal(guided.selection.count,7);assert.equal(guided.selection.paletteColorCount,0);
  assert.deepEqual(guided.selection.settings.palette,[]);assert.equal(guided.document.svg,completed.document.svg);
});
