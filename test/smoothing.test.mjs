import assert from 'node:assert/strict';
import { test } from 'node:test';
import reference from './fixtures/curvature-prior-reference.json' with { type: 'json' };
import objectiveReference from './fixtures/smoothing-objective-reference.json' with { type: 'json' };
import lineReference from './fixtures/smoothing-line-search-reference.json' with { type: 'json' };
import optimizerReference from './fixtures/smoothing-optimizer-reference.json' with { type: 'json' };
import cachedOptimizerReference from './fixtures/smoothing-cached-optimizer-reference.json' with { type: 'json' };
import { curvaturePrior, quadraticPrior, smoothingObjective, smoothingLineSearch, smoothingOptimize } from '../src/smoothing.mjs';

test('small-region quadratic priors and derivatives match all 5016 original calculations exactly', () => {
  assert.equal(reference.cases.length, 5016);
  for (const [index, fixture] of reference.cases.entries()) {
    const actual = quadraticPrior(fixture);
    assert.equal(actual.energy, fixture.quadraticExpected.energy, `case ${index} energy`);
    assert.deepEqual(Array.from(actual.firstGradient), fixture.quadraticExpected.firstGradient, `case ${index} first derivative`);
    assert.deepEqual(Array.from(actual.lastGradient), fixture.quadraticExpected.lastGradient, `case ${index} last derivative`);
  }
});

test('curvature priors and endpoint derivatives match all 5010 original calculations exactly', () => {
  const finite = reference.cases.filter(fixture => !fixture.degenerate);
  assert.equal(finite.length, 5010);
  for (const [index, fixture] of finite.entries()) {
    const actual = curvaturePrior(fixture);
    assert.equal(actual.energy, fixture.expected.energy, `case ${index} energy`);
    assert.deepEqual(Array.from(actual.firstGradient), fixture.expected.firstGradient, `case ${index} first derivative`);
    assert.deepEqual(Array.from(actual.lastGradient), fixture.expected.lastGradient, `case ${index} last derivative`);
  }
});

test('collapsed edges retain native IEEE results instead of adding invented clamping', () => {
  const degenerate = reference.cases.filter(fixture => fixture.degenerate);
  assert.equal(degenerate.length, 6);
  for (const fixture of degenerate) {
    assert.deepEqual(fixture.expected, { energy:'NaN', firstGradient:['NaN','NaN'], lastGradient:['NaN','NaN'] });
    const actual = curvaturePrior(fixture);
    assert.ok(Number.isNaN(actual.energy));
    assert.ok(Array.from(actual.firstGradient).every(Number.isNaN));
    assert.ok(Array.from(actual.lastGradient).every(Number.isNaN));
  }
});

test('prior endpoint derivatives agree with independent finite differences', () => {
  const input = { points:[[0.25, 0.5], [1.5, 0.7], [2.1, 2.4]], sampleCounts:[1, 3], lengthWeight:0.5 };
  const step = 1e-6;
  for (const prior of [curvaturePrior, quadraticPrior]) {
    const { firstGradient, lastGradient } = prior(input);
    for (const [point, gradient] of [[0, firstGradient], [2, lastGradient]]) {
      for (const axis of [0, 1]) {
        const before = structuredClone(input), after = structuredClone(input);
        before.points[point][axis] -= step;
        after.points[point][axis] += step;
        const difference = (prior(after).energy - prior(before).energy) / (2 * step);
        assert.ok(Math.abs(difference - gradient[axis]) < 1e-8, `${prior.name}, point ${point}, axis ${axis}`);
      }
    }
  }
});

test('curvature prior owns derivative output and validates its public boundary', () => {
  const input = { points:[Float64Array.of(0, 0), Float64Array.of(1, 1), Float64Array.of(3, 1)],
    sampleCounts:Int32Array.of(1, 2), lengthWeight:0.1 };
  const before = structuredClone(input);
  const actual = curvaturePrior(input);
  const quadratic = quadraticPrior(input);
  actual.firstGradient.fill(7);
  actual.lastGradient.fill(7);
  quadratic.firstGradient.fill(7);
  quadratic.lastGradient.fill(7);
  assert.deepEqual(input, before);
  for (const invalid of [null, {}, { ...input, points:[] }, { ...input, points:[[], [], []] },
    { ...input, points:[[0, 0], [NaN, 1], [2, 2]] }, { ...input, sampleCounts:[0, 1] },
    { ...input, sampleCounts:[1, 2147483648] }, { ...input, sampleCounts:[1, 1.5] },
    { ...input, sampleCounts:new Array(2) }, { ...input, lengthWeight:-1 }, { ...input, lengthWeight:Infinity }]) {
    assert.throws(() => curvaturePrior(invalid));
  }
  assert.deepEqual(input, before);
});

