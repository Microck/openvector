import assert from 'node:assert/strict';
import projectionReference from './fixtures/subpixel-palette-projection-reference.json' with { type: 'json' };
import { test } from 'node:test';

test('fixed-palette projection matches all 1644 original states and assignment thresholds', () => {
  assert.equal(projectionReference.cases.length, 1644);
  for (const [index, fixture] of projectionReference.cases.entries()) {
    const model = createSubpixelModel(fixture);
    assertSubpixelState(model.snapshot(), fixture.before, `projection case ${index} before`);
    model.projectPalette(fixture);
    assertSubpixelState(model.snapshot(), fixture.expected, `projection case ${index} after`);
    // A changed palette cannot replace an already assigned color identifier.
    model.projectPalette({ ...fixture, palette:[1, 0, 1, 1] });
    assertSubpixelState(model.snapshot(), fixture.expected, `projection case ${index} repeated`);
  }
});

test('fixed-palette projection retains the original double-distance winner and duplicate ties', () => {
  const closeCases = projectionReference.cases.filter(fixture => fixture.purpose?.startsWith('Separate'));
  assert.equal(closeCases.length, 12);
  for (const fixture of closeCases) {
    assert.equal(fixture.expected.regions[0].colorIndex, 1);
    const model = createSubpixelModel(fixture);
    model.projectPalette(fixture);
    assert.equal(model.snapshot().regions[0].colorIndex, 1);
  }
  const duplicateCases = projectionReference.cases.filter(fixture => fixture.purpose?.startsWith('Check'));
  assert.equal(duplicateCases.length, 12);
  for (const fixture of duplicateCases) {
    assert.equal(fixture.floatingControlWord, 0x027f, 'Original x87 arithmetic uses 53-bit precision.');
    assert.equal(fixture.expected.regions[0].colorIndex, 0);
  }
});

test('fixed-palette projection validates controls and does not borrow caller colors', () => {
  const input = structuredClone(projectionReference.cases.find(fixture => fixture.width === 5 && fixture.minimumInteriorPixels === 0));
  input.palette = Float32Array.from(input.palette);
  const model = createSubpixelModel(input);
  const before = model.snapshot();
  for (const invalid of [null, {}, { ...input, minimumInteriorPixels:-1 },
    { ...input, minimumInteriorPixels:1.5 }, { ...input, minimumInteriorPixels:2147483648 },
    { ...input, palette:[] }, { ...input, palette:[NaN, 0, 0, 1] }]) {
    assert.throws(() => model.projectPalette(invalid));
  }
  assert.deepEqual(model.snapshot(), before);
  model.projectPalette(input);
  const projected = model.snapshot();
  input.palette.fill(0);
  assert.deepEqual(model.snapshot(), projected);
});
import reference from './fixtures/subpixel-statistics-reference.json' with { type: 'json' };
import mergeReference from './fixtures/subpixel-merge-cost-reference.json' with { type: 'json' };
import appliedReference from './fixtures/subpixel-merge-reference.json' with { type: 'json' };
import passReference from './fixtures/subpixel-pass-reference.json' with { type: 'json' };
import rebuildReference from './fixtures/subpixel-rebuild-reference.json' with { type: 'json' };
import optimizationReference from './fixtures/subpixel-optimization-reference.json' with { type: 'json' };
import colorSelectionReference from './fixtures/subpixel-color-selection-reference.json' with { type: 'json' };
import { buildSubpixelStatistics, createSubpixelModel, optimizeSubpixel } from '../src/subpixel.mjs';

function assertSubpixelState(actual, expected, context) {
  assert.deepEqual(Array.from(actual.indices), expected.indices, `${context} indices`);
  assert.deepEqual(Array.from(actual.costs), expected.costs, `${context} costs`);
  assert.equal(actual.regions.length, expected.regions.length);
  for (const [region, statistic] of actual.regions.entries()) {
    const plain = Object.fromEntries(Object.entries(statistic).map(([key, value]) =>
      [key, ArrayBuffer.isView(value) ? Array.from(value) : value]));
    assert.deepEqual(plain, expected.regions[region], `${context} region ${region}`);
  }
}

