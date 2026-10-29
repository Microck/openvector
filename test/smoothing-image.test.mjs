import assert from 'node:assert/strict';
import { test } from 'node:test';
import objectives from './fixtures/smoothing-image-objective-reference.json' with { type:'json' };
import lines from './fixtures/smoothing-image-line-search-reference.json' with { type:'json' };
import optimizers from './fixtures/smoothing-image-optimizer-reference.json' with { type:'json' };
import sequences from './fixtures/smoothing-image-objective-sequence-reference.json' with { type:'json' };
import { createContourImageModel } from '../src/image-measurement.mjs';
import { smoothingObjective, smoothingLineSearch, smoothingOptimize } from '../src/smoothing.mjs';

const imageFor = fixture => createContourImageModel({ ...fixture, indices:fixture.segmentation.indices });
const geometryAt = (geometry, positions) => ({ ...geometry,
  nodes:geometry.nodes.map((node, index) => ({ ...node, x:positions[index * 2], y:positions[index * 2 + 1] })) });

test('combined image smoothing objectives match all 2799 original energies and gradients exactly', () => {
  assert.equal(objectives.cases.length, 2799);
  for (const [index, fixture] of objectives.cases.entries()) {
    const actual = smoothingObjective({ ...fixture, imageModel:imageFor(fixture) });
    assert.deepEqual({ ...actual, gradient:[...actual.gradient] }, fixture.expected, `image objective ${index}`);
  }
});

test('image-based quadratic line searches match all 3312 original controller results exactly', () => {
  assert.equal(lines.cases.length, 3312);
  for (const [index, fixture] of lines.cases.entries()) {
    const actual = smoothingLineSearch({ ...fixture, imageModel:imageFor(fixture) });
    assert.deepEqual({ ...actual, positions:[...actual.positions] }, fixture.expected, `image line search ${index}`);
  }
});

test('complete prepared image optimizers match all 1029 native runs with both profile line-search modes', () => {
  assert.equal(optimizers.cases.length, 1029);
  const phases = new Set(), lineTypes = new Set();
  let earlyStops = 0, restarts = 0;
  for (const [index, fixture] of optimizers.cases.entries()) {
    const actual = smoothingOptimize({ ...fixture, imageModel:imageFor(fixture) });
    assert.deepEqual({ ...actual, positions:[...actual.positions], storedGradient:[...actual.storedGradient],
      direction:[...actual.direction] }, fixture.expected, `image optimizer ${index}`);
    const prefix = `ContourSmoother::phase_${fixture.phase}.`;
    phases.add(fixture.phase);
    lineTypes.add(fixture.parameters[prefix + 'cg_line_search_type']);
    if (actual.iterations < fixture.parameters[prefix + 'cg_max_iter']) earlyStops++;
    restarts += actual.restarts;
  }
  assert.deepEqual([...phases].sort(), [0, 1, 2]);
  assert.deepEqual([...lineTypes].sort(), [0, 5]);
  assert.ok(earlyStops > 0 && restarts > 0);
});

test('combined objectives preserve native cache state through all 6981 original sequence evaluations', () => {
  assert.equal(sequences.cases.length, 1425);
  let states = 0;
  for (const [index, fixture] of sequences.cases.entries()) {
    const imageModel = imageFor(fixture);
    for (const [step, positions] of fixture.positions.entries()) {
      const geometry = geometryAt(fixture.geometry, positions);
      const actual = smoothingObjective({ ...fixture, geometry, imageModel });
      assert.deepEqual({ ...actual, gradient:[...actual.gradient] }, fixture.expected[step], `combined sequence ${index}/${step}`);
      states++;
    }
  }
  assert.equal(states, 6981);
});

test('image smoothing validates model binding before mutation and owns its gradient storage', () => {
  const fixture = sequences.cases[0], before = structuredClone(fixture), imageModel = imageFor(fixture);
  const actual = smoothingObjective({ ...fixture, imageModel });
  actual.gradient.fill(7);
  assert.deepEqual(fixture, before);
  const wrongCount = structuredClone(fixture.geometry), wrongContour = structuredClone(fixture.geometry);
  wrongCount.regions[0].pixelCount++;
  wrongContour.regions[0].contour.reverse();
  for (const invalid of [{ ...fixture }, { ...fixture, imageModel:{} },
    { ...fixture, imageModel, geometry:wrongCount }, { ...fixture, imageModel, geometry:wrongContour }]) {
    assert.throws(() => smoothingObjective(invalid));
  }
  assert.throws(() => smoothingLineSearch({ ...fixture, imageModel, width:fixture.width + 1,
    direction:new Float64Array(fixture.geometry.nodes.length * 2), epsilon:0.01, nearBorderNodes:[] }));
  const geometry = geometryAt(fixture.geometry, fixture.positions[1]);
  const after = smoothingObjective({ ...fixture, geometry, imageModel });
  assert.deepEqual({ ...after, gradient:[...after.gradient] }, fixture.expected[1]);
  assert.deepEqual(fixture, before);
});
