import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { BatchRunner } from '../src/batch-runner.mjs';
import { CoreSession } from '../src/palette.mjs';
import { decodeImage, resolveImageFilename } from '../src/image-input.mjs';
import { readBatchControls } from '../src/document-controls.mjs';
import { compareDocumentState } from './helpers/native-gui.mjs';
import { batchReferences, createBatchCase, assertBatchResult } from './helpers/native-batch.mjs';

const [provenance, ...references] = gunzipSync(readFileSync(new URL('./fixtures/batch-cancellation-reference.ndjson.gz', import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const taskKinds = { load: 8, classification: 1, palette: 2, segmentation: 3, smoothing: 4, fitting: 5, export: 6 };

for (const reference of references) test('batch cancellation matches original: ' + reference.id, async t => {
  const context = createBatchCase(reference, t);
  const runner = new BatchRunner({ queue: context.queue });
  // Retain the same runner, files and random stream for the native retry jobs.
  for (const job of reference.runs) {
    const controller = new AbortController(), stages = [];
    let decisions = 0;
    const began = Date.now();
    const actual = await runner.run({ settings: job.settings, signal: controller.signal,
      decideOverwrite: outputs => {
        decisions++;assert.equal(outputs.length, Object.keys(job.initialOutputs).length);return job.decision;
      },
      onStage: event => {
        assert.equal(runner.snapshot().running, true);
        stages.push([event.index, taskKinds[event.stage], event.state.status, event.state.progress,
          Number(event.state.cancelled), event.state.randomState]);
        if (job.cancelAfter?.index === event.index && job.cancelAfter.task === taskKinds[event.stage]) controller.abort();
      } });
    const ended = Date.now();
    assert.equal(decisions, Object.keys(job.initialOutputs).length ? 1 : 0);
    assertBatchResult(actual, { ...context, reference: { ...reference, ...job } }, began, ended);
    assert.deepEqual(stages, job.taskTrace.events.filter(event => event.kind === 0)
      .map(event => [event.index, event.task, event.result, event.progress, event.cancelled, event.randomState]),
    'completed computation/export boundaries');
    const snapshot = runner.snapshot();
    assert.equal(snapshot.running, false);
    assert.deepEqual(snapshot.queue, actual.queue);
    compareDocumentState(snapshot.document, job.expected.document.state, provenance.parameters);
    // Mutate public snapshots to verify that retry does not inherit caller edits.
    snapshot.queue.queue[0].filename = 'caller-owned snapshot';
    snapshot.document.source.bgra.fill(0);
    snapshot.document.settings.palette.fill(0);
    compareDocumentState(runner.snapshot().document, job.expected.document.state, provenance.parameters);
  }
});

test('a pre-aborted batch leaves its queue and retained document untouched', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  const runner = new BatchRunner({ queue: context.queue });
  const before = runner.snapshot();
  const cancelled = await runner.run({ signal: AbortSignal.abort() });
  assert.deepEqual(cancelled, { status: 'cancelled', files: [], queue: before.queue, log: '', converted: 0, failed: 0, skipped: 0 });
  assert.deepEqual(runner.snapshot(), before);
  assert.deepEqual(readdirSync(context.outputDirectory), []);
  await runner.run();
  const retained = runner.snapshot();
  await runner.run({ signal: AbortSignal.abort() });
  assert.deepEqual(runner.snapshot(), retained);
});

test('an empty queue remains empty with an aborted signal', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  context.queue.clear();
  const runner = new BatchRunner({ queue: context.queue });
  const before = runner.snapshot();
  assert.equal((await runner.run({ signal: AbortSignal.abort() })).status, 'empty');
  assert.deepEqual(runner.snapshot(), before);
});

test('aborting an awaited overwrite decision preserves preflight eligibility and document', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'overwrite-cancel'), t);
  const runner = new BatchRunner({ queue: context.queue });
  await runner.run({ decideOverwrite: () => 'overwrite' });
  const retained = runner.snapshot().document;
  const controller = new AbortController();
  let decide;
  const pending = runner.run({ signal: controller.signal,
    decideOverwrite: () => new Promise(resolve => { decide = resolve; }) });
  assert.equal(runner.snapshot().running, true);
  controller.abort();decide('overwrite');
  const cancelled = await pending;
  assert.equal(cancelled.status, 'cancelled');
  assert.ok(cancelled.files.every(file => file.status === 'skipped'));
  assert.ok(cancelled.queue.queue.every(record => !record.shouldProcess));
  assert.deepEqual(runner.snapshot().document, retained);
  assert.equal(runner.snapshot().running, false);
  assert.doesNotMatch(cancelled.log, /Starting to process images|End time|Done processing images/);
});

test('invalid signals fail before queue mutation or output writes', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  const runner = new BatchRunner({ queue: context.queue });
  const before = runner.snapshot();
  for (const signal of [null, false, {}, { aborted: true }]) await assert.rejects(runner.run({ signal }), /AbortSignal/);
  assert.deepEqual(runner.snapshot(), before);
  assert.deepEqual(readdirSync(context.outputDirectory), []);
});

test('observer failures propagate and release the runner for another job', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  const runner = new BatchRunner({ queue: context.queue });
  const failure = new Error('observer failure');
  await assert.rejects(runner.run({ onStage: () => { throw failure; } }), error => error === failure);
  assert.equal(runner.snapshot().running, false);
  assert.equal(runner.snapshot().document.cancelled, false);
  assert.deepEqual(readdirSync(context.outputDirectory), []);
  assert.equal((await runner.run()).status, 'complete');
});