test('subpixel rebuilds match all 540 original statistic and energy snapshots', () => {
  assert.equal(reference.cases.length, 540);
  for (const [index, fixture] of reference.cases.entries()) {
    const actual = buildSubpixelStatistics(fixture);
    assertSubpixelState(actual, fixture.expected, `case ${index}`);
  }
});

test('subpixel rebuild owns output storage and validates explicit controls', () => {
  const input = structuredClone(reference.cases.find(fixture => fixture.width === 5));
  input.bgra = Uint8Array.from(input.bgra);
  input.regions = Int32Array.from(input.regions);
  const before = structuredClone(input);
  const actual = buildSubpixelStatistics(input);
  actual.indices.fill(7);
  actual.costs.fill(7);
  for (const statistic of actual.regions) {
    statistic.sums.fill(7);
    statistic.color.fill(7);
    statistic.boundary.fill(7);
  }
  assert.deepEqual(input, before);
  for (const invalid of [null, {}, { ...input, neighborhood: 'invalid' },
    { ...input, parameters: null }, { ...input, parameters: {} }, { ...input, width: 0 },
    { ...input, bgra: [0] }, { ...input, regions: [-1] },
    { ...input, parameters: { ...input.parameters, 'SubPixelSegmentation::self_weight_eps': Infinity } }]) {
    assert.throws(() => buildSubpixelStatistics(invalid));
  }
});

test('directed subpixel merge proposals match original bounded energy decisions', () => {
  assert.equal(mergeReference.cases.length, 540);
  let rejectedBounds = 0, recomputed = 0;
  for (const [index, fixture] of mergeReference.cases.entries()) {
    const model = createSubpixelModel(fixture);
    const before = model.snapshot();
    for (const [query, proposal] of fixture.queries.entries()) {
      assert.equal(model.mergeCost(proposal), proposal.expected.cost, `case ${index} query ${query}`);
      if (proposal.expected.lowerBound > Math.fround(proposal.lambda)) rejectedBounds++;
      else recomputed++;
    }
    assert.deepEqual(model.snapshot(), before);
  }
  assert.ok(rejectedBounds > 0 && recomputed > 0, 'Exercise both native evaluation paths.');
});

test('subpixel model owns persistent pixels and snapshots and validates proposals', () => {
  const fixture = mergeReference.cases.find(input => input.width === 5 && input.queries.some(query => query.source !== query.target));
  const input = structuredClone(fixture);
  const model = createSubpixelModel(input);
  const before = model.snapshot();
  input.bgra.fill(0);
  input.regions.fill(7);
  input.parameters['SubPixelSegmentation::self_weight_eps'] = 99;
  const snapshot = model.snapshot();
  snapshot.indices.fill(7);
  snapshot.costs.fill(7);
  for (const statistic of snapshot.regions) {
    statistic.color.fill(7);
    statistic.interiorSums.fill(7);
    statistic.boundary.fill(7);
  }
  for (const proposal of fixture.queries) assert.equal(model.mergeCost(proposal), proposal.expected.cost);
  for (const invalid of [null, {}, { source: -1, target: 0, lambda: 0 },
    { source: 0, target: before.regions.length, lambda: 0 }, { source: 0.5, target: 0, lambda: 0 },
    { source: 0, target: 0, lambda: -1 }, { source: 0, target: 0, lambda: Infinity },
    { source: 0, target: 0, lambda: 1e100 }]) assert.throws(() => model.mergeCost(invalid));
  assert.deepEqual(model.snapshot(), before);
});

