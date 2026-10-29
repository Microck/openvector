import test from 'node:test';
import assert from 'node:assert/strict';
import { BatchRunner } from '../src/batch-runner.mjs';
import { compareDocumentState } from './helpers/native-gui.mjs';
import { batchProvenance, batchReferences, createBatchCase, assertBatchResult } from './helpers/native-batch.mjs';
import { writeFileSync, readdirSync } from 'node:fs';

for (const reference of batchReferences) test('filesystem batch matches original: ' + reference.id, async t => {
  const context = createBatchCase(reference, t);
  const runner = new BatchRunner({ queue: context.queue });
  for (const settings of reference.priorSettings) {
    const prior = await runner.run({ settings });
    assert.equal(prior.status, 'complete');assert.equal(prior.failed, 0);
  }
  let decisions = 0, lastExport, appeared = false;
  const stages = [], taskKinds = {load:8, classification:1, palette:2, segmentation:3, smoothing:4, fitting:5, export:6};
  const began = Date.now();
  const actual = await runner.run({ settings: reference.settings,
    decideOverwrite: async outputs => { decisions++;assert.ok(outputs.length > 0);return reference.decision; },
    onStage: event => {
      stages.push([event.index, taskKinds[event.stage], event.state.status, event.state.progress,
        Number(event.state.cancelled), event.state.randomState]);
      const appearance = reference.filesystemAppearance;
      if (appearance && !appeared && event.index === appearance.index) {
        const target = context.queue.snapshot().queue[appearance.index].outputFilename;
        writeFileSync(target, Buffer.from(appearance.bytes, 'base64'), { flag: 'wx' });appeared = true;
      }
      if (event.stage === 'export') lastExport = event.state;
    } });
  const ended = Date.now();
  assert.equal(decisions, Object.keys(reference.initialOutputs).length || reference.priorSettings.length ? 1 : 0);
  assert.equal(appeared, Boolean(reference.filesystemAppearance));
  assertBatchResult(actual, context, began, ended);
  if (reference.expected.document) compareDocumentState(lastExport, reference.expected.document.state, batchProvenance.parameters);
  if (reference.taskTrace) assert.deepEqual(stages, reference.taskTrace.events.filter(event => event.kind === 0)
    .map(event => [event.index,event.task,event.result,event.progress,event.cancelled,event.randomState]), 'native completed computation/export boundaries');
});

test('batch validation rejects invalid controls and inherited format names before writes', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  const runner = new BatchRunner({ queue: context.queue });
  for (const input of [{ settings: null }, { settings: { quality: 4 } }, { settings: { advanced: 1 } },
    { settings: { advanced: true, colorCount: 0 } }, { settings: { sensitivity: 3 } },
    { settings: { advanced: true, smoothness: 0 } }, { settings: { advanced: true, antiAlias: 1 } },
    { settings: { paletteMode: 0 } }, { settings: { advanced: true, unexpected: 1 } },
    { settings: { advanced: true, palette: null } }, { settings: { advanced: true, palette: [0,0,2,1] } },
    { settings: { advanced: true, palette: [0,0,0] } },
    { exportOptions: { layout: 'unknown' } }, { exportOptions: { thinStroke: 1 } }, { onStage: 2 }])
    await assert.rejects(runner.run(input));
  for (const format of ['toString', '__proto__', 'SVG']) {
    context.queue.exportFormat = format;await assert.rejects(runner.run(), /Batch format/);
  }
  assert.deepEqual(readdirSync(context.outputDirectory), []);
});

test('one runner rejects concurrent jobs while awaiting the real overwrite decision', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'overwrite-cancel'), t);
  const runner = new BatchRunner({ queue: context.queue });
  let decide;
  const pending = runner.run({ decideOverwrite: () => new Promise(resolve => { decide = resolve; }) });
  await assert.rejects(runner.run(), /already processing/);
  decide('cancel');assert.equal((await pending).status, 'cancelled');
  const next = await runner.run({ decideOverwrite: () => 'cancel' });assert.equal(next.status, 'cancelled');
});
