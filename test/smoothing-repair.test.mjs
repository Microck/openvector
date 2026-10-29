import assert from 'node:assert/strict';
import { test } from 'node:test';
import repairs from './fixtures/smoothing-repair-reference.json' with { type:'json' };
import optimizers from './fixtures/smoothing-repair-optimizer-reference.json' with { type:'json' };
import withoutRepair from './fixtures/smoothing-image-optimizer-reference.json' with { type:'json' };
import neighborhoods from './fixtures/contour-neighborhoods-reference.json' with { type:'json' };
import { createContourRepair } from '../src/smoothing-repair.mjs';
import { createContourImageModel } from '../src/image-measurement.mjs';
import { readImageKernel } from '../src/image-measurement-kernel.mjs';
import { smoothingOptimize, prepareContourNeighborhoods } from '../src/smoothing.mjs';
import { extractContours } from '../src/contours.mjs';

const imageFor = fixture => createContourImageModel({ ...fixture, indices:fixture.segmentation.indices });

function originalNeighborhoods(fixture) {
  const geometry = extractContours({ ...fixture.segmentation, width:fixture.width, height:fixture.height,
    antiAlias:Boolean(fixture.antiAlias) });
  const prepared = prepareContourNeighborhoods({ geometry });
  assert.deepEqual(prepared.nearbyEdges, fixture.nearbyEdges);
  return prepared.nearbyEdges;
}

test('independent neighborhood construction matches all 2814 native ordered lists and node flags', () => {
  assert.equal(neighborhoods.cases.length, 2814);
  let nodes = 0, associations = 0;
  const variants = new Set();
  for (const [index, fixture] of neighborhoods.cases.entries()) {
    const actual = prepareContourNeighborhoods(fixture);
    assert.deepEqual({ ...actual, flags:[...actual.flags] }, fixture.expected, `neighborhood ${index}`);
    nodes += actual.flags.length;
    associations += actual.nearbyEdges.reduce((sum, list) => sum + list.length, 0);
    if (fixture.neighborhoodVariant !== undefined) variants.add(fixture.neighborhoodVariant);
  }
  assert.equal(nodes, 64518);
  assert.equal(associations, 1045003);
  assert.equal(variants.size, 12);
});

test('neighborhood preparation owns output and validates geometry before building its index', () => {
  const fixture = neighborhoods.cases[0], before = structuredClone(fixture);
  const actual = prepareContourNeighborhoods(fixture);
  actual.flags.fill(255);
  for (const edges of actual.nearbyEdges) { for (const edge of edges) edge.fill(999); edges.push([0, 0]); }
  assert.deepEqual(fixture, before);
  const typed = structuredClone(fixture);
  for (const region of typed.geometry.regions) region.contour = Int32Array.from(region.contour);
  const prepared = prepareContourNeighborhoods(typed);
  assert.deepEqual({ ...prepared, flags:[...prepared.flags] }, fixture.expected);
  for (const invalid of [null, {}, { geometry:{ nodes:[], regions:[] } },
    { geometry:{ ...fixture.geometry, nodes:fixture.geometry.nodes.map(node => ({ ...node, flags:256 })) } },
    { geometry:{ ...fixture.geometry, nodes:fixture.geometry.nodes.map(node => ({ ...node, x:NaN })) } },
    { geometry:{ ...fixture.geometry, regions:[{ contour:[0, 1, fixture.geometry.nodes.length] }] } },
    { geometry:{ ...fixture.geometry, regions:[{ contour:[0, 1] }] } },
    { geometry:{ nodes:Array.from({ length:60 }, () => ({ x:0, y:0, flags:0 })), regions:[{ contour:[0, 1, 2] }] } },
  ]) assert.throws(() => prepareContourNeighborhoods(invalid));
  assert.deepEqual(fixture, before);
});