test('applied subpixel merges match original incremental states and retained colors', () => {
  assert.equal(appliedReference.cases.length, 340);
  for (const [index, fixture] of appliedReference.cases.entries()) {
    const model = createSubpixelModel(fixture);
    for (const [step, merge] of fixture.merges.entries()) {
      model.merge(merge);
      assert.equal(model.mergeCost(merge.proposal), merge.proposal.expected, `case ${index} step ${step} subsequent proposal`);
      const actual = model.snapshot();
      assertSubpixelState(actual, merge.expected, `case ${index} step ${step}`);
    }
  }
});

test('invalid applied merges preserve model state and reject emptied slots', () => {
  const fixture = appliedReference.cases[0];
  const model = createSubpixelModel(fixture);
  const before = model.snapshot();
  for (const invalid of [null, {}, { source: 0, target: 0 }, { source: -1, target: 0 },
    { source: 0, target: before.regions.length }]) assert.throws(() => model.merge(invalid));
  assert.deepEqual(model.snapshot(), before);
  const merge = fixture.merges[0];
  model.merge(merge);
  const after = model.snapshot();
  assert.throws(() => model.merge(merge));
  assert.throws(() => model.mergeCost({ ...merge, lambda: 1 }));
  assert.deepEqual(model.snapshot(), after);
});

test('ordered subpixel merge-selection passes match original complete states', () => {
  assert.equal(passReference.cases.length, 540);
  let changingPasses = 0;
  for (const [index, fixture] of passReference.cases.entries()) {
    const model = createSubpixelModel(fixture);
    for (const [step, controls] of fixture.passes.entries()) {
      const merges = model.iterate(controls);
      assert.equal(merges, controls.expectedMerges, `case ${index} step ${step} merge count`);
      assertSubpixelState(model.snapshot(), controls.expected, `case ${index} step ${step}`);
      if (merges > 0) changingPasses++;
    }
  }
  assert.ok(changingPasses > 0, 'The pass corpus must contain accepted merges.');
});

test('invalid subpixel pass controls leave the model unchanged', () => {
  const fixture = passReference.cases.find(input => input.width === 5);
  const model = createSubpixelModel(fixture);
  const before = model.snapshot();
  const controls = fixture.passes[0];
  for (const invalid of [null, {}, { ...controls, lambda: -1 }, { ...controls, lambda: Infinity },
    { ...controls, lambda: 1e100 }, { ...controls, maximumSourcePixels: -1 },
    { ...controls, maximumTargetPixels: 2147483648 }, { ...controls, maximumSourcePixels: 0.5 },
    { ...controls, sourceCount: 'invalid' }]) assert.throws(() => model.iterate(invalid));
  assert.deepEqual(model.snapshot(), before);
});

test('subpixel rebuilds after mutations match original neighborhood changes and retained cache', () => {
  assert.equal(rebuildReference.cases.length, 540);
  for (const [index, fixture] of rebuildReference.cases.entries()) {
    const model = createSubpixelModel(fixture);
    for (const merge of fixture.merges) model.merge(merge);
    assertSubpixelState(model.snapshot(), fixture.beforeCleanup, `case ${index} before rebuild`);
    for (const [step, controls] of fixture.rebuilds.entries()) {
      model.rebuild(controls);
      assertSubpixelState(model.snapshot(), controls.expected, `case ${index} rebuild ${step}`);
    }
  }
});

test('original thin-region cleanup leaves valid subpixel states unchanged', () => {
  let thinRegions = 0;
  for (const fixture of rebuildReference.cases) {
    assert.deepEqual(fixture.beforeCleanup, fixture.afterCleanup);
    for (const [region, statistic] of fixture.beforeCleanup.regions.entries()) {
      if (statistic.count === 0 || statistic.interiorCount !== 0) continue;
      thinRegions++;
      assert.ok(statistic.boundary.some(pixel => fixture.beforeCleanup.indices[pixel] !== region),
        'A valid zero-interior region has an outside boundary pixel that triggers the original rejection.');
    }
  }
  assert.ok(thinRegions > 0);
});

