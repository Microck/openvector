import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { performance } from 'node:perf_hooks';
import { GuiEngine } from '../engine.mjs';
import { BatchController } from '../batch-controller.mjs';
import {startIsolatedGui} from './gui-host.mjs';
import { defaultBatchProfile } from '../../src/settings-profile.mjs';
import { cleanStartupProfile, compareDocumentState, profileFromFrame } from '../../test/helpers/native-gui.mjs';
import { batchReferences, createBatchCase, assertBatchResult } from '../../test/helpers/native-batch.mjs';

const [provenance,...references] = gunzipSync(readFileSync(new URL('../../test/fixtures/batch-gui-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const kinds = {load:8,classification:1,palette:2,segmentation:3,smoothing:4,fitting:5,export:6};

function warmed(reference) {
  const engine = new GuiEngine();
  if (reference.priorAutomatic) {
    const source = reference.priorAutomatic.source;
    const loaded = engine.open({bytes:Buffer.from(source.bytes,'base64'),name:source.name});
    engine.automatic({settings:{...loaded.settings,...structuredClone(cleanStartupProfile.controls)}});
  }
  return engine;
}

for (const reference of references) test('GUI batch document matches native: '+reference.id,async t => {
  const context = createBatchCase(reference,t),engine = warmed(reference),controller = new AbortController(),stages = [];
  const began = Date.now();
  const profile = defaultBatchProfile();
  const {colorCount,...settings} = reference.settings;
  profile.settings = {...profile.settings,...settings};
  if (colorCount !== undefined) profile[settings.advanced ? 'colorCount' : 'basicColorCount'] = colorCount;
  const {summary,document} = await engine.runBatch(context.queue,{profile,signal:controller.signal,
    onStage:event => {
      stages.push([event.index,kinds[event.stage],event.state.status,event.state.progress,Number(event.state.cancelled),event.state.randomState]);
      if (event.stage === 'load') assert.equal(event.source === null,event.state.status !== 0);
      if (reference.cancelAfter?.task === kinds[event.stage] && reference.cancelAfter.index === event.index) controller.abort();
    }});
  assertBatchResult(summary,context,began,Date.now());
  compareDocumentState(document,reference.expected.document.state,provenance.parameters);
  const expected = reference.expected.document;
  assert.deepEqual(engine.selectedProfile(),profileFromFrame({settingsBits:expected.controls.selectedSettingsBits,
    paletteArgbBits:expected.selectedPaletteArgbBits}),'complete selected batch profile');
  assert.equal(engine.snapshot().committedCount,Buffer.from(expected.state.settingsBits,'hex').readInt32LE(8));
  if (reference.taskTrace) assert.deepEqual(stages,reference.taskTrace.events.filter(event => event.kind === 0)
    .map(event => [event.index,event.task,event.result,event.progress,event.cancelled,event.randomState]));
  const presentation = engine.state(true);
  if (!document.original.width) assert.equal(presentation,null,'failed/cancelled loads discard stale viewer content');
  else {
    assert.equal(presentation.width,document.original.width);
    assert.equal(Boolean(presentation.svg),Boolean(document.fitting));
    if (document.fitting) assert.equal(Buffer.from(engine.export({format:'svg',layout:'grouped-cutouts'}).bytes).toString(),
      Buffer.from(reference.expectedOutputs[presentation.file.name.replace(/\.[^.]+$/,'')+'.svg'],'base64').toString());
  }
});

test('GUI batch controller converts real files without thumbnail PRNG consumption',async t => {
  const reference = references.find(row => row.id === 'gui-single-then-basic'),context = createBatchCase(reference,t),engine = warmed(reference);
  const publications = [];
  const controller = new BatchController({engine,applicationDirectory:context.directory,workingDirectory:context.directory,
    cancellation:new Int32Array(new SharedArrayBuffer(4)),publish:(state,assets) => publications.push({state,assets})});
  await controller.command({command:'enter'});
  await controller.command({command:'add',filenames:context.filenames});
  await controller.command({command:'add',filenames:[context.filenames[0]]});
  assert.deepEqual(controller.snapshot().duplicates,[context.filenames[0]]);
  await controller.command({command:'dismiss-duplicates'});
  assert.ok(controller.snapshot().thumbnails.every(image => image.id.length === 64 && !Object.hasOwn(image,'rgba') && image.width <= 96 && image.height <= 96));
  assert.equal(publications.flatMap(event => event.assets).length,2,'publish pixels once when each image is added');
  await controller.command({command:'configure',settings:reference.settings,exportDirectory:context.outputDirectory,exportFormat:'svg'});
  const began = Date.now();await controller.command({command:'start'});const final = await controller.completion;
  assert.ok(publications.filter(event => event.state.job.running).every(event => event.assets.length === 0),'progress carries no pixel payloads');
  assert.equal(final.page,'summary');assert.equal(final.job.running,false);
  assertBatchResult(final.job.summary,context,began,Date.now());
  const snapshot = controller.snapshot();snapshot.queue.queue[0].filename = 'caller-owned';snapshot.settings.quality = 0;
  assert.notDeepEqual(controller.snapshot(),snapshot);
  await controller.command({command:'leave'});assert.equal(controller.active,false);
  assert.ok(engine.state().svg,'single-image mode can use the last batch document');
});

test('GUI batch controls retain separate basic/advanced counts and validate atomically',async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'),t);
  const controller = new BatchController({engine:new GuiEngine(),applicationDirectory:context.directory,workingDirectory:context.directory,
    cancellation:new Int32Array(new SharedArrayBuffer(4))});
  await controller.command({command:'configure',settings:{colorCount:12}});
  await controller.command({command:'configure',settings:{advanced:true}});assert.equal(controller.snapshot().settings.colorCount,2);
  await controller.command({command:'configure',settings:{colorCount:24}});
  await controller.command({command:'configure',settings:{advanced:false}});assert.equal(controller.snapshot().settings.colorCount,12);
  await controller.command({command:'configure',settings:{advanced:true}});assert.equal(controller.snapshot().settings.colorCount,24);
  const before = controller.snapshot();
  for (const input of [{command:'configure',settings:null},{command:'configure',settings:{quality:4}},
    {command:'configure',exportOptions:{thinStroke:1}},{command:'configure',exportFormat:'constructor'},
    {command:'configure',exportDirectory:0},{command:'page',page:'unknown'},{command:'decide',decision:'overwrite'}]) {
    await assert.rejects(controller.command(input));assert.deepEqual(controller.snapshot(),before);
  }
});

test('a returned batch document cannot mutate the GUI settings or retained scene',async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'),t),engine = new GuiEngine();
  const {document} = await engine.runBatch(context.queue);
  const before = engine.state(true);
  document.settings.background.fill(0);document.settings.palette.fill(0);
  document.fitting.geometry.nodes[0].x = -1000;
  document.fitting.geometry.regions[0].bgra.fill(0);
  assert.deepEqual(engine.state(true),before);
});

test('GUI overwrite cancellation preserves the previous document and releases its job',async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'overwrite-cancel'),t);
  const engine = warmed(references[0]),before = engine.state(true);
  let waiting;const overwrite = new Promise(resolve => {waiting = resolve;});
  const controller = new BatchController({engine,applicationDirectory:context.directory,workingDirectory:context.directory,
    cancellation:new Int32Array(new SharedArrayBuffer(4)),publish:state => {if (state.job.overwrite) waiting(state);}});
  await controller.command({command:'add',filenames:context.filenames});
  await controller.command({command:'configure',exportDirectory:context.outputDirectory,exportFormat:'svg'});
  await controller.command({command:'start'});const modal = await overwrite;
  assert.ok(modal.job.overwrite.length);await assert.rejects(controller.command({command:'clear'}),/Wait/);
  await controller.command({command:'cancel'});const final = await controller.completion;
  assert.equal(final.job.status,'cancelled');assert.equal(final.page,'settings');
  assert.deepEqual(engine.state(true),before);
});

