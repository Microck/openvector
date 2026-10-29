import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, readdirSync, existsSync, rmSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { tmpdir } from 'node:os';
import { join, dirname, basename, resolve, sep } from 'node:path';
import { BatchQueue } from '../../src/batch-queue.mjs';

export const [batchProvenance, ...batchReferences] = gunzipSync(readFileSync(new URL('../fixtures/batch-runner-reference.ndjson.gz', import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

// Native captures ran on NTFS, where outputs differing only in case alias one
// file: the first created name stays listed and the last write supplies its
// bytes. Probe this host's temporary filesystem instead of assuming by OS.
const caseSensitiveFiles = (() => {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-case-'));
  try { writeFileSync(join(directory, 'a'), '');return !existsSync(join(directory, 'A')); }
  finally { rmSync(directory, { recursive: true, force: true }); }
})();

export function createBatchCase(reference, t) {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-batch-'));
  t.after(() => {
    assert.equal(dirname(directory), resolve(tmpdir()));
    assert.ok(basename(directory).startsWith('openvector-batch-'));
    rmSync(directory, { recursive: true, force: true });
  });
  const queue = new BatchQueue({ applicationDirectory: directory, workingDirectory: directory });
  const filenames = [];
  for (const source of reference.sources) {
    const filename = join(directory, ...source.name.split('/'));
    mkdirSync(dirname(filename), { recursive: true });writeFileSync(filename, Buffer.from(source.bytes, 'base64'));
    queue.addFiles([filename]);filenames.push(filename);
  }
  const outputDirectory = join(directory, ...reference.exportDirectory.split('/'));
  if (reference.directoryExists) mkdirSync(outputDirectory, { recursive: true });
  queue.exportDirectory = outputDirectory;queue.exportFormat = reference.format;
  for (const [name, bytes] of Object.entries(reference.initialOutputs)) writeFileSync(join(outputDirectory, name), Buffer.from(bytes, 'base64'));
  return { directory, queue, filenames, outputDirectory, reference };
}

function normalizedLog(log) {
  return log.replace(/^(Start time|End time): \d{2}:\d{2}:\d{2}$/gm, '$1: <clock>')
    .replace(/^(Done classifying|Done finding palette|Done segmenting|Done smoothing|Done fitting|Elapsed time): \d+:\d{2}\.\d{3}$/gm, '$1: <duration>');
}

function epsWithoutClock(bytes, began, ended) {
  const text = bytes.toString(), dates = Array.from(text.matchAll(/^%%CreationDate: (.+)\r$/gm));
  assert.equal(dates.length, 1);
  if (began !== undefined) {
    const date = Date.parse(dates[0][1]);
    assert.ok(date >= began - 1000 && date <= ended + 1000, 'EPS creation clock falls within this execution');
  }
  return text.replace(/^%%CreationDate: .+\r$/m, '%%CreationDate: <clock>\r');
}

export function assertBatchResult(actual, { reference, directory, outputDirectory }, began, ended) {
  const native = reference.expected, log = native.processing.log;
  const page = native.processing.wizardPage;
  assert.ok([0,1,2,3].includes(page), 'settled native batch page');
  const expectedStatus = page === 1 ? 'empty' : page === 2 || reference.decision === 'cancel' ? 'cancelled' :
    page === 0 ? 'no-files' : 'complete';
  assert.equal(actual.status, expectedStatus);
  assert.equal(actual.queue.processingIndex >>> 0, native.processingIndex);
  assert.deepEqual(actual.queue.queue.map(record => record.shouldProcess), native.queue.map(record => Boolean(record.initializedFlags[0])));
  // Captured paths use Windows separators below the evidence prefix. The same
  // relative paths were created here with this host's separator.
  const relocate = value => value.replaceAll(batchProvenance.evidencePrefix, directory)
    .replaceAll(batchProvenance.evidencePrefix.replaceAll('\\', '/'), directory.replaceAll('\\', '/'))
    .replaceAll('\\', sep);
  assert.equal(normalizedLog(actual.log), normalizedLog(relocate(log)));
  const failures = log.split('\n').filter(line => ['Failed To Open File.', 'Failed To Save File.'].includes(line)).length;
  assert.equal(actual.failed, failures);
  assert.equal(actual.converted, reference.expectedConverted);
  assert.equal(actual.skipped, native.queue.filter(record => !record.initializedFlags[0]).length);
  assert.deepEqual(actual.files.map(file => file.outputFilename), native.processing.outputs.map(output => relocate(output[0])));
  assert.deepEqual(actual.files.map(file => file.filename), native.queue.map(record => relocate(record.filename)));
  // Only declared timing fields vary. Check the independent durations against
  // this execution's wall interval instead of accepting arbitrary replacements.
  for (const line of actual.log.split('\n')) {
    const match = /^(?:Done classifying|Done finding palette|Done segmenting|Done smoothing|Done fitting|Elapsed time): (\d+):(\d{2})\.(\d{3})$/.exec(line);
    if (match) {
      const milliseconds = Number(match[1]) * 60000 + Number(match[2]) * 1000 + Number(match[3]);
      assert.ok(Number(match[2]) < 60 && milliseconds <= ended - began + 1000);
    }
  }
  if (reference.directoryExists) {
    // A case-sensitive host keeps every processed alias as its own file; the
    // native bytes are those of the last alias written in queue order.
    const processed = native.processing.outputs.map(output => basename(relocate(output[0])))
      .filter((name, index) => native.queue[index].initializedFlags[0]);
    const aliases = name => caseSensitiveFiles ? processed.filter(output => output.toLowerCase() === name.toLowerCase()) : [];
    const listing = new Set(Object.keys(reference.expectedOutputs).flatMap(name => [name, ...aliases(name)]));
    assert.deepEqual(readdirSync(outputDirectory).sort(), [...listing].sort());
    for (const [name, bytes] of Object.entries(reference.expectedOutputs)) {
      const holder = aliases(name).at(-1) ?? name;
      const observed = readFileSync(join(outputDirectory, holder)), expected = Buffer.from(bytes, 'base64');
      if (reference.format === 'eps') assert.equal(epsWithoutClock(observed, began, ended), epsWithoutClock(expected));
      else assert.deepEqual(observed, expected, holder);
    }
  } else assert.equal(existsSync(outputDirectory), false, 'missing export directory remains missing');
}