test('invalid subpixel rebuild controls preserve owned state', () => {
  const model = createSubpixelModel(rebuildReference.cases[0]);
  const before = model.snapshot();
  for (const invalid of [null, {}, { neighborhood: 'invalid' }]) assert.throws(() => model.rebuild(invalid));
  assert.deepEqual(model.snapshot(), before);
});

test('complete unclustered subpixel schedules match original final states', () => {
  assert.equal(optimizationReference.cases.length, 432);
  for (const [index, fixture] of optimizationReference.cases.entries()) {
    assertSubpixelState(optimizeSubpixel(fixture), fixture.expected, `case ${index} complete schedule`);
  }
});

test('subpixel optimization preserves caller input and validates stage controls', () => {
  const input = structuredClone(optimizationReference.cases.find(fixture => fixture.width === 5));
  const before = structuredClone(input);
  const state = optimizeSubpixel(input);
  state.indices.fill(7);
  state.costs.fill(7);
  for (const statistic of state.regions) statistic.color.fill(7);
  assert.deepEqual(input, before);
  for (const invalid of [null, {}, { ...input, parameters: null },
    { ...input, parameters: { ...input.parameters, 'SubPixelSegmenter::lambda_f': Infinity } },
    { ...input, parameters: { ...input.parameters, 'SubPixelSegmenter::lambda_f': -1 } },
    { ...input, parameters: { ...input.parameters, 'SubPixelSegmenter::use_connected_24_iteration': 2 } }]) {
    assert.throws(() => optimizeSubpixel(invalid));
  }
});

test('unrounded bounded boundary costs match all 7832 original evaluations', () => {
  let evaluations = 0;
  for (const [index, fixture] of colorSelectionReference.cases.entries()) {
    const model = createSubpixelModel(fixture);
    assertSubpixelState(model.snapshot(), fixture.before, `case ${index} initial state`);
    for (const [query, proposal] of fixture.queries.entries()) {
      assert.equal(model.boundaryCost(proposal), proposal.expected, `case ${index} query ${query}`);
      evaluations++;
    }
    assertSubpixelState(model.snapshot(), fixture.before, `case ${index} unchanged state`);
  }
  assert.equal(evaluations, 7832);
});

test('color selection matches all 548 original states and top-100 candidate ordering', () => {
  assert.equal(colorSelectionReference.cases.length, 548);
  let changedColors = 0, cappedCandidateCases = 0;
  for (const [index, fixture] of colorSelectionReference.cases.entries()) {
    const model = createSubpixelModel(fixture);
    model.selectColors({ margin: fixture.margin });
    assertSubpixelState(model.snapshot(), fixture.expected, `case ${index} selected colors`);
    changedColors += fixture.expected.regions.filter((statistic, region) =>
      statistic.color.some((value, channel) => value !== fixture.before.regions[region].color[channel])).length;
    if (fixture.before.regions.filter(statistic => statistic.interiorCount >= 3).length > 100) cappedCandidateCases++;
  }
  assert.ok(changedColors > 0 && cappedCandidateCases > 0, 'Exercise actual color replacements and the native candidate cap.');
});

test('color selection and boundary evaluation validate controls without changing owned state', () => {
  const input = structuredClone(colorSelectionReference.cases.find(fixture => fixture.width === 5));
  const beforeInput = structuredClone(input);
  const model = createSubpixelModel(input);
  const before = model.snapshot();
  for (const invalid of [null, {}, { margin: -1 }, { margin: Infinity }, { margin: 1e100 }]) {
    assert.throws(() => model.selectColors(invalid));
  }
  for (const invalid of [null, {}, { source: 0, target: 0, budget: -1 },
    { source: 0, target: 0, budget: Infinity }, { source: -1, target: 0, budget: 0 }]) {
    assert.throws(() => model.boundaryCost(invalid));
  }
  assert.deepEqual(model.snapshot(), before);
  model.selectColors({ margin: input.margin });
  assert.deepEqual(input, beforeInput);
});