test('GUI palette cancellation polls repeated percentages and retains the loaded source for retry',async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'),t),engine = new GuiEngine();
  const before = engine.snapshot();
  await assert.rejects(engine.runBatch(context.queue,{onProgress:true}),/progress observer must be a function/);
  assert.deepEqual(engine.snapshot(),before);
  const cancellation = new Int32Array(new SharedArrayBuffer(4)),progress = [];
  let requested = false;
  const controller = new BatchController({engine,applicationDirectory:context.directory,workingDirectory:context.directory,
    cancellation,publish:(state,assets) => {
      if (!state.job.running || state.job.current?.stage !== 'palette') return;
      assert.deepEqual(assets,[]);
      if (!requested) assert.equal(state.job.current.complete,false,'even 100 percent is not task completion');
      progress.push([state.job.current.progress,state.job.status]);
      // Native reports 100% at iterations 5..9. Set the shared host flag
      // after publishing iteration 5, then require cancellation on the next
      // checkpoint even though its displayed percentage has not changed.
      if (state.job.current.progress === 100 && !requested) {requested = true;Atomics.store(cancellation,0,1);}
    }});
  await controller.command({command:'add',filenames:context.filenames});
  await controller.command({command:'configure',settings:{advanced:true,paletteMode:1,colorCount:2},
    exportDirectory:context.outputDirectory,exportFormat:'svg'});
  await controller.command({command:'start'});const final = await controller.completion;
  assert.deepEqual(progress,[[0,'running'],[20,'running'],[40,'running'],[60,'running'],[80,'running'],[100,'running'],[100,'cancelling']]);
  assert.equal(final.job.status,'cancelled');assert.equal(final.page,'settings');assert.equal(final.job.error,null);
  const retained = engine.snapshot().document;
  assert.equal(retained.cancelled,true);assert.equal(retained.progress,100);assert.equal(retained.completed[2],0);
  assert.ok(retained.original.width > 0);assert.ok(retained.source.width > 0);
  assert.equal(engine.state(true).file.name,context.filenames[0].split(/[\\/]/).at(-1));
  assert.equal(engine.selectedProfile().settings.backgroundMode,1,'palette cancellation does not reset a completed load');
  assert.deepEqual(readdirSync(context.outputDirectory),[]);
  await controller.command({command:'start'});assert.equal((await controller.completion).job.status,'complete');
});

