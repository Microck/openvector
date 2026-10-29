import { readRasterInput } from './region-data.mjs';
import { readPaletteColors } from './pixel-input.mjs';
import { collectBlendIndices, eightDirections, fitPreparedBlend, writeBlendSample } from './blend-solver.mjs';
import { prepareContourNeighborhoods } from './contour-neighborhoods.mjs';

const diagonal = 1 / Math.sqrt(2);
const borderThreshold = Math.fround(1.1);

function readContourDimensions(input) {
  const { width, height } = input ?? {};
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 2 || height < 2 ||
      width > 2147483647 || height > 2147483647) {
    throw new Error('Contour preparation needs int32 dimensions of at least two.');
  }
  return { width, height };
}

function readFixedBorderCount(borderCounts, nodes) {
  if ((!Array.isArray(borderCounts) && !(borderCounts instanceof Int32Array)) || borderCounts.length !== 4 ||
      Array.from(borderCounts).some(count => !Number.isInteger(count) || count < 0) || borderCounts[0] < 2 || borderCounts[1] < 2) {
    throw new Error('Contour preparation needs the four fixed border counts.');
  }
  const border = Array.from(borderCounts).reduce((sum, count) => sum + count, 0);
  if (border > nodes.length) throw new Error('Border counts must fit the shared nodes.');
  return border;
}

function collectNearBorderNodes(width, height, nodes, border) {
  const selected = [], rightThreshold = width - borderThreshold, bottomThreshold = height - borderThreshold;
  for (let node = border; node < nodes.length; node++) {
    const { x, y } = nodes[node];
    // x87 loads the float32 threshold into double precision. The apparent
    // bottom-edge float casts in the decompiler are absent from assembly.
    if (x < borderThreshold || rightThreshold < x || y < borderThreshold || bottomThreshold < y) selected.push(node);
  }
  return Int32Array.from(selected);
}

/** Record the fixed near-border list before optimization moves any nodes. */
export function selectNearBorderNodes(input) {
  const { width, height } = readContourDimensions(input);
  const nodes = input.geometry?.nodes;
  if (!Array.isArray(nodes) || nodes.length < 4) throw new Error('Contour preparation needs shared nodes.');
  const border = readFixedBorderCount(input.borderCounts, nodes);
  for (const node of nodes) if (!node || !Number.isFinite(node.x) || !Number.isFinite(node.y)) {
    throw new Error('Near-border selection needs finite node coordinates.');
  }
  return collectNearBorderNodes(width, height, nodes, border);
}

/** Own the raw grid geometry and fixed caches used by all smoothing phases. */
export function createContourSmoothingState(input) {
  const { width, height } = readContourDimensions(input);
  if (typeof input.antiAlias !== 'boolean') throw new Error('Contour preparation requires antiAlias: true or false.');
  if (!Array.isArray(input.geometry?.nodes) || input.geometry.nodes.length < 4 ||
      !Array.isArray(input.geometry.regions) || input.geometry.regions.length < 1) {
    throw new Error('Contour preparation needs raw shared geometry.');
  }
  const geometry = structuredClone(input.geometry), borderCounts = new Int32Array(4);
  const referencePositions = new Float32Array(geometry.nodes.length * 2);
  let previousGroup = 0;
  for (const [index, node] of geometry.nodes.entries()) {
    if (!node || !Number.isInteger(node.x) || !Number.isInteger(node.y) || node.x < 0 || node.x > width || node.y < 0 || node.y > height ||
        (node.kind !== 0 && node.kind !== 3) || !Number.isInteger(node.flags) || node.flags < 0 || node.flags > 255) {
      throw new Error('Contour preparation needs integer-grid nodes, raw kinds and byte flags.');
    }
    let group = 4;
    if (node.x === 0) group = 0;
    else if (node.x === width) group = 1;
    else if (node.y === 0) group = 2;
    else if (node.y === height) group = 3;
    if (group < previousGroup) throw new Error('Raw contour nodes must follow left/right/top/bottom/interior order.');
    previousGroup = group;
    if (group < 4) borderCounts[group]++;
    referencePositions[index * 2] = node.x;
    referencePositions[index * 2 + 1] = node.y;
  }
  const border = readFixedBorderCount(borderCounts, geometry.nodes);
  const nearBorderNodes = collectNearBorderNodes(width, height, geometry.nodes, border);
  return { geometry, referencePositions, penaltyCounts:new Uint8Array(geometry.nodes.length), borderCounts,
    nearBorderNodes, repairInterval:10 };
}

