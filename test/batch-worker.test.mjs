import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { startBatchWorker } from '../bin/batch-worker.mjs';
import { createBatchCase, assertBatchResult, batchReferences } from './helpers/native-batch.mjs';

const [, ...references] = gunzipSync(readFileSync(new URL('./fixtures/batch-cancellation-reference.ndjson.gz', import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const taskKinds = { load: 8, classification: 1, palette: 2, segmentation: 3, smoothing: 4, fitting: 5, export: 6 };

for (const reference of references.filter(row => row.runs.length === 1)) test('worker cancellation matches original: ' + reference.id, async t => {
  const context = createBatchCase(reference, t), run = reference.runs[0];
  const controller = new AbortController(), stages = [];
  const began = Date.now();
  const job = startBatchWorker(context.queue, { settings: run.settings, decision: run.decision,
    signal: controller.signal, onStage: event => {
      stages.push([event.index, taskKinds[event.stage], event.state.status, event.state.progress,
        Number(event.state.cancelled), event.state.randomState]);
      if (event.index === run.cancelAfter.index && taskKinds[event.stage] === run.cancelAfter.task) controller.abort();
    } });
  const actual = await job.completion;
  assertBatchResult(actual, { ...context, reference: { ...reference, ...run } }, began, Date.now());
  assert.deepEqual(stages, run.taskTrace.events.filter(event => event.kind === 0)
    .map(event => [event.index, event.task, event.result, event.progress, event.cancelled, event.randomState]));
});

test('a worker observes a signal aborted before thread startup', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  const job = startBatchWorker(context.queue, { signal: AbortSignal.abort() });
  const actual = await job.completion;
  assert.equal(actual.status, 'cancelled');assert.equal(actual.log, '');assert.deepEqual(actual.files, []);
  assert.deepEqual(readdirSync(context.outputDirectory), []);
});

test('worker processing errors stay visible and terminate the thread', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  const job = startBatchWorker(context.queue, { settings: { quality: 4 } });
  await assert.rejects(job.completion, /quality/i);
  assert.deepEqual(readdirSync(context.outputDirectory), []);
});

test('observer failures reject the worker job without leaving it waiting for acknowledgement', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  for (const observer of ['onStage','onProgress']) for (const failure of [new Error('host observer failed'), undefined, 0]) {
    const job = startBatchWorker(context.queue, { [observer]: () => { throw failure; } });
    await assert.rejects(job.completion, error => error === failure);
  }
  assert.deepEqual(readdirSync(context.outputDirectory), []);
});

test('worker progress acknowledgements cancel active palette work before completion',async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'),t);
  const controller = new AbortController(), stages = [], progress = [];
  assert.throws(() => startBatchWorker(context.queue,{onProgress:true}),/observer must be a function/);
  const job = startBatchWorker(context.queue,{settings:{category:2,quality:0,paletteMode:1,colorCount:2},
    signal:controller.signal,onStage:event => stages.push(event.stage),onProgress:event => {
      progress.push([event.index,event.stage,event.colorCount,event.iteration,event.percent]);
      if (event.colorCount === 3 && event.iteration === 0) controller.abort();
    }});
  const cancelled = await job.completion;
  assert.equal(cancelled.status,'cancelled');assert.deepEqual(stages,['load']);
  assert.deepEqual(progress.at(-1),[0,'palette',3,0,9]);
  assert.ok(cancelled.files.every(file => file.status === 'pending'));
  assert.doesNotMatch(cancelled.log,/Done finding palette|End time|Done processing images/);
  assert.deepEqual(readdirSync(context.outputDirectory),[]);
});

for (const [stage,checkpoint] of Object.entries({segmentation:[6,-2,0,100],smoothing:[8,2,0,4],fitting:[10,2,0,97]}))
test(`worker progress acknowledgements interrupt ${stage} before its completion event`,async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'),t);
  const controller = new AbortController(),stages = [],progress = [];
  const job = startBatchWorker(context.queue,{settings:{category:2,quality:0,paletteMode:2,backgroundMode:1},
    signal:controller.signal,onStage:event => stages.push(event.stage),onProgress:event => {
      if (event.stage !== stage) return;
      progress.push([event.phase,event.substep,event.counter,event.percent]);
      if (event.phase === checkpoint[0] && event.substep === checkpoint[1]) controller.abort();
    }});
  const cancelled = await job.completion;
  assert.equal(cancelled.status,'cancelled');
  assert.deepEqual(stages,['load','segmentation','smoothing'].slice(0,['segmentation','smoothing','fitting'].indexOf(stage)+1));
  assert.deepEqual(progress.at(-1),checkpoint);
  assert.deepEqual(readdirSync(context.outputDirectory),[]);
  assert.doesNotMatch(cancelled.log,/Done fitting|Done processing images|End time/);
  const retry = startBatchWorker(context.queue,{settings:{category:2,quality:0,paletteMode:2,backgroundMode:1}});
  assert.equal((await retry.completion).status,'complete');
  assert.equal(readdirSync(context.outputDirectory).length,2);
});

test('forced termination finishes a worker waiting at an observed task boundary', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  const job = startBatchWorker(context.queue, { onStage: () => job.terminate() });
  assert.equal(await job.completion, null);
  assert.deepEqual(readdirSync(context.outputDirectory), []);
});

test('host timers run while the worker performs synchronous image processing', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'logo-alpha-auto'), t);
  let ticks = 0, firstTicks;
  const timer = setInterval(() => { ticks++; }, 1);
  const job = startBatchWorker(context.queue, { onStage: event => {
    if (firstTicks === undefined) firstTicks = ticks;
    if (event.stage === 'fitting') assert.ok(ticks > firstTicks, 'host advanced during mathematical worker tasks');
  } });
  try { assert.equal((await job.completion).status, 'complete'); }
  finally { clearInterval(timer); }
});