for (const stage of ['segmentation','smoothing'])
test(`GUI ${stage} cancellation at 100 percent keeps the active task incomplete and supports retry`,async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'),t),engine = new GuiEngine();
  const cancellation = new Int32Array(new SharedArrayBuffer(4)),progress = [];
  let requested = false;
  const controller = new BatchController({engine,applicationDirectory:context.directory,workingDirectory:context.directory,
    cancellation,publish:state => {
      if (!state.job.running || state.job.current?.stage !== stage || state.job.current.complete) return;
      progress.push([state.job.current.progress,state.job.status]);
      if (state.job.current.progress === 100 && !requested) {requested = true;Atomics.store(cancellation,0,1);}
    }});
  await controller.command({command:'add',filenames:context.filenames});
  await controller.command({command:'configure',settings:{category:stage === 'segmentation' ? 1 : 2,quality:stage === 'segmentation' ? 1 : 0,paletteMode:2,backgroundMode:1},
    exportDirectory:context.outputDirectory,exportFormat:'svg'});
  await controller.command({command:'start'});const final = await controller.completion;
  assert.deepEqual(progress.slice(-2),[[100,'running'],[100,'cancelling']]);
  assert.equal(final.job.status,'cancelled');assert.equal(final.job.error,null);
  const retained = engine.snapshot().document;
  assert.equal(retained.status,2);assert.equal(retained.cancelled,true);assert.equal(retained.progress,100);
  assert.equal(retained.stageKeys.smoothing,'');assert.equal(retained.stageKeys.fitting,'');assert.equal(retained.fitting,null);
  assert.ok(retained.original.width>0);
  if (stage === 'segmentation') {
    assert.equal(retained.stageKeys.segmentation,'');assert.equal(retained.contours,null);
    assert.equal(engine.state(true).segments,null);
  } else assert.ok(retained.contours.nodes.length>0);
  assert.equal(engine.state(true).svg,null);assert.equal(engine.state(true).preview,null);
  assert.throws(() => engine.export({format:'svg'}),/Finish vectorizing/);
  assert.deepEqual(readdirSync(context.outputDirectory),[]);
  await controller.command({command:'start'});assert.equal((await controller.completion).job.status,'complete');
  assert.ok(engine.state().svg.startsWith('<?xml'));
});

