import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createReadStream } from 'node:fs';
import { createGunzip } from 'node:zlib';
import { createInterface } from 'node:readline';
import mergeReference from './fixtures/merge-cost-reference.json' with { type: 'json' };
import initialReference from './fixtures/initial-regions-reference.json' with { type: 'json' };
import passReference from './fixtures/region-pass-reference.json' with { type: 'json' };
import segmentationReference from './fixtures/unweighted-segmentation-reference.json' with { type: 'json' };
import diagonalReference from './fixtures/diagonal-contacts-reference.json' with { type: 'json' };
import diagonalSegmentationReference from './fixtures/unweighted-diagonal-segmentation-reference.json' with { type: 'json' };
import presegmentationReference from './fixtures/presegmentation-reference.json' with { type: 'json' };
import antiAliasReference from './fixtures/anti-alias-segmentation-reference.json' with { type: 'json' };
import clusteredReference from './fixtures/clustered-anti-alias-segmentation-reference.json' with { type: 'json' };
import pipelineReference from './fixtures/pipeline-segmentation-reference.json' with { type: 'json' };
import suppliedPaletteReference from './fixtures/supplied-palette-segmentation-reference.json' with { type: 'json' };
import { initializeRegions, regionColorMergeCost, createRegionOptimizer, segmentUnweighted, presegmentRegions, segment } from '../src/segmentation.mjs';

test('supplied-palette segmentation matches all 782 supported native results', () => {
  assert.equal(suppliedPaletteReference.cases.length, 782);
  assert.equal(suppliedPaletteReference.unsupportedCases.length, 482);
  for (const [index, fixture] of suppliedPaletteReference.cases.entries()) {
    const actual = segment(fixture);
    for (const key of ['indices', 'counts', 'palette', 'colorIndices']) {
      assert.deepEqual(Array.from(actual[key]), fixture.expected[key], `supplied case ${index} ${key}`);
    }
    assert.deepEqual(Array.from(actual.palette), fixture.palette, 'Preserve supplied floats, order and unused entries.');
  }
});

test('supplied-palette segmentation owns colors and pixels and validates its input', () => {
  for (const antiAlias of [0, 1]) {
    const input = structuredClone(suppliedPaletteReference.cases.find(fixture =>
      fixture.width === 17 && fixture.palette.length > 4 && fixture.parameters['Shared::is_anti_aliased'] === antiAlias));
    input.palette = Float32Array.from(input.palette);
    input.bgra = Uint8Array.from(input.bgra);
    const before = structuredClone(input);
    const segmented = segment(input);
    segmented.palette.fill(0);
    segmented.indices.fill(0);
    assert.deepEqual(input, before);
    for (const palette of [null, [], [0], [0, 0, 0, 2], [NaN, 0, 0, 1]]) {
      assert.throws(() => segment({ ...input, palette }), /Palette/);
    }
  }
});

test('region color merge costs match all 1042 original SSE calculations', () => {
  assert.equal(mergeReference.cases.length, 1042);
  for (const fixture of mergeReference.cases) {
    assert.equal(regionColorMergeCost(fixture), fixture.expected);
  }
});

test('recursive tile initialization matches all 300 original region maps', () => {
  assert.equal(initialReference.cases.length, 300);
  for (const fixture of initialReference.cases) {
    const labels = initializeRegions(fixture);
    assert.ok(labels instanceof Int32Array);
    assert.deepEqual(Array.from(labels), fixture.expected, `${fixture.width}x${fixture.height} threshold ${fixture.threshold}`);
  }
});

