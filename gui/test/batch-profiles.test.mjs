import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {BatchController} from '../batch-controller.mjs';
import {startIsolatedGui} from './gui-host.mjs';
import {SavedAdvancedProfiles,defaultAdvancedProfile} from '../../src/saved-profiles.mjs';
import {compareDocumentState,profileFromFrame,settingsFromBits} from '../../test/helpers/native-gui.mjs';
import {createBatchCase,assertBatchResult} from '../../test/helpers/native-batch.mjs';

const [provenance,...references] = gunzipSync(readFileSync(new URL('../../test/fixtures/batch-profiles-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const [runtimeProvenance,...runtimeReferences] = gunzipSync(readFileSync(new URL('../../test/fixtures/batch-palette-runtime-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
assert.deepEqual(runtimeProvenance,provenance);
const kinds = {load:8,classification:1,palette:2,segmentation:3,smoothing:4,fitting:5,export:6};
const slots = {'setAdvSegmentationComplexity(int)':'complexity','setAdvContourSmoothness(int)':'smoothness',
  'setPaletteMode(int)':'paletteMode'};

for (const reference of [...references,...runtimeReferences]) test('GUI saved batch profile matches native: '+reference.id,async t => {
  const context = createBatchCase(reference,t),engine = new GuiEngine(),profiles = new SavedAdvancedProfiles();
  const stages = [],observed = new Set();
  let cancelSent = false,cancellation;
  const controller = new BatchController({engine,applicationDirectory:context.directory,workingDirectory:context.directory,
    cancellation:new Int32Array(new SharedArrayBuffer(4)),publish:state => {
      const current = state.job.current;
      if (!state.job.running || !current?.complete) return;
      const key = current.index+':'+current.stage;
      if (!observed.has(key)) {
        observed.add(key);
        const document = engine.snapshot().document;
        stages.push([current.index,kinds[current.stage],current.result,current.progress,Number(document.cancelled),document.randomState]);
      }
      if (!cancelSent && reference.cancelAfter?.[0] === kinds[current.stage] && reference.cancelAfter[1] === current.index) {
        cancelSent = true;cancellation = controller.command({command:'cancel'});
      }
    }});

  function compareProfiles(frame) {
    assert.deepEqual(engine.selectedProfile(),profileFromFrame(frame.selected),'selected single-image profile');
    assert.deepEqual(controller.selectedProfile(),profileFromFrame(frame.batch),'requested batch profile');
    assert.deepEqual(profiles.snapshot(),frame.profiles.map(row => ({name:row.name,profile:profileFromFrame(row)})),
      'saved entries retain exact profile snapshots');
    if (frame.paletteRuntime) {
      const backup = settingsFromBits(frame.selected.settingsBits,frame.paletteRuntime.backupArgbBits).palette;
      assert.deepEqual(engine.paletteRuntime(),{backup:Array.from(backup),customized:frame.paletteRuntime.customized},
        'palette runtime survives actual batch conversion');
    }
  }
  for (const [index,action] of reference.before.entries()) {
    const [kind,...values] = action;
    if (kind === 'automatic') {
      engine.open({bytes:Buffer.from(reference.inputs[values[0]],'base64'),name:values[0]});
      engine.automatic({settings:engine.selectedProfile().settings});
    } else if (kind === 'set') {
      const [receiver,signature,value] = values;assert.equal(receiver,2);
      if (signature === 'setAdvNumColors(int)') engine.configureProfile({colorCount:value});
      else if (signature === 'addColor()') engine.palette({action:'add'});
      else if (signature === 'setPaletteFromList(int)') {
        const colors = engine.state().palettes.find(candidate => candidate.count === value+1).colors.flat();
        engine.palette({action:'choose',palette:colors});
      }
      else {const name = slots[signature];assert.ok(name,signature);engine.configureProfile({settings:{[name]:value}});}
    } else if (kind === 'save') profiles.put(values[0],engine.selectedProfile());
    else if (kind === 'batch') {assert.equal(values[0],true);await controller.command({command:'enter'});}
    else if (kind === 'load') {
      const selected = profiles.select(values[0]);assert.ok(selected.profile);
      await controller.command({command:'apply-profile',profile:selected.profile});
    } else assert.fail('Unknown profile setup action: '+kind);
    if (reference.expectedBefore[index].selected) compareProfiles(reference.expectedBefore[index]);
  }
  const before = reference.expectedBefore.find(frame => frame.document);
  if (before) {
    compareDocumentState(engine.snapshot().document,before.document.state,provenance.parameters);
    assert.equal(engine.snapshot().committedCount,Buffer.from(before.document.state.settingsBits,'hex').readInt32LE(8),
      'batch selection does not commit the active single-image document');
  }
  await controller.command({command:'add',filenames:context.filenames});
  await controller.command({command:'configure',exportDirectory:context.outputDirectory,exportFormat:'svg'});
  const began = Date.now();await controller.command({command:'start'});
  const completed = await controller.completion;await cancellation;
  assert.equal(completed.job.error,null);
  assertBatchResult(completed.job.summary,context,began,Date.now());
  compareDocumentState(engine.snapshot().document,reference.expected.document.state,provenance.parameters);
  assert.equal(engine.snapshot().committedCount,Buffer.from(reference.expected.document.state.settingsBits,'hex').readInt32LE(8));
  compareProfiles(reference.profiles);
  assert.deepEqual(stages,reference.taskTrace.events.filter(event => event.kind === 0)
    .map(event => [event.index,event.task,event.result,event.progress,event.cancelled,event.randomState]),'actual task order and random consumption');
});

test('batch profile ownership and validation preserve both owners on errors',async t => {
  const context = createBatchCase(references[0],t),engine = new GuiEngine();
  const controller = new BatchController({engine,applicationDirectory:context.directory,workingDirectory:context.directory,
    cancellation:new Int32Array(new SharedArrayBuffer(4))});
  const before = controller.snapshot(),document = engine.snapshot();
  for (const profile of [null,{}, {...defaultAdvancedProfile(),paletteColorCount:NaN}]) {
    await assert.rejects(controller.command({command:'apply-profile',profile}));
    assert.deepEqual(controller.snapshot(),before);assert.deepEqual(engine.snapshot(),document);
  }
  const profile = defaultAdvancedProfile();profile.settings.palette = Float32Array.of(.1,.2,.3,1);
  await controller.command({command:'apply-profile',profile});
  const selected = controller.selectedProfile();profile.settings.palette.fill(1);selected.settings.palette.fill(0);
  assert.deepEqual(controller.selectedProfile().settings.palette,Float32Array.of(.1,.2,.3,1));
  assert.deepEqual(engine.snapshot().document,document.document);
  assert.equal(engine.selectedProfile().settings.advanced,true);
  await assert.rejects(engine.runBatch(context.queue,{settings:{advanced:true}}),/Unknown GUI batch option/);
});

test('HTTP batch profile colors survive JSON and worker transfer without replacing the requested count',async t => {
  const reference = references.find(row => row.id === 'custom-two'),context = createBatchCase(reference,t);
  const source = reference.before[0][1],preparation = new GuiEngine();
  preparation.open({bytes:Buffer.from(reference.inputs[source],'base64'),name:source});
  preparation.automatic({settings:preparation.selectedProfile().settings});
  const profile = preparation.configureProfile({colorCount:7,settings:{complexity:9,smoothness:10}});
  const host = await startIsolatedGui({port:0,workingDirectory:context.directory});t.after(() => host.close());
  const headers = {'Content-Type':'application/json',Origin:host.url,'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const post = input => fetch(host.url+'/api/batch',{method:'POST',headers,
    body:JSON.stringify(input,(_,value) => ArrayBuffer.isView(value) ? Array.from(value) : value)});
  await post({command:'enter'});await post({command:'add',filenames:context.filenames});
  await post({command:'configure',exportDirectory:context.outputDirectory,exportFormat:'svg'});
  const applied = await post({command:'apply-profile',profile});assert.equal(applied.status,200);
  const selected = await applied.json();
  assert.equal(selected.settings.colorCount,7);assert.equal(selected.settings.paletteMode,0);
  assert.deepEqual(selected.settings.palette,Array.from(profile.settings.palette));
  assert.equal((await post({command:'apply-profile',profile:null})).status,400);
  const began = Date.now();assert.equal((await post({command:'start'})).status,200);
  let completed;
  do {
    assert.ok(Date.now()-began < 5000,'batch worker settles within the test deadline');
    await new Promise(resolve => setTimeout(resolve,20));
    completed = await (await fetch(host.url+'/api/batch',{headers})).json();
  } while (completed.job.running);
  assert.equal(completed.job.error,null);assertBatchResult(completed.job.summary,context,began,Date.now());
  const document = await (await fetch(host.url+'/api/state',{headers})).json();
  assert.equal(document.count,7);assert.equal(document.committedCount,7);
  assert.deepEqual(document.settings.palette,Array.from(profile.settings.palette));
});
