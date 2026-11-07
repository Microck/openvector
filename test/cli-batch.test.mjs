import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { writeFileSync, readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { gunzipSync } from 'node:zlib';
import { CoreSession } from '../src/settings.mjs';
import { decodeImage, resolveImageFilename } from '../src/image-input.mjs';
import { defaultSelectedProfile, defaultBatchProfile, applyAdvancedProfile, batchProfileControls } from '../src/settings-profile.mjs';
import { batchReferences, createBatchCase, assertApplicationBatchResult } from './helpers/native-batch.mjs';

const cli = fileURLToPath(new URL('../bin/openvector.mjs', import.meta.url));
const invoke = (args, cwd) => spawnSync(process.execPath, [cli, ...args], { cwd, encoding: 'utf8' });

for (const reference of batchReferences.filter(row => row.sources.length && !row.filesystemAppearance && !row.priorSettings.length)) test('batch CLI matches original files and log: ' + reference.id, t => {
  const context = createBatchCase(reference, t), settings = join(context.directory, 'settings.json');
  writeFileSync(settings, JSON.stringify(reference.settings));
  const decision = reference.decision === 'overwrite' ? 'replace' : reference.decision;
  const began = Date.now();
  const run = invoke(['batch', ...context.filenames, '--output-dir', context.outputDirectory,
    '--format', reference.format, '--settings', settings, '--overwrite', decision], context.directory);
  const ended = Date.now();
  const result = JSON.parse(run.stdout);
  assert.equal(run.status, result.status === 'cancelled' ? 130 : result.failed ? 1 : 0, run.stderr);
  assert.equal(run.stderr.split('\n').filter(line => line.includes('Failed To ')).length, result.failed);
  assertApplicationBatchResult(result, context, began, ended, reference.settings);
});

test('batch CLI help ignores other arguments without processing inputs', () => {
  for (const args of [['batch', '--help', 'missing.png'], ['help', 'batch'], ['batch', '-h']]) {
    const run = invoke(args);
    assert.equal(run.status, 0, run.stderr);assert.equal(run.stderr, '');
    assert.match(run.stdout, /Usage: openvector batch/);
    assert.match(run.stdout, /--overwrite/);
  }
});

test('batch CLI rejects invalid arguments and controls before writing', t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  const settings = join(context.directory, 'invalid.json');writeFileSync(settings, '{"quality":4}');
  for (const args of [['batch'], ['batch', '-'], ['batch', ...context.filenames, '--format', 'toString'],
    ['batch', ...context.filenames, '--overwrite', 'yes'], ['batch', ...context.filenames, '--settings', settings]]) {
    const run = invoke([...args, '--output-dir', context.outputDirectory], context.directory);
    assert.equal(run.status, 1);assert.equal(run.stdout, '');assert.ok(run.stderr.length > 0);
    assert.deepEqual(readdirSync(context.outputDirectory), []);
  }
});

test('batch CLI keeps default AI format and the last source directory, reporting duplicate inputs', t => {
  const context = createBatchCase(batchReferences.find(row => row.id === 'uniform-pair'), t);
  const input = context.filenames[0], run = invoke(['batch', input, input], context.directory);
  assert.equal(run.status, 0, run.stderr);assert.match(run.stderr, /Duplicate input skipped/);
  const result = JSON.parse(run.stdout);assert.equal(result.converted, 1);assert.equal(result.files.length, 1);
  const nativeAi = batchReferences.find(row => row.id === 'format-ai').expectedOutputs['a.ai'];
  assert.deepEqual(readFileSync(result.files[0].outputFilename), Buffer.from(nativeAi, 'base64'));
  assert.equal(result.queue.exportFormat, 'ai');
});

test('batch CLI compact EPS reaches the real worker and writes macro-based files',t=>{
  const context=createBatchCase(batchReferences.find(row=>row.id==='uniform-pair'),t);
  const run=invoke(['batch',...context.filenames,'--format','eps','--compact','--output-dir',context.outputDirectory],context.directory);
  assert.equal(run.status,0,run.stderr);const summary=JSON.parse(run.stdout);assert.equal(summary.converted,context.filenames.length);
  for(const file of summary.files) {
    const output=readFileSync(file.outputFilename,'utf8');
    assert.match(output,/^%!PS-Adobe-3\.0 EPSF-3\.0\r\n/);
    assert.match(output,/\/mt \{ moveto \} def\r\n/);assert.match(output,/\r\nnp\r\n/);
    assert.match(output,/\r\n[-\d.]+ [-\d.]+ mt\r\n/);
  }
});

const [, ...profileReferences] = gunzipSync(readFileSync(new URL('./fixtures/batch-profiles-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

for (const [id,paletteMode] of [['custom-two',0],['custom-fixed',1],['custom-flexible',2]])
  test('batch CLI preserves supplied profile colors: '+id,t => {
    const reference = profileReferences.find(row => row.id === id),context = createBatchCase(reference,t);
    const source = reference.before[0][1],filename = resolveImageFilename(source);
    // Build explicit CLI input with the real automatic controller. These
    // uniform source exports also match when the CLI starts its fresh PRNG.
    const document = new CoreSession().createDocument({...decodeImage(Buffer.from(reference.inputs[source],'base64')),
      shadowControl:filename.shadowControl});
    const initial = defaultSelectedProfile(),automatic = document.runAutomatic({...initial.settings,extension:filename.extension});
    assert.equal(automatic.paletteCount,2);
    const profile = {...initial,settings:{...automatic.settings,complexity:9,smoothness:10,paletteMode},
      colorCount:7,paletteColorCount:automatic.paletteCount};
    const requested = applyAdvancedProfile(defaultBatchProfile(),profile,{batch:true});
    const settings = join(context.directory,'profile-settings.json');
    const controls=batchProfileControls(requested);
    writeFileSync(settings,JSON.stringify(controls));
    const began = Date.now();
    const run = invoke(['batch',...context.filenames,'--settings',settings,'--format','svg','--output-dir',context.outputDirectory],context.directory);
    assert.equal(run.status,0,run.stderr);assert.equal(run.stderr,'');
    assertApplicationBatchResult(JSON.parse(run.stdout),context,began,Date.now(),controls);
  });