test('segmentation primitives preserve caller storage and reject malformed boundaries', () => {
  const raster = { width: 1, height: 1, bgra: [83, 149, 211, 255], threshold: 0.0001 };
  const merge = { counts: [1, 2], sums: [[0, 0, 0, 1], [1, 1, 1, 2]] };
  const before = structuredClone({ raster, merge });
  initializeRegions(raster);
  regionColorMergeCost(merge);
  assert.deepEqual({ raster, merge }, before);
  for (const invalid of [null, {}, { ...raster, width: 0 }, { ...raster, height: 1.5 },
    { ...raster, bgra: [0] }, { ...raster, bgra: new Array(4) }, { ...raster, bgra: [0, 0, 0, 256] },
    { ...raster, threshold: -1 }, { ...raster, threshold: Infinity }]) {
    assert.throws(() => initializeRegions(invalid));
  }
  for (const invalid of [null, {}, { ...merge, counts: [0, 1] }, { ...merge, counts: new Array(2) },
    { ...merge, counts: [2147483647, 1] }, { ...merge, sums: [[0], [0]] },
    { ...merge, sums: [[NaN, 0, 0, 1], [1, 1, 1, 2]] }]) {
    assert.throws(() => regionColorMergeCost(invalid));
  }
});

test('unweighted optimizer passes match all 945 native maps and statistic arrays', () => {
  assert.equal(passReference.cases.length, 945);
  for (const [index, fixture] of passReference.cases.entries()) {
    const optimizer = createRegionOptimizer(fixture);
    optimizer.iterate({ lambda: fixture.lambda, mode: fixture.mode });
    const actual = optimizer.snapshot();
    assert.deepEqual(Array.from(actual.indices), fixture.expected.indices, `case ${index} labels`);
    assert.deepEqual(Array.from(actual.counts), fixture.expected.counts, `case ${index} counts`);
    assert.deepEqual(Array.from(actual.sums), fixture.expected.sums, `case ${index} sums`);
    assert.deepEqual(Array.from(actual.colorIndices), fixture.expected.colorIndices, `case ${index} colors`);
    assert.equal(actual.activeCount, fixture.expected.activeCount, `case ${index} active regions`);
  }
});

test('complete recovered unweighted segmentation matches all 160 original runs', () => {
  assert.equal(segmentationReference.cases.length, 160);
  for (const [index, fixture] of segmentationReference.cases.entries()) {
    const actual = segmentUnweighted(fixture);
    for (const field of ['indices', 'counts', 'palette', 'colorIndices']) {
      assert.deepEqual(Array.from(actual[field]), fixture.expected[field], `case ${index} ${field}`);
    }
  }
});

test('diagonal-contact repair matches all 994 native maps without changing statistics', () => {
  assert.equal(diagonalReference.cases.length, 994);
  let changed = 0;
  for (const [index, fixture] of diagonalReference.cases.entries()) {
    const optimizer = createRegionOptimizer(fixture);
    optimizer.iterate(fixture);
    assert.deepEqual(Array.from(optimizer.snapshot().indices), fixture.beforeRepair, `case ${index} before repair`);
    optimizer.repairDiagonalContacts();
    const actual = optimizer.snapshot();
    for (const field of ['indices', 'counts', 'sums', 'colorIndices']) {
      assert.deepEqual(Array.from(actual[field]), fixture.expected[field], `case ${index} ${field}`);
    }
    assert.equal(actual.activeCount, fixture.expected.activeCount);
    if (fixture.beforeRepair.some((label, pixel) => label !== fixture.expected.indices[pixel])) changed++;
  }
  assert.ok(changed > 0, 'Corpus must exercise replacements, not only unchanged maps.');
});

test('complete diagonal segmentation matches all 168 original artwork-profile runs', () => {
  assert.equal(diagonalSegmentationReference.cases.length, 168);
  for (const [index, fixture] of diagonalSegmentationReference.cases.entries()) {
    const actual = segmentUnweighted(fixture);
    for (const field of ['indices', 'counts', 'palette', 'colorIndices']) {
      assert.deepEqual(Array.from(actual[field]), fixture.expected[field], `case ${index} ${field}`);
    }
  }
});

