import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { BatchQueue } from '../src/batch-queue.mjs';

const references = gunzipSync(readFileSync(new URL('./fixtures/batch-queue-reference.ndjson.gz', import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

for (const reference of references.filter(row => row.actions?.length)) {
  test('batch queue follows original operations: ' + reference.id, () => {
    const queue = new BatchQueue(reference);
    for (const { input, expected } of reference.actions) {
      let duplicates = [];
      switch (input.action) {
        case 0: break;
        case 1: duplicates = queue.addFiles(input.value.split('\0')); break;
        case 2: queue.clear(); break;
        case 3: queue.remove(input.value); break;
        case 4: queue.exportDirectory = input.value; break;
        case 5: queue.exportFormat = input.value; break;
        case 6: queue.processingIndex = input.value; break;
        case 7: queue.setAcceptingEdits(Boolean(input.value)); break;
        // Batch-mode navigation lives in the application controller. The
        // captured queue itself stays unchanged when that mode changes.
        case 8: break;
        case 9: queue.select(input.index, input.selected); break;
        case 10: queue.removeSelected(); break;
        default: assert.fail('Unrecognized retained native action.');
      }
      const actual = queue.snapshot();
      assert.equal(actual.exportDirectory, expected.exportDirectory);
      assert.equal(actual.exportFormat, expected.exportFormat);
      // The probe header writes a u32; the original setter and public model
      // use a signed int. Compare the retained bits, including native -1.
      assert.equal(actual.processingIndex >>> 0, expected.processingIndex);
      assert.equal(duplicates.length > 0, expected.dialogSeen);
      assert.deepEqual(actual.queue.map(({ filename, buttonIndex }) => ({ filename, buttonIndex })),
        expected.queue.map(({ filename, buttonIndex }) => ({ filename, buttonIndex })));
    }
  });
}

for (const reference of references.filter(row => row.filenames)) {
  test('batch output planning matches original paths: ' + reference.id, () => {
    const queue = new BatchQueue({ applicationDirectory: '.', workingDirectory: 'D:/migracion/vector-magic-re' });
    queue.addFiles(reference.filenames);
    queue.exportDirectory = reference.exportDirectory;
    queue.exportFormat = reference.exportFormat;
    const existing = new Set(reference.existingOutputs);
    const planned = queue.prepare(filename => existing.has(filename));
    assert.deepEqual(planned.overwrites, reference.existingOutputs);
    assert.equal(planned.processable, reference.filenames.length - existing.size);
    const decision = queue.decideOverwrite(reference.decision);
    assert.deepEqual(queue.snapshot().queue.map(record => record.outputFilename),
      reference.expected.processing.outputs.map(output => output[0]));
    assert.deepEqual(queue.snapshot().queue.map(record => record.shouldProcess),
      reference.expected.queue.map(record => Boolean(record.initializedFlags[0])));
    const processable = reference.expected.queue.filter(record => record.initializedFlags[0]).length;
    assert.deepEqual(decision, { shouldStart: reference.decision !== 'cancel' && processable > 0, processable });
  });
}

test('batch snapshots own records and failed input validation preserves the queue', () => {
  const queue = new BatchQueue({ applicationDirectory: 'D:/app', workingDirectory: 'D:/images' });
  queue.addFiles(['a.png', 'b.png']);
  const before = queue.snapshot();
  assert.throws(() => queue.addFiles(['c.png', 3]), /array of strings/);
  assert.throws(() => queue.select(2, true), /out of range/);
  assert.throws(() => queue.exportDirectory = null, /must be a string/);
  assert.throws(() => queue.exportFormat = null, /must be a string/);
  assert.throws(() => queue.processingIndex = 2.5, /signed int32/);
  assert.throws(() => queue.prepare(null), /existence reader/);
  assert.throws(() => queue.decideOverwrite('unknown'), /overwrite, skip or cancel/);
  assert.deepEqual(queue.snapshot(), before);
  before.queue[0].filename = 'changed';
  before.queue.pop();
  assert.deepEqual(queue.snapshot().queue.map(record => record.filename), ['a.png', 'b.png']);
});
