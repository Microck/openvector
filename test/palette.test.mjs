import assert from 'node:assert/strict';
import { test } from 'node:test';
import reference from './fixtures/palette-reference.json' with { type: 'json' };
import refinementReference from './fixtures/refinement-reference.json' with { type: 'json' };
import peaksReference from './fixtures/peaks-reference.json' with { type: 'json' };
import generationReference from './fixtures/generation-reference.json' with { type: 'json' };
import measurementReference from './fixtures/measurement-reference.json' with { type: 'json' };
import weightedReference from './fixtures/weighted-generation-reference.json' with { type: 'json' };
import weightedRefinementReference from './fixtures/weighted-refinement-reference.json' with { type: 'json' };
import scoreReference from './fixtures/score-reference.json' with { type: 'json' };
import { assignPalette, CoreSession, findHistogramPeaks, measureAntiAlias, scoreUnweightedPalette } from '../src/palette.mjs';

test('palette assignment matches all 12032 original host-CPU pixel decisions', () => {
  assert.equal(reference.cases.length, 1193);
  for (const fixture of reference.cases) {
    const indices = assignPalette({ bgra: fixture.pixels, palette: fixture.palette });
    assert.ok(indices instanceof Int32Array);
    assert.deepEqual(Array.from(indices), fixture.expected);
  }
});

test('assignment preserves inputs, handles empty images, and keeps the first color on ties', () => {
  const bgra = new Uint8Array([128, 128, 128, 255]);
  const palette = new Float32Array([0, 0, 0, 1, 0, 0, 0, 1]);
  assert.deepEqual(Array.from(assignPalette({ bgra, palette })), [0]);
  assert.deepEqual(Array.from(bgra), [128, 128, 128, 255]);
  assert.deepEqual(Array.from(palette), [0, 0, 0, 1, 0, 0, 0, 1]);
  assert.equal(assignPalette({ bgra: [], palette }).length, 0);
});

test('palette inputs reject incomplete, sparse, and invalid channels at the boundary', () => {
  for (const input of [null, {}, { bgra: [0, 0], palette: [0, 0, 0, 1] },
    { bgra: [0, 0, 0, 256], palette: [0, 0, 0, 1] },
    { bgra: new Array(4), palette: [0, 0, 0, 1] },
    { bgra: [], palette: [] }, { bgra: [], palette: [0, NaN, 0, 1] },
    { bgra: [], palette: [0, 0, 0, 1.1] }, { bgra: [], palette: new Array(4) }]) {
    assert.throws(() => assignPalette(input));
  }
});

test('unweighted refinement matches all 89 original calls in a shared random session', () => {
  assert.equal(refinementReference.cases.length, 89);
  const session = new CoreSession();
  for (const fixture of refinementReference.cases) {
    const refined = session.refineUnweighted(fixture);
    assert.deepEqual(Array.from(refined.palette), fixture.expected.palette, 'refined colors');
    assert.deepEqual(Array.from(refined.indices), fixture.expected.indices, 'final assignments');
    assert.deepEqual(Array.from(refined.weights), fixture.expected.weights, 'membership weights');
    assert.equal(refined.passes, fixture.expected.passes);
    assert.equal(session.randomState, fixture.expected.randomState);
  }
});

test('refinement sessions own their random state and leave caller input unchanged', () => {
  const input = { bgra: [0, 0, 0, 255], palette: [0, 0, 0, 1, 0, 0, 0, 1] };
  const before = structuredClone(input);
  const first = new CoreSession();
  const second = new CoreSession();
  assert.deepEqual(first.refineUnweighted(input), second.refineUnweighted(input));
  assert.equal(first.randomState, second.randomState);
  first.refineUnweighted(input);
  assert.notEqual(first.randomState, second.randomState);
  assert.deepEqual(input, before);
  assert.throws(() => first.refineUnweighted({ bgra: [0], palette: input.palette }));
});

test('histogram peaks and their tie ordering match all 44 native scans', () => {
  assert.equal(peaksReference.cases.length, 44);
  for (const fixture of peaksReference.cases) {
    const histogram = new Float64Array(fixture.bins ** 4);
    for (const [index, weight] of fixture.entries) histogram[index] = weight;
    const peaks = findHistogramPeaks({ histogram, bins: fixture.bins, connected: fixture.connected });
    assert.deepEqual(Array.from(peaks.bgra), fixture.expected.bgra, 'ordered peak colors');
    assert.deepEqual(Array.from(peaks.weights), fixture.expected.weights, 'ordered peak weights');
  }
});

test('histogram inputs reject malformed shapes and invalid weights', () => {
  const histogram = new Float64Array(8 ** 4);
  for (const input of [null, {}, { histogram, bins: 7, connected: true },
    { histogram, bins: 8 }, { histogram: [0], bins: 8, connected: true },
    { histogram: new Array(8 ** 4), bins: 8, connected: true }]) {
    assert.throws(() => findHistogramPeaks(input));
  }
  histogram[0] = -1;
  assert.throws(() => findHistogramPeaks({ histogram, bins: 8, connected: true }));
  histogram[0] = Infinity;
  assert.throws(() => findHistogramPeaks({ histogram, bins: 8, connected: true }));
});