test('optimizer storage, snapshots, palette projection, and pass validation preserve ownership', () => {
  const input = { width: 2, height: 1, bgra: [0, 0, 0, 255, 255, 255, 255, 255],
    regions: [0, 1], finalLambda: 2, minimumPixels: 0 };
  const optimizer = createRegionOptimizer(input);
  input.bgra.fill(0);
  input.regions.fill(7);
  const snapshot = optimizer.snapshot();
  snapshot.indices.fill(7);
  snapshot.sums.fill(7);
  assert.deepEqual(Array.from(optimizer.snapshot().indices), [0, 1]);
  assert.deepEqual(Array.from(optimizer.snapshot().sums), [0, 0, 0, 1, 1, 1, 1, 1]);
  for (const invalid of [null, {}, { lambda: -1, mode: 'axis' }, { lambda: 1, mode: 'invalid' },
    { lambda: 1, mode: 'axis', minimumPixels: null }, { lambda: 1, mode: 'small', minimumPixels: -1 }]) {
    assert.throws(() => optimizer.iterate(invalid));
  }
  assert.deepEqual(Array.from(optimizer.snapshot().indices), [0, 1]);
  optimizer.iterate({ lambda: 2, mode: 'axis' });
  assert.throws(() => optimizer.projectPalette(), /Rebuild/);
  optimizer.rebuild();
  assert.throws(() => optimizer.projectPalette({ diagonalExtraction: 1 }), /boolean/);
  const palette = optimizer.projectPalette();
  assert.deepEqual(Array.from(palette), [Math.fround(128 * Math.fround(1 / 255)), Math.fround(128 * Math.fround(1 / 255)), Math.fround(128 * Math.fround(1 / 255)), 1]);
  assert.throws(() => optimizer.projectPalette(), /already/);
  optimizer.rebuild();
  assert.deepEqual(Array.from(optimizer.snapshot().colorIndices), [0]);
});

test('full segmentation validates parameters and rejects unfinished branches explicitly', () => {
  const input = structuredClone(segmentationReference.cases[0]);
  const before = structuredClone(input);
  segmentUnweighted(input);
  assert.deepEqual(input, before);
  for (const [key, value] of [['Shared::is_anti_aliased', 1], ['Segmenter::use_over_seg_diag_extr', 2],
    ['Segmenter::size_penalty_weight', 1], ['Segmenter::max_iterations', 0],
    ['Segmenter::lambda_initial', 0], ['Segmenter::rise_fraction', Infinity]]) {
    assert.throws(() => segmentUnweighted({ ...input, parameters: { ...input.parameters, [key]: value } }));
  }
  for (const invalid of [null, {}, { ...input, parameters: null }, { ...input, bgra: [0] }]) {
    assert.throws(() => segmentUnweighted(invalid));
  }
  for (const invalid of [{ ...input, regions: [0], finalLambda: 1, minimumPixels: -1 },
    { ...input, regions: [-1], finalLambda: 1, minimumPixels: 0 }]) {
    assert.throws(() => createRegionOptimizer(invalid));
  }
});

test('true presegmentation matches all 632 original base-region states', () => {
  assert.equal(presegmentationReference.cases.length, 632);
  for (const [index, fixture] of presegmentationReference.cases.entries()) {
    const state = presegmentRegions(fixture);
    for (const field of ['indices', 'counts', 'sums', 'colorIndices']) {
      assert.deepEqual(Array.from(state[field]), fixture.expected[field], `case ${index} ${field}`);
    }
    assert.equal(state.activeCount, fixture.expected.activeCount);
  }
});

test('complete automatic anti-alias segmentation matches all 240 original runs', () => {
  assert.equal(antiAliasReference.cases.length, 240);
  for (const [index, fixture] of antiAliasReference.cases.entries()) {
    const state = segment(fixture);
    for (const field of ['indices', 'counts', 'palette', 'colorIndices']) {
      assert.deepEqual(Array.from(state[field]), fixture.expected[field], `case ${index} ${field}`);
    }
  }
});

test('automatic segmentation owns output and validates controls before mutation', () => {
  const input = structuredClone(antiAliasReference.cases.find(fixture => fixture.width === 3));
  const before = structuredClone(input);
  const state = segment(input);
  for (const array of Object.values(state)) array.fill(7);
  assert.deepEqual(input, before);
  for (const invalid of [null, {}, { ...input, parameters: null },
    { ...input, parameters: { ...input.parameters, 'Shared::is_anti_aliased': 2 } },
    { ...input, parameters: { ...input.parameters, 'Segmenter::size_penalty_weight': 1 } },
    { ...input, parameters: { ...input.parameters, clusterColors: 'yes' } }]) assert.throws(() => segment(invalid));
  assert.deepEqual(input, before);
});