test('prepared smoothing energy and constrained gradients match all 5598 native states exactly', () => {
  assert.equal(objectiveReference.cases.length, 5598);
  const phases = new Set(), measurements = new Set(), priors = new Set();
  let flaggedNodes = 0, penalizedNodes = 0, smallRegions = 0, smoothedStates = 0;
  for (const [index, fixture] of objectiveReference.cases.entries()) {
    const actual = smoothingObjective(fixture);
    assert.equal(actual.energy, fixture.expected.energy, `case ${index} energy`);
    assert.deepEqual(Array.from(actual.gradient), fixture.expected.gradient, `case ${index} gradient`);
    phases.add(fixture.phase);
    measurements.add(fixture.parameters['ContourSmoother::measurement_types'][fixture.phase]);
    priors.add(fixture.parameters['ContourSmoother::prior_types'][fixture.phase]);
    flaggedNodes += fixture.geometry.nodes.filter(node => node.flags & 16).length;
    penalizedNodes += fixture.penaltyCounts.filter(count => count > 0).length;
    smallRegions += fixture.geometry.regions.filter(region => region.pixelCount < 8).length;
    if (fixture.sourcePipeline) smoothedStates++;
  }
  assert.deepEqual([...phases].sort(), [0, 1, 2]);
  assert.deepEqual([...measurements].sort(), [0, 2]);
  assert.deepEqual([...priors].sort(), [0, 3]);
  assert.ok(flaggedNodes > 0 && penalizedNodes > 0 && smallRegions > 0);
  assert.equal(smoothedStates, 90);
});

test('prepared smoothing derivatives agree with finite differences on free coordinates', () => {
  // Native gradients project border constraints. Finite differences therefore
  // check only free coordinates, leaving corners and the fixed axis untouched.
  const fixture = structuredClone(objectiveReference.cases[1]);
  const { gradient } = smoothingObjective(fixture);
  const [left, right, top, bottom] = fixture.borderCounts;
  for (let node = 0; node < fixture.geometry.nodes.length; node++) {
    for (const [axis, field] of ['x', 'y'].entries()) {
      const isCorner = [0, left - 1, left, left + right - 1].includes(node);
      const constrained = isCorner || (axis === 0 && node < left + right) ||
        (axis === 1 && node >= left + right && node < left + right + top + bottom);
      if (constrained) {
        assert.equal(gradient[node * 2 + axis], 0);
        continue;
      }
      const before = structuredClone(fixture), after = structuredClone(fixture), step = 1e-6;
      before.geometry.nodes[node][field] -= step;
      after.geometry.nodes[node][field] += step;
      const derivative = (smoothingObjective(after).energy - smoothingObjective(before).energy) / (2 * step);
      assert.ok(Math.abs(derivative - gradient[node * 2 + axis]) < 1e-6, `node ${node} ${field}`);
    }
  }
});

test('prepared smoothing owns outputs, normalizes references to float32 and validates its boundary', () => {
  const fixture = structuredClone(objectiveReference.cases[1]);
  const before = structuredClone(fixture);
  const actual = smoothingObjective(fixture);
  actual.gradient.fill(7);
  assert.deepEqual(fixture, before);
  const typed = { ...fixture, referencePositions:Float32Array.from(fixture.referencePositions),
    penaltyCounts:Uint8Array.from(fixture.penaltyCounts), borderCounts:Int32Array.from(fixture.borderCounts) };
  assert.deepEqual(smoothingObjective(typed), smoothingObjective(fixture));
  for (const invalid of [null, {}, { ...fixture, phase:3 }, { ...fixture, geometry:{} },
    { ...fixture, referencePositions:[] }, { ...fixture, penaltyCounts:[] }, { ...fixture, borderCounts:[1, 1, 0, 0] },
    { ...fixture, referencePositions:fixture.referencePositions.map(() => Infinity) },
    { ...fixture, penaltyCounts:fixture.penaltyCounts.map(() => 256) },
    { ...fixture, parameters:{ ...fixture.parameters, 'ContourSmoother::measurement_types':[1, 1, 1] } },
    { ...fixture, parameters:{ ...fixture.parameters, 'ContourSmoother::prior_types':[1, 1, 1] } },
    { ...fixture, parameters:{ ...fixture.parameters, 'ContourSmoother::air_pressure_weight':-1 } }]) {
    assert.throws(() => smoothingObjective(invalid));
  }
  assert.deepEqual(fixture, before);
});

