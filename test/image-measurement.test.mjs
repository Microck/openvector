import assert from 'node:assert/strict';
import { test } from 'node:test';
import reference from './fixtures/smoothing-image-measurement-reference.json' with { type:'json' };
import sequences from './fixtures/smoothing-image-sequence-reference.json' with { type:'json' };
import { createContourImageModel } from '../src/image-measurement.mjs';
import scratchReference from './fixtures/image-fragment-scratch-reference.json' with { type:'json' };
import { PixelFragmentScratch } from '../src/image-measurement-kernel.mjs';

test('fragment scratch fields match the original overlapping words at the retained cache fault', () => {
  const scratch = new PixelFragmentScratch();
  scratch.count = 0;
  scratch.background = -1;
  const records = scratchReference.records;
  let groups = 0;
  for (let first = 0; first < records.length;) {
    let last = first, area = 0;
    while (last + 1 < records.length && records[last + 1][2] === records[first][2]) {
      area = (records[last + 1][4] * records[last][3] - records[last][4] * records[last + 1][3]) + area;
      last++;
    }
    area = ((records[first][4] * records[last][3] - records[last][4] * records[first][3]) + area) * 0.5;
    scratch.writeIdentifier(scratch.count, records[first][2]);
    scratch.writeArea(scratch.count, area);
    scratch.count = scratch.count + 1;
    groups++;
    first = last + 1;
  }
  assert.equal(groups, 11);
  assert.equal(scratch.count, scratchReference.expectedCountWord);
  const bytes = Buffer.from(scratchReference.scratchBytesHex, 'hex');
  const original = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  assert.equal(scratch.background, original.getInt32(0x7c, true));
  for (let index = 0; index < 10; index++) assert.equal(scratch.identifier(index), original.getInt32(index * 4, true));
  for (let index = 0; index < 11; index++) assert.equal(scratch.area(index), original.getFloat64(0x28 + index * 8, true));
});

const inputFor = fixture => ({ width:fixture.width, height:fixture.height, bgra:fixture.bgra,
  indices:fixture.segmentation.indices, geometry:fixture.geometry });
const positionsFor = fixture => fixture.geometry.nodes.flatMap(node => [node.x, node.y]);

function assertMeasurement(actual, expected, context) {
  assert.deepEqual({ ...actual, gradient:[...actual.gradient], colors:[...actual.colors], flags:[...actual.flags] }, expected, context);
}

test('image measurement matches all 2802 native energies, colors, clipped records and raw gradients', () => {
  assert.equal(reference.cases.length, 2802);
  let records = 0, pipelineStates = 0;
  const purposes = new Set();
  for (const [index, fixture] of reference.cases.entries()) {
    const actual = createContourImageModel(inputFor(fixture)).evaluate(positionsFor(fixture));
    assertMeasurement(actual, fixture.expected, `image measurement ${index}`);
    records += actual.pixels.reduce((sum, pixel) => sum + pixel.length, 0);
    if (fixture.sourcePipeline) pipelineStates++;
    if (fixture.purpose) purposes.add(fixture.purpose);
  }
  assert.equal(records, 925831);
  assert.equal(pipelineStates, 45);
  assert.ok(purposes.has('Exact quarter-pixel displacement'));
  assert.ok(purposes.has('Near-horizontal double directions, exponent -70'));
  assert.ok(purposes.has('Near-horizontal double directions, exponent -90'));
  const perimeterWrap = reference.cases.find(fixture => fixture.sourceObjective === 2217);
  assert.equal(perimeterWrap.phase, 2);
  assert.deepEqual(perimeterWrap.expected.pixels[1], []);
});

test('persistent image measurement matches all 6981 native states across 1425 coordinate sequences', () => {
  assert.equal(sequences.cases.length, 1425);
  let states = 0, records = 0, hysteresis = 0;
  for (const [index, fixture] of sequences.cases.entries()) {
    const model = createContourImageModel(inputFor(fixture));
    for (const [step, positions] of fixture.positions.entries()) {
      const actual = model.evaluate(positions);
      assertMeasurement(actual, fixture.expected[step], `image sequence ${index}, step ${step}`);
      states++;
      records += actual.pixels.reduce((sum, pixel) => sum + pixel.length, 0);
    }
    if (fixture.expected[0].energy !== fixture.expected.at(-1).energy) hysteresis++;
  }
  assert.equal(states, 6981);
  assert.equal(records, 2346200);
  // Identical restored coordinates can retain different native cached colors.
  // Reinitializing from segmentation on every call would erase this behavior.
  assert.equal(hysteresis, 652);
});

test('quarter-pixel image error and free interior derivatives agree with direct geometry', () => {
  const fixture = reference.cases.find(sample => sample.purpose === 'Exact quarter-pixel displacement');
  const positions = positionsFor(fixture), input = inputFor(fixture);
  const actual = createContourImageModel(input).evaluate(positions);
  assert.equal(actual.energy, 0.1875);
  const interior = fixture.geometry.nodes.findIndex(node => node.kind === 0);
  assert.ok(interior >= 0);
  const step = 1 / 1024;
  for (const axis of [0, 1]) {
    const before = positions.slice(), after = positions.slice(), channel = interior * 2 + axis;
    before[channel] -= step;
    after[channel] += step;
    const derivative = (createContourImageModel(input).evaluate(after).energy -
      createContourImageModel(input).evaluate(before).energy) / (2 * step);
    assert.equal(derivative, actual.gradient[channel]);
  }
});

test('image models own prepared inputs, returned snapshots and evaluation coordinates', () => {
  const fixture = reference.cases[1];
  const input = { ...inputFor(fixture), bgra:Uint8Array.from(fixture.bgra),
    indices:Int32Array.from(fixture.segmentation.indices), geometry:structuredClone(fixture.geometry) };
  const before = structuredClone(input), positions = Float64Array.from(positionsFor(fixture));
  const model = createContourImageModel(input);
  const actual = model.evaluate(positions);
  assert.deepEqual(input, before);
  assertMeasurement(actual, fixture.expected);
  input.bgra.fill(17);
  input.indices.fill(0);
  input.geometry.nodes[0].x = NaN;
  input.geometry.regions[0].bgra.fill(17);
  input.geometry.regions[0].contour.fill(0);
  actual.colors.fill(7);
  actual.flags.fill(7);
  actual.gradient.fill(7);
  actual.pixels[0][0].fill(7);
  positions.fill(7);
  assertMeasurement(model.evaluate(positionsFor(fixture)), fixture.expected);
});

test('invalid prepared image inputs and coordinates fail before changing persistent state', () => {
  const fixture = sequences.cases[1], input = inputFor(fixture);
  for (const invalid of [null, {}, { ...input, width:1 }, { ...input, bgra:[] }, { ...input, geometry:{} },
    { ...input, indices:[] }, { ...input, indices:input.indices.map(() => -1) },
    { ...input, bgra:input.bgra.map(() => 256) }]) assert.throws(() => createContourImageModel(invalid));
  const model = createContourImageModel(input);
  assertMeasurement(model.evaluate(fixture.positions[0]), fixture.expected[0]);
  for (const positions of [null, [], new Array(fixture.positions[0].length),
    fixture.positions[0].map(() => Infinity), new Float32Array(fixture.positions[0])]) {
    assert.throws(() => model.evaluate(positions));
  }
  assertMeasurement(model.evaluate(fixture.positions[1]), fixture.expected[1]);
});