test('batch cancellation inside palette analysis retains its native-tested document state and supports retry', async t => {
  const settings = {category:2,quality:0,paletteMode:1,colorCount:2,backgroundMode:1};
  for (const colorCount of [2,3]) {
    const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
    const runner = new BatchRunner({queue:context.queue}), controller = new AbortController(), stages = [], progress = [];
    const cancelled = await runner.run({settings,signal:controller.signal,
      onStage:event => stages.push(event.stage),onProgress:event => {
        assert.equal(event.index,0);assert.equal(event.filename,context.filenames[0]);
        assert.equal(event.stage,'palette');assert.equal(event.status,0);
        progress.push([event.colorCount,event.iteration,event.fraction,event.percent]);
        if (event.colorCount === colorCount && event.iteration === 0) controller.abort();
      }});
    assert.equal(cancelled.status,'cancelled');assert.equal(cancelled.queue.processingIndex,0);
    assert.deepEqual(stages,['load']);assert.ok(cancelled.files.every(file => file.status === 'pending'));
    assert.doesNotMatch(cancelled.log,/Done finding palette|End time|Done processing images/);
    assert.deepEqual(readdirSync(context.outputDirectory),[]);

    // The direct document operation is compared against 234 recorded native
    // scenarios in document-cancellation.test.mjs. This integration check
    // proves that batch orchestration retains that operation's entire state.
    const document = new CoreSession().createDocument(), controls = readBatchControls(settings);
    const source = decodeImage(readFileSync(context.filenames[0]));
    const {shadowControl} = resolveImageFilename(context.filenames[0]);
    await document.runBatchLoad(controls,async () => ({...source,shadowControl}));
    document.sourceAlphaType();
    const expectedProgress = [];
    assert.throws(() => document.analyze({...document.snapshot().settings,lower:2,upper:12,onProgress:event => {
      expectedProgress.push([event.colorCount,event.iteration,event.fraction,event.percent]);
      if (event.colorCount === colorCount && event.iteration === 0) document.requestCancel();
    }}),{name:'CancelTaskException'});
    assert.deepEqual(progress,expectedProgress);
    assert.deepEqual(runner.snapshot().document,document.snapshot());
    assert.equal(Boolean(runner.snapshot().document.completed[2]),colorCount === 3);
    assert.equal(runner.snapshot().running,false);
    assert.equal((await runner.run({settings})).status,'complete');
    assert.equal(runner.snapshot().document.cancelled,false);
    assert.equal(readdirSync(context.outputDirectory).length,2);
  }
});

test('progress observer validation and errors cannot masquerade as task cancellation',async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'),t);
  const runner = new BatchRunner({queue:context.queue}),before = runner.snapshot();
  await assert.rejects(runner.run({onProgress:true}),/observers must be functions/);
  assert.deepEqual(runner.snapshot(),before);
  const controller = new AbortController(),failure = new Error('observer failed');failure.name = 'CancelTaskException';
  await assert.rejects(runner.run({signal:controller.signal,onProgress:() => {controller.abort();throw failure;}}),
    error => error === failure);
  assert.equal(runner.snapshot().running,false);assert.deepEqual(readdirSync(context.outputDirectory),[]);
});

for (const [stage,checkpoints] of Object.entries({segmentation:[[2,-1],[6,-2]],smoothing:[[8,1],[8,-2]],fitting:[[10,2],[10,-2]]}))
for (const [phase,substep] of checkpoints)
test(`batch ${stage} interruption retains incomplete state before output: ${phase}/${substep}`,async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'),t);
  const runner = new BatchRunner({queue:context.queue}),controller = new AbortController(),stages = [];
  const settings = {category:2,quality:0,paletteMode:2,backgroundMode:1};
  const cancelled = await runner.run({settings,signal:controller.signal,onStage:event => stages.push(event.stage),
    onProgress:event => {if (event.stage === stage && event.phase === phase && event.substep === substep) controller.abort();}});
  assert.equal(cancelled.status,'cancelled');
  assert.deepEqual(stages,['load','segmentation','smoothing'].slice(0,['segmentation','smoothing','fitting'].indexOf(stage)+1));
  const state = runner.snapshot().document;
  assert.equal(state.status,2);assert.equal(state.cancelled,true);assert.equal(state.progress,100);
  assert.equal(state.stageKeys.fitting,'');
  if (stage === 'segmentation') {
    assert.equal(state.stageKeys.segmentation,'');assert.equal(state.stageKeys.smoothing,'');
    assert.equal(state.fitting,null);assert.equal(state.contours,null);
    if (phase === 2) assert.equal(state.segmentation.indices,null);
    else assert.ok(state.segmentation.counts.length>0);
  }
  else if (stage === 'smoothing') {assert.equal(state.stageKeys.smoothing,'');assert.equal(state.fitting,null);}
  else assert.equal(state.fitting.complete,false);
  assert.ok(state.original.width>0);
  if (stage !== 'segmentation') assert.ok(state.contours.nodes.length>0);
  assert.deepEqual(readdirSync(context.outputDirectory),[]);
  assert.doesNotMatch(cancelled.log,/Done fitting|Done processing images|End time/);
  if (stage === 'smoothing') assert.doesNotMatch(cancelled.log,/Done smoothing/);
  if (stage === 'segmentation') assert.doesNotMatch(cancelled.log,/Done segmenting|Done smoothing/);
  assert.equal((await runner.run({settings})).status,'complete');
  assert.equal(runner.snapshot().document.fitting.complete,true);
  assert.equal(readdirSync(context.outputDirectory).length,2);
});