test('quadratic line search matches all 3312 original position and controller results exactly', () => {
  assert.equal(lineReference.cases.length, 3312);
  const variants = new Set(), iterations = new Set();
  let zeroSteps = 0, negativeSteps = 0, maximumEpsilons = 0;
  for (const [index, fixture] of lineReference.cases.entries()) {
    const actual = smoothingLineSearch(fixture);
    assert.deepEqual({ ...actual, positions:Array.from(actual.positions) }, fixture.expected, `line case ${index}`);
    variants.add(fixture.lineVariant);
    iterations.add(actual.iterations);
    if (actual.step === 0) zeroSteps++;
    if (actual.step < 0) negativeSteps++;
    if (actual.epsilon === fixture.parameters[`ContourSmoother::phase_${fixture.phase}.cg_max_eps`]) maximumEpsilons++;
  }
  assert.deepEqual([...variants].sort(), [0, 1, 2, 3, 4, 5, 6, 7]);
  assert.deepEqual([...iterations].sort(), [1, 2, 3]);
  assert.equal(zeroSteps, 860);
  assert.equal(negativeSteps, 143);
  assert.equal(maximumEpsilons, 751);
});

test('line search preserves caller state and validates the prepared cache and phase controls', () => {
  const fixture = structuredClone(lineReference.cases[8]);
  const before = structuredClone(fixture);
  const actual = smoothingLineSearch(fixture);
  actual.positions.fill(7);
  assert.deepEqual(fixture, before);
  const typed = { ...fixture, direction:Float64Array.from(fixture.direction), nearBorderNodes:Int32Array.from(fixture.nearBorderNodes) };
  assert.deepEqual(smoothingLineSearch(typed), smoothingLineSearch(fixture));
  const prefix = `ContourSmoother::phase_${fixture.phase}.`;
  for (const invalid of [{ ...fixture, width:1 }, { ...fixture, height:Infinity }, { ...fixture, epsilon:0 },
    { ...fixture, direction:[] }, { ...fixture, direction:fixture.direction.map(() => NaN) },
    { ...fixture, nearBorderNodes:[0] }, { ...fixture, nearBorderNodes:[fixture.geometry.nodes.length] },
    { ...fixture, parameters:{ ...fixture.parameters, [prefix + 'cg_min_eps']:0 } },
    { ...fixture, parameters:{ ...fixture.parameters, [prefix + 'cg_max_eps']:0 } },
    { ...fixture, parameters:{ ...fixture.parameters, [prefix + 'cg_max_step_size']:-1 } },
    { ...fixture, parameters:{ ...fixture.parameters, [prefix + 'cg_quad_line_srch_max_iter']:0 } },
    { ...fixture, parameters:{ ...fixture.parameters, [prefix + 'cg_use_eps_from_step']:2 } }]) {
    assert.throws(() => smoothingLineSearch(invalid));
  }
  assert.deepEqual(fixture, before);
});

test('prepared conjugate-gradient optimization matches all 588 native complete controllers exactly', () => {
  assert.equal(optimizerReference.cases.length, 588);
  const variants = new Set(), phases = new Set();
  let greatestIteration = 0, earlyStops = 0, restarts = 0;
  for (const [index, fixture] of optimizerReference.cases.entries()) {
    const actual = smoothingOptimize(fixture);
    assert.deepEqual({ ...actual, positions:Array.from(actual.positions), storedGradient:Array.from(actual.storedGradient),
      direction:Array.from(actual.direction) }, fixture.expected, `optimizer case ${index}`);
    variants.add(fixture.optimizerVariant);
    phases.add(fixture.phase);
    greatestIteration = Math.max(greatestIteration, actual.iterations);
    restarts += actual.restarts;
    if (actual.iterations < fixture.parameters[`ContourSmoother::phase_${fixture.phase}.cg_max_iter`]) earlyStops++;
  }
  assert.deepEqual([...variants].sort(), [0, 1, 2, 3]);
  assert.deepEqual([...phases].sort(), [0, 1, 2]);
  assert.equal(greatestIteration, 260);
  assert.ok(earlyStops > 0 && restarts > 0);
});