// Native 0x4822b0 writes neighborhood flags after the first preparation
// checkpoint. Keep this producer separate from the raw contour state.
export function initializeSmoothingNeighborhoods(state, antiAlias) {
  const { geometry } = state;
  const prepared = antiAlias ? prepareContourNeighborhoods({ geometry }) :
    { nearbyEdges:Array.from(geometry.nodes, () => []) };
  if (antiAlias) for (const [index, node] of geometry.nodes.entries()) node.flags = prepared.flags[index];
  state.nearbyEdges = prepared.nearbyEdges;
  return state;
}

/** Prepare all fixed caches for callers that do not observe interruption. */
export function prepareContourSmoothing(input) {
  return initializeSmoothingNeighborhoods(createContourSmoothingState(input), input.antiAlias);
}

function readInitialization(input) {
  const { width, height, bgra } = readRasterInput(input);
  const { geometry, indices, referencePositions, borderCounts, parameters } = input;
  const palette = readPaletteColors(input.palette);
  if (width < 2 || height < 2 || !geometry || !Array.isArray(geometry.nodes) || geometry.nodes.length < 4 ||
      !Array.isArray(geometry.regions) || geometry.regions.length < 1) {
    throw new Error('Contour initialization needs dimensions of at least two and shared geometry.');
  }
  const positions = new Float64Array(geometry.nodes.length * 2);
  for (let index = 0; index < geometry.nodes.length; index++) {
    const node = geometry.nodes[index];
    if (!node || !Number.isFinite(node.x) || !Number.isFinite(node.y)) {
      throw new Error('Contour initialization needs finite node coordinates.');
    }
    positions[index * 2] = node.x;
    positions[index * 2 + 1] = node.y;
  }
  if ((!Array.isArray(referencePositions) && !(referencePositions instanceof Float32Array) && !(referencePositions instanceof Float64Array)) ||
      referencePositions.length !== positions.length) throw new Error('Contour initialization needs two float32 references per node.');
  for (const value of referencePositions) if (!Number.isFinite(value) || !Number.isFinite(Math.fround(value))) {
    throw new Error('Contour references must fit finite float32 coordinates.');
  }
  const references = Float32Array.from(referencePositions);
  if ((!Array.isArray(borderCounts) && !(borderCounts instanceof Int32Array)) || borderCounts.length !== 4 ||
      Array.from(borderCounts).some(count => !Number.isInteger(count) || count < 0) || borderCounts[0] < 2 || borderCounts[1] < 2) {
    throw new Error('Contour initialization needs the four fixed border counts.');
  }
  const border = Array.from(borderCounts).reduce((sum, count) => sum + count, 0);
  if (border > geometry.nodes.length) throw new Error('Border counts must fit the shared nodes.');
  for (let node = border; node < geometry.nodes.length; node++) {
    const column = Math.trunc(references[node * 2]) - 1, row = Math.trunc(references[node * 2 + 1]) - 1;
    if (column < 0 || row < 0 || column + 1 >= width || row + 1 >= height) {
      throw new Error('Interior node references must identify a complete original 2 by 2 pixel joint.');
    }
  }
  if ((!Array.isArray(indices) && !(indices instanceof Int32Array)) || indices.length !== width * height) {
    throw new Error('Contour initialization needs one completed region label per pixel.');
  }
  const antiAlias = parameters?.['Shared::is_anti_aliased'], amplitude = parameters?.['ContourSmoother::perturbation_range'];
  if ((antiAlias !== 0 && antiAlias !== 1) || !Number.isFinite(amplitude) || amplitude < 0) {
    throw new Error('Contour initialization needs an anti-alias flag and finite nonnegative perturbation range.');
  }
  const colorIndices = Array.from(geometry.regions, region => {
    // With anti-aliasing disabled the original skips color sampling entirely.
    // Painted region IDs can then exceed a subsequently reloaded live palette.
    if (!region || !Number.isInteger(region.colorIndex) || region.colorIndex < 0 ||
        (antiAlias && region.colorIndex >= palette.length / 4)) {
      throw new Error('Contour region colors must identify the completed palette.');
    }
    return region.colorIndex;
  });
  const paletteIndices = Int32Array.from(indices, region => {
    if (!Number.isInteger(region) || region < 0 || region >= colorIndices.length) {
      throw new Error('Contour labels must identify existing regions.');
    }
    return colorIndices[region];
  });
  return { width, height, bgra, palette, paletteIndices, positions, references, border, antiAlias, amplitude,
    pixelSample:new Float64Array(4) };
}

