import assert from 'node:assert/strict';
import { createReadStream } from 'node:fs';
import { createInterface } from 'node:readline';
import { createGunzip } from 'node:zlib';
import { test } from 'node:test';
import reference from './fixtures/fitting-samples-reference.json' with { type:'json' };
import costs from './fixtures/fitting-cost-reference.json' with { type:'json' };
import keepers from './fixtures/fitting-keepers-reference.json' with { type:'json' };
import pipeline from './fixtures/pipeline-fitting-reference.json' with { type:'json' };
import complete from './fixtures/fitting-full-reference.json' with { type:'json' };
import { prepareFittingSamples, fitContourInterval, partitionContourInterval } from '../src/fitting.mjs';
import { ContourFitter, fitContours } from '../src/contour-fitting.mjs';
import { CoreSession,resolveParameters } from '../src/settings.mjs';

test('fitting sample preparation matches all 2784 original chord-length and marking states', () => {
  assert.equal(reference.cases.length, 2784);
  let samples = 0;
  for (const [index, fixture] of reference.cases.entries()) {
    const input = structuredClone(fixture);
    const actual = prepareFittingSamples(input);
    const { randomState, ...expected } = fixture.expected;
    assert.equal(randomState, 1);
    assert.deepEqual({ parameters:[...actual.parameters], points:actual.points,
      kinds:input.nodes.map(node => node.kind) }, expected, `original sample preparation ${index}`);
    assert.deepEqual(input.contour, fixture.contour);
    assert.deepEqual(input.nodes.map(({ x, y }) => [x, y]), fixture.nodes.map(({ x, y }) => [x, y]));
    samples += actual.points.length;
  }
  assert.equal(samples, 7408);
});

test('fitting sample outputs own their coordinates and parameters', () => {
  const fixture = reference.cases.find(sample => sample.expected.points.length >= 4 && !sample.markInterior);
  const input = structuredClone(fixture), before = structuredClone(input);
  const first = prepareFittingSamples(input), second = prepareFittingSamples(input);
  first.parameters[0] = -1;
  first.points[0][0] = -1;
  assert.deepEqual(input, before);
  assert.deepEqual([...second.parameters], fixture.expected.parameters);
  assert.deepEqual(second.points, fixture.expected.points);
});

test('interval fit costs and control points match all 2784 original sample-count branches', () => {
  assert.equal(costs.cases.length, 2784);
  for (const [index, fixture] of costs.cases.entries()) {
    const input = structuredClone(fixture);
    const actual = fitContourInterval(input);
    const { randomState, ...expected } = fixture.expected;
    assert.equal(randomState, 1);
    assert.deepEqual({ ...actual, kinds:input.nodes.map(node => node.kind) }, expected,
      `original fit cost ${index}`);
  }
});

test('keeper partitions, cached costs and threshold schedules match all 3907 original states', () => {
  assert.equal(keepers.cases.length, 3907);
  for (const [index, fixture] of keepers.cases.entries()) {
    const input = structuredClone(fixture), before = structuredClone(input);
    const { randomState, ...expected } = fixture.expected;
    assert.equal(randomState, 1);
    assert.deepEqual(partitionContourInterval(input), expected,
      `original keeper partition ${index}`);
    assert.deepEqual(input, before);
  }
});

test('complete fitting matches all 15 original shared-geometry pipeline targets', () => {
  assert.equal(pipeline.cases.length, 15);
  for (const fixture of pipeline.cases) {
    const before = structuredClone(fixture);
    assert.deepEqual(fitContours(fixture), fixture.expected, fixture.id);
    assert.deepEqual(fixture, before);
    assert.equal(fixture.randomStateBefore, fixture.randomStateAfter);
  }
});

test('complete fitting matches all 2712 original geometries from the full smoothing corpus', () => {
  assert.equal(complete.cases.length, 2712);
  const profiles = new Set();
  for (const [index, fixture] of complete.cases.entries()) {
    assert.deepEqual(fitContours(fixture), fixture.expected, `complete original fitting ${index}`);
    assert.equal(fixture.randomStateBefore, fixture.randomStateAfter);
    if (fixture.fullProfile !== undefined) profiles.add(fixture.fullProfile);
  }
  assert.deepEqual([...profiles].sort(), [1, 2, 3, 4, 5, 6, 7, 8, 9]);
});