test('prepared optimizer owns state and rejects unsupported controllers or invalid stopping controls', () => {
  const fixture = structuredClone(optimizerReference.cases[7]);
  const before = structuredClone(fixture);
  const actual = smoothingOptimize(fixture);
  actual.positions.fill(7);
  actual.direction.fill(7);
  actual.storedGradient.fill(7);
  assert.deepEqual(fixture, before);
  const prefix = `ContourSmoother::phase_${fixture.phase}.`;
  for (const [name, value] of [['cg_line_search_type', 1], ['cg_iter_for_restart', 0], ['cg_max_iter', 0],
    ['cg_min_iter', -1], ['cg_knock_out_count_down', -1], ['cg_rel_tol', NaN], ['cg_abs_tol', -1]]) {
    assert.throws(() => smoothingOptimize({ ...fixture, parameters:{ ...fixture.parameters, [prefix + name]:value } }));
  }
  assert.throws(() => smoothingOptimize({ ...fixture, parameters:{ ...fixture.parameters, 'ContourSmoother::optimizer_type':[1, 1, 1] } }));
  assert.deepEqual(fixture, before);
});

test('seeing/blind optimization matches all 441 complete native controllers exactly', () => {
  assert.equal(cachedOptimizerReference.cases.length, 441);
  const variants = new Set(), phases = new Set();
  let blindRuns = 0, rejectedBlindRuns = 0, wrappedCycles = 0;
  for (const [index, fixture] of cachedOptimizerReference.cases.entries()) {
    const actual = smoothingOptimize(fixture);
    assert.deepEqual({ ...actual, positions:[...actual.positions], storedGradient:[...actual.storedGradient],
      direction:[...actual.direction] }, fixture.expected, `seeing/blind case ${index}`);
    variants.add(fixture.cachedVariant);
    phases.add(fixture.phase);
    if (actual.lineIterations < actual.iterations) blindRuns++;
    const prefix = `ContourSmoother::phase_${fixture.phase}.`;
    const calls = actual.iterations + (actual.iterations < fixture.parameters[prefix + 'cg_max_iter'] ? 1 : 0);
    const seeing = fixture.parameters[prefix + 'cg_num_seeing_steps'];
    const cycle = seeing + fixture.parameters[prefix + 'cg_num_blind_steps'];
    // Without rejected estimates, this cyclic schedule predicts the number of
    // seeing calls. Extra seeing calls establish that the retry branch ran.
    const scheduled = Math.floor(calls / cycle) * seeing + Math.min(calls % cycle, seeing);
    if (actual.lineIterations > scheduled) rejectedBlindRuns++;
    if (calls > cycle) wrappedCycles++;
  }
  assert.deepEqual([...variants].sort(), [0, 1, 2]);
  assert.deepEqual([...phases].sort(), [0, 1, 2]);
  assert.ok(blindRuns > 0);
  assert.equal(rejectedBlindRuns, 187);
  assert.equal(wrappedCycles, 321);
});

test('seeing/blind controls fail before mutation and successful calls own their state', () => {
  const fixture = structuredClone(cachedOptimizerReference.cases[2]);
  const before = structuredClone(fixture);
  const actual = smoothingOptimize(fixture);
  actual.positions.fill(7);
  actual.direction.fill(7);
  actual.storedGradient.fill(7);
  assert.deepEqual(fixture, before);
  const prefix = `ContourSmoother::phase_${fixture.phase}.`;
  for (const [name, value] of [['cg_num_seeing_steps', 0], ['cg_num_blind_steps', -1],
    ['cg_blind_blend_latest', -0.1], ['cg_blind_blend_latest', 1.1], ['cg_blind_blend_latest', Infinity]]) {
    assert.throws(() => smoothingOptimize({ ...fixture, parameters:{ ...fixture.parameters, [prefix + name]:value } }));
  }
  assert.deepEqual(fixture, before);
});