test('independent segmentation matches all 15 known-settings original pipeline stages', () => {
  assert.equal(pipelineReference.cases.length, 15);
  for (const fixture of pipelineReference.cases) {
    const state = segment(fixture);
    for (const field of ['indices', 'counts', 'palette', 'colorIndices']) {
      assert.deepEqual(Array.from(state[field]), fixture.expected[field], `${fixture.id} ${field}`);
    }
  }
});

test('clustered anti-alias segmentation matches all 240 original complete runs', () => {
  assert.equal(clusteredReference.cases.length, 240);
  for (const [index, fixture] of clusteredReference.cases.entries()) {
    const state = segment(fixture);
    for (const field of ['indices', 'counts', 'palette', 'colorIndices']) {
      assert.deepEqual(Array.from(state[field]), fixture.expected[field], `case ${index} clustered ${field}`);
    }
  }
});

test('complete shadow dispatch matches 578 native automatic and supplied-palette runs', async () => {
  const path = new URL('./fixtures/shadow-segmentation-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let count = 0, enabled = 0, supplied = 0, clustered = 0, positive = 0, accepted = 0, rejected = 0, summary;
  const controls = new Set(), profiles = new Set();
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') {
      assert.equal(frame.wholeNativeResponseRepeatIdentical, true);
      assert.equal(frame.wholeHardwareTraceRepeatIdentical, true);
      assert.equal(frame.tracedAndUntracedCompleteResponsesIdentical, true);
      continue;
    }
    if (frame.type === 'summary') { summary = frame; continue; }
    assert.equal(frame.type, 'case'); assert.equal(frame.randomState, 1);
    const original = structuredClone({ parameters:frame.parameters, bgra:frame.bgra, palette:frame.palette });
    const state = segment(frame);
    for (const field of ['indices','counts','palette','colorIndices']) {
      assert.deepEqual(Array.from(state[field]), frame.expected[field], `${frame.id} complete native ${field}`);
    }
    assert.deepEqual({ parameters:frame.parameters, bgra:frame.bgra, palette:frame.palette }, original,
      `${frame.id} input ownership`);
    if (frame.palette !== undefined) assert.deepEqual(Array.from(state.palette), frame.palette, `${frame.id} supplied palette`);
    count++; enabled += Number(frame.parameters.shadowing); supplied += Number(frame.palette !== undefined);
    clustered += Number(frame.parameters.clusterColors); positive += Number(frame.nativeShadowDecisions.length > 0);
    for (const decision of frame.nativeShadowDecisions) {
      accepted += Number(decision.accepted); rejected += Number(!decision.accepted);
    }
    controls.add(frame.parameters.shadowControl);
    profiles.add(frame.id.match(/-profile-(\d+)-/)[1]);
  }
  assert.equal(count, 578); assert.equal(enabled, 476); assert.equal(supplied, 340); assert.equal(clustered, 323);
  assert.equal(positive, 55); assert.equal(accepted, 81); assert.equal(rejected, 6);
  assert.deepEqual([...controls].sort((a, b) => a - b), [0,1,2,9]); assert.equal(profiles.size, 5);
  assert.deepEqual(summary, { type:'summary', cases:count, shadowEnabledCases:enabled,
    suppliedPaletteCases:supplied, clusteredCases:clustered, shadowPositiveImages:positive,
    acceptedShadowRegions:accepted, rejectedShadowRegions:rejected });
});

test('enabled shadow dispatch requires explicit native controls and preserves caller input', () => {
  const input = structuredClone(antiAliasReference.cases.find(frame => frame.width === 3));
  const original = structuredClone(input);
  for (const change of [{ shadowing:'yes' }, { shadowing:1 }, { shadowing:true },
    { shadowing:true, shadowControl:0.5 }, { shadowing:true, shadowControl:2147483648 },
    { shadowing:true, shadowControl:-2147483649 }, { shadowing:true, shadowControl:NaN }]) {
    assert.throws(() => segment({ ...input, parameters:{ ...input.parameters, ...change } }), /shadow/);
  }
  assert.deepEqual(input, original);
});
