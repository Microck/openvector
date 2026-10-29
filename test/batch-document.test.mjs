import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { CoreSession } from '../src/palette.mjs';
import { decodeImage, resolveImageFilename } from '../src/image-input.mjs';
import { BatchRunner } from '../src/batch-runner.mjs';
import { cleanStartupProfile, compareDocumentState } from './helpers/native-gui.mjs';
import { batchReferences, createBatchCase, assertBatchResult } from './helpers/native-batch.mjs';

const [provenance, ...references] = gunzipSync(readFileSync(new URL('./fixtures/batch-gui-reference.ndjson.gz', import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const taskKinds = {load:8,classification:1,palette:2,segmentation:3,smoothing:4,fitting:5,export:6};

for (const reference of references) test('single-image and batch share the native document: ' + reference.id, async t => {
  const context = createBatchCase(reference, t);
  const session = new CoreSession();
  let document;
  if (reference.priorAutomatic) {
    const source = reference.priorAutomatic.source;
    const { extension, shadowControl } = resolveImageFilename(source.name);
    document = session.createDocument({ ...decodeImage(Buffer.from(source.bytes, 'base64')), shadowControl });
    const automatic = document.runAutomatic({ ...structuredClone(cleanStartupProfile.controls), extension });
    compareDocumentState(automatic.state, reference.priorAutomatic.expected, provenance.parameters);
  } else {
    document = session.createDocument();
    document.commitSettings(structuredClone(cleanStartupProfile.controls));
  }
  const runner = new BatchRunner({ queue: context.queue, document });
  assert.equal(runner.document, document);
  const stages = [];
  const controller = new AbortController();
  const began = Date.now();
  const result = await runner.run({ settings: reference.settings, signal: controller.signal, onStage: event => {
    stages.push([event.index,taskKinds[event.stage],event.state.status,event.state.progress,
      Number(event.state.cancelled),event.state.randomState]);
    if (reference.cancelAfter?.task === taskKinds[event.stage] && reference.cancelAfter.index === event.index) controller.abort();
  } });
  assertBatchResult(result, context, began, Date.now());
  assert.equal(runner.document, document, 'keep the same document across workflows');
  if (reference.priorAutomatic) assert.deepEqual(reference.expected.documentIdentity, { sameDocument: true, sameContext: true });
  compareDocumentState(document.snapshot(), reference.expected.document.state, provenance.parameters);
  if (reference.taskTrace) assert.deepEqual(stages,reference.taskTrace.events.filter(event => event.kind === 0)
    .map(event => [event.index,event.task,event.result,event.progress,event.cancelled,event.randomState]),
    'all native load/computation/export boundaries');
});

test('an application can adopt a document first created by batch conversion', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  const runner = new BatchRunner({ queue: context.queue });
  assert.equal(runner.document, null);
  assert.equal((await runner.run()).status, 'complete');
  const document = runner.document;
  assert.ok(document);
  const snapshot = runner.snapshot();
  snapshot.document.source.bgra.fill(0);
  assert.notDeepEqual(document.snapshot().source.bgra, snapshot.document.source.bgra);
  assert.equal((await runner.run({ decideOverwrite: () => 'overwrite' })).status, 'complete');
  assert.equal(runner.document, document);
});

test('application startup controls and failed loads preserve document ownership', async t => {
  const context = createBatchCase(references.find(row => row.id === 'gui-first-batch-invalid'), t);
  const runner = new BatchRunner({ queue: context.queue });
  assert.equal((await runner.run()).failed, 1);
  const document = runner.document;
  assert.ok(document, 'even the first failed load creates an adoptable document');
  const controls = structuredClone(cleanStartupProfile.controls);
  document.commitSettings(controls);
  const before = document.snapshot();
  controls.background.fill(1);
  assert.deepEqual(document.snapshot(), before, 'application controls are copied');
  assert.throws(() => document.commitSettings({...cleanStartupProfile.controls,quality:4}), /quality/);
  await assert.rejects(document.runBatchLoad({quality:4}, () => decodeImage(readFileSync(context.filenames[0]))), /quality/);
  await assert.rejects(document.runBatchLoad({}, null), /source operation/);
  assert.deepEqual(document.snapshot(), before, 'invalid task requests preserve startup state');
});

test('document adoption waits for an active batch decision to settle', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'overwrite-cancel'), t);
  const runner = new BatchRunner({ queue: context.queue });
  let decide;
  const pending = runner.run({ decideOverwrite: () => new Promise(resolve => { decide = resolve; }) });
  assert.throws(() => runner.document, /Wait for the batch job/);
  decide('cancel');assert.equal((await pending).status, 'cancelled');
  assert.equal(runner.document, null);
});

test('batch ownership rejects invalid or competing owners before mutating the queue', async t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  const before = context.queue.snapshot();
  for (const document of [null, {}, new CoreSession()])
    assert.throws(() => new BatchRunner({ queue: context.queue, document }), /ImageDocument/);
  for (const session of [null, {}])
    assert.throws(() => new BatchRunner({ queue: context.queue, session }), /CoreSession/);
  const source = context.reference.sources[0];
  const session = new CoreSession(), document = session.createDocument({
    ...decodeImage(Buffer.from(source.bytes, 'base64')), shadowControl: resolveImageFilename(source.name).shadowControl,
  });
  assert.throws(() => new BatchRunner({ queue: context.queue, session, document }), /Choose an existing document or a session/);
  assert.deepEqual(context.queue.snapshot(), before);
});