test('public fitting validates geometry and controls before changing caller storage', () => {
  const fixture = structuredClone(pipeline.cases[0]);
  for (const change of [
    input => { input.geometry.nodes[0].x = Infinity; },
    input => { input.geometry.regions[0].contour[0] = input.geometry.nodes.length; },
    input => { input.geometry.regions[0].boundary.pop(); },
    input => { input.parameters['BezierFitter::stat_thresh_initial'] = 0; },
    input => { input.parameters['BezierFitter::max_iterations'] = -1; },
  ]) {
    const input = structuredClone(fixture);
    change(input);
    const before = structuredClone(input);
    assert.throws(() => fitContours(input), /Fitting needs/);
    assert.deepEqual(input, before);
  }
  const before = structuredClone(fixture), fitted = fitContours(fixture);
  fitted.nodes[0].x = -1;
  fitted.regions[0].bgra[0] = -1;
  fitted.regions[0].contour[0] = -1;
  fitted.cubics[0].points[0] = -1;
  assert.deepEqual(fixture, before);
  assert.deepEqual(fitContours(fixture), fixture.expected);
});

for (const [fixtureName,expectedCases] of [['fitting-cancellation',40],['fitting-region-progress',64],['fitting-resmoothing',16],['fitting-resegmenting',16]]) {
test(`retained fitting cancellation preserves every defined native node, edge and cubic slot: ${fixtureName}`, async () => {
  const rows = createInterface({input:createReadStream(new URL(`./fixtures/${fixtureName}-reference.ndjson.gz`,import.meta.url))
    .pipe(createGunzip()),crlfDelay:Infinity});
  let cases = 0,checkpoints = 0,cancellations = 0;
  for await (const line of rows) {
    const fixture = JSON.parse(line);
    if (fixture.type === 'metadata') {assert.equal(fixture.wholeNativeResponsesRepeatIdentical,true);continue;}
    const {input,native,mode,id} = fixture,session = new CoreSession();
    const document = session.createDocument({width:input.width,height:input.height,
      bgra:Uint8Array.from(Buffer.from(input.bgra,'base64')),hasAlpha:true,shadowControl:0});
    const controls = step => ({...step,palette:Float32Array.from(step.palette.flat())});
    document.segment(controls(input.steps[0]));document.smooth(controls(input.steps[1]));
    const source = document.snapshot().contours,before = structuredClone(source),fitter = new ContourFitter(source);
    // Native +0x1b4 retains the last notification across tasks, including
    // cancelled runs. It is separate from the reset task progress member.
    let previousPercent = document.snapshot().progress;
    let regionsReconstructed = false;
    for (let index = 2; index < input.steps.length; index++) {
      const selected = input.steps[index],expected = native.steps[index],events = [];
      if (selected.action === 1) {
        document.segment(controls(selected));regionsReconstructed = true;continue;
      }
      if (selected.action === 2) {
        document.smooth(controls(selected));
        if (regionsReconstructed) fitter.setRegions(document.snapshot().contours);
        else fitter.setContours(document.snapshot().contours);
        regionsReconstructed = false;
        // Smoothing's final notification survives into the following task.
        // Compare it without using native output as independent state input.
        const last = expected.events.at(-1),fraction = Buffer.from(last.fractionBits,'hex').readDoubleLE();
        previousPercent = document.snapshot().progress;
        assert.equal(Math.floor(fraction*100+0.5),previousPercent,id+' smoothing notification');
        assert.equal(fitter.snapshot().cubicStorageSlots,expected.state.geometryContainerCounts[2],id);
        continue;
      }
      const parameters = resolveParameters({stage:'fitting',mode:'basic',
        category:['photograph','artwork-with-blending','artwork-without-blending'][selected.category],
        quality:['high','medium','low'][selected.quality]});
      const interrupted = new Error('Stop at the selected native checkpoint.');
      const run = () => fitter.run(parameters,event => {
        if (mode === 1 && event.percent === previousPercent) return;
        previousPercent = event.percent;
        const fraction = Buffer.alloc(8);fraction.writeDoubleLE(event.fraction);
        events.push({phase:event.phase,substep:event.substep,counter:event.counter,fractionBits:fraction.toString('hex')});
        if (events.length === selected.cancelAt) throw interrupted;
      });
      if (selected.cancelAt) {assert.throws(run,error => error === interrupted,id);cancellations++;}
      else run();
      assert.deepEqual(events,expected.events.map(({header,fractionBits}) => ({phase:header[3],substep:header[4]|0,
        counter:header[5]|0,fractionBits})),`${id} step ${index} progress`);
      assert.deepEqual(fitter.snapshot(),expected.state.fittedGeometry,`${id} step ${index} live geometry`);
      assert.equal(session.randomState,expected.state.randomState,`${id} step ${index} random stream`);
      assert.deepEqual(source,before,`${id} caller ownership`);
      checkpoints += events.length;
    }
    cases++;
  }
  assert.deepEqual({cases,cancellations},{cases:expectedCases,cancellations:expectedCases});
  assert.ok(checkpoints > expectedCases*10);
});
}