test('GUI segmentation cancellation never adopts unavailable or exported-but-incomplete maps',async t => {
  for (const [phase,substep] of [[2,-1],[6,-2]]) {
    const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'),t),engine = new GuiEngine();
    const controller = new AbortController(),stages = [];
    const profile = defaultBatchProfile();
    profile.settings = {...profile.settings,category:2,quality:0,paletteMode:2,backgroundMode:1};
    const {summary,document} = await engine.runBatch(context.queue,{profile,signal:controller.signal,
      onStage:event => stages.push(event.stage),onProgress:event => {
        if (event.stage === 'segmentation' && event.phase === phase && event.substep === substep) controller.abort();
      }});
    assert.equal(summary.status,'cancelled');assert.deepEqual(stages,['load']);
    assert.equal(document.stageKeys.segmentation,'');assert.equal(document.status,2);
    if (phase === 2) assert.equal(document.segmentation.indices,null);
    else assert.ok(document.segmentation.counts.length>0,'exported records alone do not complete the task');
    const presentation = engine.state(true);
    assert.ok(presentation.bitmap.rgba);assert.equal(presentation.segments,null);
    assert.equal(presentation.svg,null);assert.equal(presentation.preview,null);
    assert.throws(() => engine.export({format:'svg'}),/Finish vectorizing/);
    assert.deepEqual(readdirSync(context.outputDirectory),[]);
    assert.equal((await engine.runBatch(context.queue,{profile})).summary.status,'complete');
    assert.ok(engine.state().svg.startsWith('<?xml'));
  }
});

test('GUI fitting cancellation retains its source and does not render incomplete curves',async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'),t),engine = new GuiEngine();
  const cancellation = new Int32Array(new SharedArrayBuffer(4)),progress = [];
  let requested = false;
  const controller = new BatchController({engine,applicationDirectory:context.directory,workingDirectory:context.directory,
    cancellation,publish:state => {
      if (!state.job.running || state.job.current?.stage !== 'fitting' || state.job.current.complete) return;
      progress.push([state.job.current.progress,state.job.status]);
      if (state.job.current.progress === 97 && !requested) {requested = true;Atomics.store(cancellation,0,1);}
    }});
  await controller.command({command:'add',filenames:context.filenames});
  await controller.command({command:'configure',settings:{category:2,quality:0,paletteMode:2,backgroundMode:1},
    exportDirectory:context.outputDirectory,exportFormat:'svg'});
  await controller.command({command:'start'});const final = await controller.completion;
  assert.deepEqual(progress,[[0,'running'],[1,'running'],[97,'running'],[99,'cancelling']]);
  assert.equal(final.job.status,'cancelled');assert.equal(final.job.error,null);
  const retained = engine.snapshot().document;
  assert.equal(retained.status,2);assert.equal(retained.cancelled,true);assert.equal(retained.fitting.complete,false);
  assert.ok(retained.original.width>0);assert.ok(retained.contours.nodes.length>0);
  assert.equal(engine.state(true).svg,null);assert.equal(engine.state(true).preview,null);
  assert.throws(() => engine.export({format:'svg'}),/Finish vectorizing/);
  assert.deepEqual(readdirSync(context.outputDirectory),[]);
  await controller.command({command:'start'});assert.equal((await controller.completion).job.status,'complete');
  assert.ok(engine.state().svg.startsWith('<?xml'));
});