function preadjustNode(state, node) {
  const { width, height, bgra, palette, paletteIndices, references, positions, pixelSample } = state;
  const column = Math.trunc(references[node * 2]) - 1, row = Math.trunc(references[node * 2 + 1]) - 1;
  const first = row * width + column;
  const quad = [paletteIndices[first], paletteIndices[first + 1], paletteIndices[first + width], paletteIndices[first + width + 1]];
  const totals = [0, 0, 0, 0];
  for (let dy = 0; dy < 2; dy++) for (let dx = 0; dx < 2; dx++) {
    const pixel = (row + dy) * width + column + dx;
    const selected = collectBlendIndices(paletteIndices, width, height, pixel, eightDirections);
    const colors = new Float32Array(selected.length * 4);
    for (const [index, color] of selected.entries()) colors.set(palette.subarray(color * 4, color * 4 + 4), index * 4);
    writeBlendSample(bgra, pixel * 4, pixelSample);
    const { weights } = fitPreparedBlend(pixelSample, colors);
    // Repeated quad colors contribute repeatedly. The first selected color
    // is the sample's own palette color and never contributes to this shift.
    for (const color of quad) {
      const index = selected.indexOf(color);
      if (index > 0) totals[dy * 2 + dx] = weights[index] + totals[dy * 2 + dx];
    }
  }
  const differenceA = totals[0] - totals[3], differenceB = totals[2] - totals[1];
  const x = (differenceB * -diagonal + differenceA * -diagonal) * 0.5;
  const y = (differenceA * -diagonal + differenceB * diagonal) * 0.5;
  positions[node * 2] = Math.max(-0.333, Math.min(0.333, x)) + positions[node * 2];
  positions[node * 2 + 1] = Math.max(-0.333, Math.min(0.333, y)) + positions[node * 2 + 1];
}

/** Original deterministic blend-driven adjustment, without PRNG consumption. */
export function preadjustContours(input) {
  const state = readInitialization(input);
  if (state.antiAlias) for (let node = state.border; node < state.positions.length / 2; node++) preadjustNode(state, node);
  return { positions:state.positions };
}

// The session owns nextUniform. Keep per-node adjustment and both draws in
// original order so this stage shares one stream with palette operations.
export function initializeContourCoordinates(input, nextUniform) {
  const state = readInitialization(input);
  if (state.antiAlias) for (let node = state.border; node < state.positions.length / 2; node++) {
    preadjustNode(state, node);
    const x = nextUniform(), y = nextUniform(), range = state.amplitude + state.amplitude;
    state.positions[node * 2] = (x * range - state.amplitude) + state.positions[node * 2];
    state.positions[node * 2 + 1] = (range * y - state.amplitude) + state.positions[node * 2 + 1];
  }
  return { positions:state.positions };
}
