import assert from 'node:assert/strict';
import { test } from 'node:test';
import preadjustments from './fixtures/smoothing-preadjustment-reference.json' with { type:'json' };
import initializations from './fixtures/smoothing-initialization-reference.json' with { type:'json' };
import mixed from './fixtures/smoothing-initialization-mixed-reference.json' with { type:'json' };
import preparations from './fixtures/contour-preparation-reference.json' with { type:'json' };
import { CoreSession } from '../src/settings.mjs';
import { CoreSession as PaletteCoreSession } from '../src/palette.mjs';
import { preadjustContours, prepareContourSmoothing, selectNearBorderNodes } from '../src/smoothing.mjs';
import { extractContours } from '../src/contours.mjs';

const prepared = fixture => ({ ...fixture, indices:fixture.segmentation.indices, palette:fixture.segmentation.palette });
const rawGeometry = fixture => extractContours({ ...fixture.segmentation, width:fixture.width,
  height:fixture.height, antiAlias:fixture.antiAlias });

test('raw contour preparation matches all 2700 native geometries, fixed caches and neighborhoods', () => {
  assert.equal(preparations.cases.length, 2700);
  for (const [index, fixture] of preparations.cases.entries()) {
    const actual = prepareContourSmoothing({ ...fixture, geometry:rawGeometry(fixture) });
    assert.deepEqual({ ...actual, referencePositions:[...actual.referencePositions], penaltyCounts:[...actual.penaltyCounts],
      borderCounts:[...actual.borderCounts], nearBorderNodes:[...actual.nearBorderNodes] }, fixture.expected, `preparation ${index}`);
  }
});

test('near-border selection matches original strict comparisons on 96 threshold-adjacent inputs', () => {
  let probes = 0;
  const remainders = new Set();
  for (const [index, fixture] of preparations.cases.entries()) {
    assert.deepEqual([...selectNearBorderNodes(fixture)], fixture.expectedNearBorderNodes, `border selection ${index}`);
    if (fixture.thresholdProbe) {
      if (fixture.thresholdProbe.precision === 'float32') assert.equal(Math.fround(fixture.thresholdProbe.value), fixture.thresholdProbe.value);
      probes++;
      remainders.add((fixture.geometry.nodes.length - fixture.borderCounts.reduce((sum, count) => sum + count, 0)) % 4);
    }
  }
  assert.equal(probes, 96);
  assert.deepEqual([...remainders].sort(), [0, 1, 2, 3]);
});

test('independent preparation composes with all 2604 retained-session native initializations', () => {
  const session = new CoreSession();
  for (const [index, fixture] of initializations.cases.entries()) {
    const caches = prepareContourSmoothing({ ...fixture, geometry:rawGeometry(fixture) });
    const actual = session.initializeContours({ ...prepared(fixture), ...caches });
    assert.deepEqual({ positions:[...actual.positions], randomState:session.randomState }, fixture.expected, `prepared initialization ${index}`);
  }
});

test('geometry preparation owns its caches and rejects moved or misordered raw inputs', () => {
  const fixture = preparations.cases.find(candidate => candidate.antiAlias && candidate.geometry.nodes.length > 10);
  const input = { ...fixture, geometry:rawGeometry(fixture) }, before = structuredClone(input);
  const actual = prepareContourSmoothing(input);
  actual.geometry.nodes[0].x = 999;
  actual.referencePositions.fill(999);
  actual.penaltyCounts.fill(255);
  actual.borderCounts.fill(999);
  actual.nearBorderNodes.fill(999);
  actual.nearbyEdges[0].push([999, 999]);
  assert.deepEqual(input, before);
  const moved = structuredClone(input.geometry);
  moved.nodes.at(-1).x += 0.125;
  const misordered = structuredClone(input.geometry);
  [misordered.nodes[0], misordered.nodes[misordered.nodes.length - 1]] = [misordered.nodes.at(-1), misordered.nodes[0]];
  const sparse = structuredClone(input.geometry);
  delete sparse.nodes[0];
  for (const invalid of [{ antiAlias:undefined }, { width:1 }, { geometry:moved }, { geometry:misordered }, { geometry:sparse }]) {
    assert.throws(() => prepareContourSmoothing({ ...input, ...invalid }));
  }
  for (const invalid of [{ borderCounts:[2, 2, 999, 0] }, { borderCounts:[2, 2, -1, 0] }, { geometry:sparse }]) {
    assert.throws(() => selectNearBorderNodes({ ...input, ...invalid }));
  }
  assert.deepEqual(input, before);
});