async function settled(host) {
  const deadline = performance.now()+15000;
  while (performance.now()<deadline) {
    const state = await (await fetch(host.url+'/api/batch')).json();
    if (!state.job.running) return state;
    await new Promise(resolve => setTimeout(resolve,20));
  }
  throw new Error('Batch job did not settle within its bounded test deadline.');
}

test('HTTP batch cache stays responsive, rejects conflicting work and cancels a real worker',async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'logo-alpha-auto'),t);
  const host = await startIsolatedGui({port:0,workingDirectory:context.directory});t.after(() => host.close());
  const post = input => fetch(host.url+'/api/batch',{method:'POST',headers:{'Content-Type':'application/json',Origin:host.url},body:JSON.stringify(input)});
  assert.equal((await fetch(host.url+'/api/batch',{headers:{Origin:'https://unrelated.example'}})).status,403);
  assert.equal((await post(null)).status,400);
  await post({command:'enter'});await post({command:'add',filenames:context.filenames});
  const images = await (await fetch(host.url+'/api/batch')).json();
  const asset = await (await fetch(host.url+'/api/batch-thumbnail?id='+images.thumbnails[0].id)).json();
  assert.equal(Buffer.from(asset.rgba,'base64').length,asset.width*asset.height*4);
  assert.equal((await fetch(host.url+'/api/batch-thumbnail?id=not-a-member')).status,404);
  await post({command:'configure',exportDirectory:context.outputDirectory,exportFormat:'svg'});
  const started = await (await post({command:'start'})).json();assert.equal(started.job.running,true);
  const began = performance.now();const current = await (await fetch(host.url+'/api/batch')).json();
  assert.ok(performance.now()-began<500,'state reads do not queue behind the mathematical worker');
  assert.equal(current.job.running,true);
  assert.equal((await post({command:'clear'})).status,409);
  assert.equal((await fetch(host.url+'/api/reset',{method:'POST',headers:{'Content-Type':'application/json'},body:'null'})).status,409);
  const cancelledAt = performance.now();const cancellation = await (await post({command:'cancel'})).json();
  assert.ok(performance.now()-cancelledAt<500,'cancellation is acknowledged by the host immediately');
  assert.equal(cancellation.job.status,'cancelling');
  const final = await settled(host);assert.equal(final.job.status,'cancelled');
  assert.ok(final.revision>started.revision);assert.equal(final.job.error,null);
  await post({command:'leave'});assert.equal((await fetch(host.url+'/api/state')).status,200);
});

test('HTTP overwrite decisions and queued cancellation complete without blocking the host',async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'overwrite-cancel'),t);
  const host = await startIsolatedGui({port:0,workingDirectory:context.directory});t.after(() => host.close());
  const post = input => fetch(host.url+'/api/batch',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(input)});
  await post({command:'enter'});await post({command:'add',filenames:context.filenames});
  await post({command:'configure',exportDirectory:context.outputDirectory,exportFormat:'svg'});
  await post({command:'start'});
  const modal = await (await fetch(host.url+'/api/batch')).json();assert.ok(modal.job.overwrite.length);
  await post({command:'decide',decision:'cancel'});
  assert.equal((await settled(host)).job.status,'cancelled');
  const invalid = await post({command:'configure',settings:{quality:4}});assert.equal(invalid.status,400);
  assert.equal((await (await post({command:'configure',settings:{quality:1}})).json()).settings.quality,1);
});