test('fixed-count palette generation matches all 74 complete native color-stage runs', () => {
  assert.equal(generationReference.cases.length, 74);
  const session = new CoreSession();
  for (const fixture of generationReference.cases) {
    const generated = session.generateUnweighted(fixture);
    assert.deepEqual(Array.from(generated.palette), fixture.expected.palette, 'quantized palette');
    assert.deepEqual(Array.from(generated.indices), fixture.expected.indices, 'assignments before quantization');
    assert.deepEqual(Array.from(generated.weights), fixture.expected.weights, 'membership weights');
    assert.equal(generated.passes, fixture.expected.passes);
    assert.equal(session.randomState, fixture.expected.randomState);
  }
});

test('generation validates controls without consuming random state or changing input', () => {
  const session = new CoreSession();
  const input = { bgra: [0, 0, 0, 255], colorCount: 2, bins: 8, connected: false };
  const before = structuredClone(input);
  for (const invalid of [{ ...input, colorCount: 0 }, { ...input, colorCount: 2.5 },
    { ...input, bins: 4 }, { ...input, connected: undefined }, { ...input, bgra: [1] }]) {
    assert.throws(() => session.generateUnweighted(invalid));
    assert.equal(session.randomState, 1);
  }
  session.generateUnweighted(input);
  assert.deepEqual(input, before);
});

test('anti-alias contrast bytes match all 471 original measurement runs', () => {
  assert.equal(measurementReference.cases.length, 471);
  for (const fixture of measurementReference.cases) {
    const measurement = measureAntiAlias(fixture);
    assert.ok(measurement instanceof Uint8Array);
    assert.deepEqual(Array.from(measurement), fixture.expected);
  }
});

test('measurement preserves input and validates its sampling dimensions', () => {
  const input = { width: 2, height: 2, bgra: [0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255] };
  const before = structuredClone(input);
  measureAntiAlias(input);
  assert.deepEqual(input, before);
  for (const invalid of [null, {}, { ...input, width: 1 }, { ...input, height: 1 },
    { ...input, height: 2.5 }, { ...input, bgra: input.bgra.slice(4) }]) {
    assert.throws(() => measureAntiAlias(invalid));
  }
});

test('weighted palette generation matches all 80 original anti-alias color-stage runs', () => {
  assert.equal(weightedReference.cases.length, 80);
  const session = new CoreSession();
  for (const fixture of weightedReference.cases) {
    const generated = session.generateWeighted(fixture);
    assert.deepEqual(Array.from(generated.palette), fixture.expected.palette, 'weighted palette');
    assert.deepEqual(Array.from(generated.indices), fixture.expected.indices, 'weighted assignments');
    assert.deepEqual(Array.from(generated.weights), fixture.expected.weights, 'raw contrast membership weights');
    assert.equal(generated.passes, fixture.expected.passes);
    assert.equal(session.randomState, fixture.expected.randomState);
  }
});

test('weighted generation rejects invalid controls before advancing session state', () => {
  const session = new CoreSession();
  const fixture = weightedReference.cases[0];
  for (const invalid of [{ ...fixture, colorCount: 0 }, { ...fixture, bins: 9 }, { ...fixture, connected: null }]) {
    assert.throws(() => session.generateWeighted(invalid));
    assert.equal(session.randomState, 1);
  }
});

test('weighted refinement matches all 80 native runs before byte quantization', () => {
  assert.equal(weightedRefinementReference.cases.length, 80);
  const session = new CoreSession();
  for (const fixture of weightedRefinementReference.cases) {
    const refined = session.refineWeighted(fixture);
    assert.deepEqual(Array.from(refined.palette), fixture.expected.palette, 'unquantized weighted colors');
    assert.deepEqual(Array.from(refined.indices), fixture.expected.indices, 'weighted assignments');
    assert.deepEqual(Array.from(refined.weights), fixture.expected.weights, 'raw contrast membership weights');
    assert.equal(refined.passes, fixture.expected.passes);
    assert.equal(session.randomState, fixture.expected.randomState);
  }
});

test('unweighted candidate scores match all 1235 unmodified original engine calls', () => {
  assert.equal(scoreReference.cases.length, 1235);
  for (const fixture of scoreReference.cases) {
    assert.deepEqual(scoreUnweightedPalette(fixture), fixture.expected);
  }
});

test('scoring consumes supplied indices, preserves inputs, and validates the boundary', () => {
  const input = { bgra: [0, 0, 0, 255], palette: [0, 0, 0, 1, 1, 1, 1, 1], indices: [1], costPerColor: 0.15 };
  const before = structuredClone(input);
  assert.deepEqual(scoreUnweightedPalette(input), { total: Math.fround(Math.log10(4)), error: Math.fround(Math.log10(4)), colors: 0 });
  assert.deepEqual(input, before);
  assert.deepEqual(scoreUnweightedPalette({ ...input, indices: Int32Array.of(0) }), { total: 0, error: 0, colors: 0 });
  for (const invalid of [null, {}, { ...input, indices: [] }, { ...input, indices: [-1] },
    { ...input, indices: [2] }, { ...input, indices: [0.5] }, { ...input, indices: new Array(1) },
    { ...input, costPerColor: undefined }, { ...input, costPerColor: Infinity }, { ...input, costPerColor: 1e100 }]) {
    assert.throws(() => scoreUnweightedPalette(invalid));
  }
});