test('contour repair matches every native state and image-error selection across 1425 persistent sequences', () => {
  assert.equal(repairs.cases.length, 1425);
  let states = 0, imageCalls = 0, nonemptyErrors = 0;
  const kinds = new Set();
  for (const [index, fixture] of repairs.cases.entries()) {
    const state = { geometry:structuredClone(fixture.geometry), penaltyCounts:fixture.penaltyCounts.slice(),
      points:fixture.geometry.nodes.map(node => [node.x, node.y]) };
    const repair = createContourRepair(state, originalNeighborhoods(fixture));
    const image = readImageKernel(imageFor(fixture), fixture.geometry);
    assert.equal(fixture.repairInterval, 10);
    for (const [step, action] of fixture.actions.entries()) {
      state.points = state.points.map((point, node) => action.positions.slice(node * 2, node * 2 + 2));
      let changed;
      if (action.kind === 3) {
        image.energy(action.positions);
        assert.deepEqual(image.invalidRegions(), action.invalidRegions, `invalid regions ${index}/${step}`);
        changed = Number(repair.invalid(image.takeInvalidRegions()));
        assert.deepEqual(image.invalidRegions(), []);
        imageCalls++;
        if (action.invalidRegions.length > 0) nonemptyErrors++;
      } else changed = action.kind === 0 ? repair.all() : action.kind === 1 ? repair.region(action.region) : repair.decay();
      assert.deepEqual({ changed, ...repair.snapshot() }, fixture.expected[step], `repair state ${index}/${step}`);
      kinds.add(action.kind);
      states++;
    }
    // Three final decays retain one instead of erasing the inversion penalty.
    assert.ok(state.penaltyCounts.every(count => count <= 1));
  }
  assert.deepEqual([...kinds].sort(), [0, 1, 2, 3]);
  assert.equal(states, 14106);
  assert.equal(imageCalls, 2850);
  assert.equal(nonemptyErrors, 274);
});

test('anti-alias repair callbacks preserve all 1029 complete native optimizer trajectories', () => {
  assert.equal(optimizers.cases.length, 1029);
  let changedPositions = 0, changedRestarts = 0, longRuns = 0;
  const phases = new Set(), lineTypes = new Set();
  for (const [index, fixture] of optimizers.cases.entries()) {
    const before = structuredClone(fixture);
    const actual = smoothingOptimize({ ...fixture, nearbyEdges:originalNeighborhoods(fixture), imageModel:imageFor(fixture) });
    assert.deepEqual({ ...actual, positions:[...actual.positions], storedGradient:[...actual.storedGradient],
      direction:[...actual.direction] }, fixture.expected, `repaired optimizer ${index}`);
    assert.deepEqual(fixture, before);
    const original = withoutRepair.cases[fixture.sourceImageOptimizer].expected;
    if (actual.positions.some((value, channel) => value !== original.positions[channel])) changedPositions++;
    if (actual.restarts !== original.restarts) changedRestarts++;
    if (actual.iterations >= fixture.repairInterval) longRuns++;
    phases.add(fixture.phase);
    lineTypes.add(fixture.parameters[`ContourSmoother::phase_${fixture.phase}.cg_line_search_type`]);
  }
  assert.deepEqual([...phases].sort(), [0, 1, 2]);
  assert.deepEqual([...lineTypes].sort(), [0, 5]);
  assert.ok(changedPositions > 0 && changedRestarts > 0 && longRuns > 0);
});

test('prepared repair validates associations before touching image state and owns changed flags and penalties', () => {
  const fixture = optimizers.cases.find(candidate => candidate.geometry.regions.length > 1);
  const imageModel = imageFor(fixture), positions = fixture.geometry.nodes.flatMap(node => [node.x, node.y]);
  const initial = imageModel.evaluate(positions);
  const invalidEdges = structuredClone(fixture.nearbyEdges);
  invalidEdges[0] = [[fixture.geometry.regions.length, 0]];
  for (const invalid of [
    { nearbyEdges:[] }, { nearbyEdges:invalidEdges }, { repairInterval:0 }, { repairInterval:1.5 },
    { nearbyEdges:fixture.nearbyEdges.map(() => [[0, fixture.geometry.regions[0].contour.length]]) },
  ]) assert.throws(() => smoothingOptimize({ ...fixture, imageModel, ...invalid }));
  assert.deepEqual(imageModel.evaluate(positions), initial);
  const before = structuredClone(fixture);
  const actual = smoothingOptimize({ ...fixture, imageModel:imageFor(fixture) });
  actual.repairState.flags.fill(255);
  actual.repairState.penaltyCounts.fill(255);
  assert.deepEqual(fixture, before);
});
