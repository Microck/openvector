import assert from 'node:assert/strict';
import { test } from 'node:test';
import reference from './fixtures/pipeline-smoothing-reference.json' with { type:'json' };
import segmentation from './fixtures/pipeline-segmentation-reference.json' with { type:'json' };
import prefix from './fixtures/smoothing-full-prefix-reference.json' with { type:'json' };
import complete from './fixtures/smoothing-full-sessions-reference.json' with { type:'json' };
import { smoothContourGeometry } from '../src/contour-smoothing.mjs';
import { CoreSession } from '../src/settings.mjs';

test('retained original conversion failure does not produce an independent vector result', () => {
  const fixture = prefix.nativeFailure.input;
  let randomState = prefix.nativeFailure.randomStateBefore;
  assert.throws(() => smoothContourGeometry({ ...fixture, indices:fixture.segmentation.indices, palette:fixture.segmentation.palette }, () => {
    randomState = randomState * 2003 % 9973;
    return randomState * (1 / 9973);
  }), 'The original faults for this recorded state; the independent stage currently returns a different outcome.');
});

test('full smoothing matches all 2713 original outcomes across explicit reference sessions', () => {
  assert.equal(complete.cases.length, 2713);
  assert.equal(complete.status, 'complete');
  let session, successes = 0, failures = 0;
  const profiles = new Set(), masks = new Set();
  for (const [index, fixture] of complete.cases.entries()) {
    if (fixture.sessionStart) session = new CoreSession();
    assert.equal(session.randomState, fixture.randomStateBefore);
    const input = { ...fixture, indices:fixture.segmentation.indices, palette:fixture.segmentation.palette };
    if (fixture.expectedFailure) {
      assert.throws(() => session.smoothContours(input), `native failure ${index}`);
      assert.equal(session.randomState, fixture.randomStateBefore);
      failures++;
    } else {
      const actual = session.smoothContours(input);
      assert.deepEqual({ geometry:actual.geometry, penaltyCounts:[...actual.penaltyCounts], randomState:session.randomState },
        fixture.expected, `completed original full-stage case ${index}`);
      successes++;
    }
    if (fixture.fullProfile !== undefined) profiles.add(fixture.fullProfile);
    if (fixture.phaseMask !== undefined) masks.add(fixture.phaseMask);
  }
  assert.equal(successes, 2712);
  assert.equal(failures, 1);
  assert.deepEqual([...profiles].sort(), [1, 2, 3, 4, 5, 6, 7, 8, 9]);
  assert.deepEqual([...masks].sort(), [0, 1, 2, 3, 4, 5, 6, 7]);
});

test('observing smoothing preserves native completed geometry for all eight phase masks', () => {
  const selected = new Map();
  for (const fixture of complete.cases) if (fixture.phaseMask !== undefined && !fixture.expectedFailure && !selected.has(fixture.phaseMask))
    selected.set(fixture.phaseMask,fixture);
  assert.equal(selected.size,8);
  for (const [mask,fixture] of selected) {
    let randomState = fixture.randomStateBefore;
    const input = {...fixture,indices:fixture.segmentation.indices,palette:fixture.segmentation.palette};
    const before = structuredClone(input),events = [];
    const actual = smoothContourGeometry(input,() => {
      randomState = randomState*2003%9973;
      return randomState*(1/9973);
    },event => events.push(event));
    assert.deepEqual({geometry:actual.geometry,penaltyCounts:[...actual.penaltyCounts],randomState},fixture.expected,`phase mask ${mask}`);
    assert.deepEqual(input,before,`owned input for mask ${mask}`);
    assert.ok(events.length > 4);
    for (const event of events) {
      assert.equal(event.phase,8);
      assert.ok(Number.isInteger(event.counter));
      assert.ok(Number.isFinite(event.fraction));
    }
  }
});

test('successful public smoothing retains one stream and failed library calls restore that stream', () => {
  const session = new CoreSession();
  for (const fixture of reference.cases.filter(candidate => candidate.parameters['Shared::is_anti_aliased'] === 1).slice(0, 3)) {
    const index = reference.cases.indexOf(fixture);
    const input = { ...fixture, geometry:fixture.before, indices:segmentation.cases[index].expected.indices,
      palette:segmentation.cases[index].expected.palette };
    assert.equal(session.randomState, fixture.randomStateBefore);
    const invalid = structuredClone(input);
    invalid.parameters['ContourSmoother::phase_2.cg_min_eps'] = -1;
    const before = structuredClone(invalid);
    assert.throws(() => session.smoothContours(invalid), /Invalid quadratic line-search phase controls/);
    assert.deepEqual(invalid, before);
    assert.equal(session.randomState, fixture.randomStateBefore);
    const actual = session.smoothContours(input);
    assert.deepEqual(actual.geometry, fixture.expected, fixture.id);
    assert.equal(session.randomState, fixture.randomStateAfter, fixture.id);
  }
});

test('complete smoothing matches all 15 original pipeline targets at their recorded session boundary', () => {
  assert.equal(reference.cases.length, 15);
  for (const [index, fixture] of reference.cases.entries()) {
    let randomState = fixture.randomStateBefore;
    const nextUniform = () => {
      randomState = randomState * 2003 % 9973;
      return randomState * (1 / 9973);
    };
    const input = { ...fixture, geometry:fixture.before, indices:segmentation.cases[index].expected.indices,
      palette:segmentation.cases[index].expected.palette };
    const before = structuredClone(input);
    const actual = smoothContourGeometry(input, nextUniform);
    assert.deepEqual(actual.geometry, fixture.expected, fixture.id);
    assert.deepEqual([...actual.penaltyCounts], fixture.stateMetadata.penaltyCounts, fixture.id);
    assert.equal(randomState, fixture.randomStateAfter, fixture.id);
    assert.deepEqual(input, before);
  }
});