test('blend-driven preadjustment matches all 2604 original position arrays without random consumption', () => {
  assert.equal(preadjustments.cases.length, 2604);
  let moved = 0;
  for (const [index, fixture] of preadjustments.cases.entries()) {
    const actual = preadjustContours(prepared(fixture));
    assert.deepEqual([...actual.positions], fixture.expected.positions, `preadjustment ${index}`);
    assert.equal(fixture.randomStateBefore, 1);
    assert.equal(fixture.expected.randomState, 1);
    for (const [node, original] of fixture.geometry.nodes.entries()) {
      if (actual.positions[node * 2] !== original.x || actual.positions[node * 2 + 1] !== original.y) moved++;
    }
  }
  assert.equal(moved, 37310);
});

test('contour initialization matches all 2604 original calls in one retained random session', () => {
  assert.equal(initializations.cases.length, 2604);
  assert.equal(CoreSession, PaletteCoreSession);
  const session = new CoreSession();
  let draws = 0, plainCases = 0, zeroRangeConsumes = 0;
  for (const [index, fixture] of initializations.cases.entries()) {
    assert.equal(session.randomState, fixture.randomStateBefore);
    const actual = session.initializeContours(prepared(fixture));
    assert.deepEqual({ positions:[...actual.positions], randomState:session.randomState }, fixture.expected, `initialization ${index}`);
    const border = fixture.borderCounts.reduce((sum, count) => sum + count, 0);
    for (let node = 0; node < border; node++) {
      assert.equal(actual.positions[node * 2], fixture.geometry.nodes[node].x);
      assert.equal(actual.positions[node * 2 + 1], fixture.geometry.nodes[node].y);
    }
    if (fixture.antiAlias) {
      draws += (fixture.geometry.nodes.length - border) * 2;
      if (fixture.amplitudeVariant === 0 && session.randomState !== fixture.randomStateBefore) zeroRangeConsumes++;
    } else {
      plainCases++;
      assert.equal(session.randomState, fixture.randomStateBefore);
      assert.deepEqual([...actual.positions], fixture.geometry.nodes.flatMap(node => [node.x, node.y]));
    }
  }
  assert.equal(draws, 111576);
  assert.equal(plainCases, 1157);
  assert.equal(zeroRangeConsumes, 4);
  assert.equal(session.randomState, 8900);
});

test('palette generation and initialization share one stream through all 110 original interleaved runs', () => {
  assert.equal(mixed.cases.length, 110);
  const session = new CoreSession();
  for (const [index, fixture] of mixed.cases.entries()) {
    assert.equal(session.randomState, fixture.warmupStateBefore);
    const palette = session.generateUnweighted(fixture.paletteWarmup);
    assert.deepEqual({ palette:[...palette.palette], indices:[...palette.indices], weights:[...palette.weights],
      randomState:session.randomState }, fixture.warmupExpected, `palette warmup ${index}`);
    assert.equal(session.randomState, fixture.randomStateBefore);
    const actual = session.initializeContours(prepared(fixture));
    assert.deepEqual({ positions:[...actual.positions], randomState:session.randomState }, fixture.expected, `mixed initialization ${index}`);
  }
  assert.equal(session.randomState, 5295);
});

test('initialization validates before consuming random state and owns coordinates', () => {
  const fixture = initializations.cases.find(candidate => candidate.antiAlias &&
    candidate.geometry.nodes.length > candidate.borderCounts.reduce((sum, count) => sum + count, 0));
  const input = prepared(fixture), before = structuredClone(input), session = new CoreSession();
  const invalidReference = fixture.referencePositions.slice();
  invalidReference[0] = '0';
  const sparseNodes = fixture.geometry.nodes.slice();
  delete sparseNodes[0];
  const sparseRegions = fixture.geometry.regions.slice();
  delete sparseRegions[0];
  for (const invalid of [
    { referencePositions:invalidReference }, { referencePositions:fixture.referencePositions.map(() => 0) },
    { referencePositions:fixture.referencePositions.map(() => Infinity) }, { borderCounts:[2, 2, 0, 999999] },
    { indices:fixture.segmentation.indices.map(() => fixture.geometry.regions.length) }, { palette:[NaN, 0, 0, 1] },
    { geometry:{ ...fixture.geometry, nodes:sparseNodes } },
    { geometry:{ ...fixture.geometry, regions:sparseRegions } },
    { geometry:{ ...fixture.geometry, nodes:fixture.geometry.nodes.map(node => ({ ...node, x:Infinity })) } },
    { parameters:{ ...fixture.parameters, 'ContourSmoother::perturbation_range':-0.1 } },
    { parameters:{ ...fixture.parameters, 'Shared::is_anti_aliased':2 } },
  ]) {
    assert.throws(() => session.initializeContours({ ...input, ...invalid }));
    assert.equal(session.randomState, 1);
  }
  const actual = session.initializeContours(input);
  actual.positions.fill(999);
  assert.deepEqual(input, before);
  assert.notEqual(session.randomState, 1);
});
