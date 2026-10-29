import { boundaryMask } from './blending.mjs';
import { dotFloat } from './blend-solver.mjs';

const f32 = Math.fround;
const byteToUnit = Float32Array.from({ length:256 }, (_, byte) => f32(byte * f32(1 / 255)));

// The two shadow models use 42 double features. Each numerical tuple chooses
// its first child only for ordered less-than; equal and unordered go second.
const shadowRegionModel = [23, 0.00402429,
  [18, 1.5,
    [14, 1.5, 0, [5, 0.0491163, [20, 1.5, 1, 0], [22, 0.196468, 0, 1]]],
    [20, 0.5, 0,
      [19, 0.5, [1, 0.0112894, 1, 0],
        [41, 0.449972,
          [21, 0.118876, [0, 41.535, 1, 0], [11, 5.5, 1, 0]], 0]]]],
  [23, 0.0241458,
    [5, 0.0473801, [22, 0.0138127, [39, 0.0102382, 0, 1], 1], 0], 0]];

const shadowAcceptanceModel = [37, 3.57554,
  [28, 0.0442212,
    [39, 0.00627504,
      ['control', 1, 1, ['control', 2, 1, [4, 0.934367, 0, 1]]], 1],
    ['control', 0, 1, [34, 0.694472, [0, 3.09081, 0, 1], 0]]],
  [35, 0.00258463, 1, 0]];

function evaluateShadowModel(model, features, control) {
  let node = model;
  while (typeof node !== 'number') {
    const matches = node[0] === 'control' ? control === node[1] : features[node[0]] < node[1];
    node = matches ? node[2] : node[3];
  }
  return node === 1;
}

/** Internal possible-shadow decision over the producer's 42 float64 values. */
export function classifyShadowFeatures(features) {
  return evaluateShadowModel(shadowRegionModel, features);
}

/** Internal refinement acceptance decision. Preserve the original int32
 * control, including values outside 0/1/2; those use native fallthrough paths. */
export function acceptShadowFeatures(features, control) {
  return evaluateShadowModel(shadowAcceptanceModel, features, control);
}

function multiplyColorMatrix(matrix, vector, output) {
  for (let channel = 0; channel < 4; channel++) {
    let sum = 0;
    for (let input = 0; input < 4; input++) sum = f32(sum + f32(matrix[input * 4 + channel] * vector[input]));
    output[channel] = sum;
  }
}

/** Internal principal-color iteration with explicit retained float32 state.
 * Matrix products round each operation to float32; lengths and Rayleigh dot
 * products accumulate in double before their final float32 stores. Native
 * convergence returns the previous eigenvalue without writing the estimate. */
export function principalColorDirection(matrix, previousEigenvalue, iterations, epsilon) {
  const direction = new Float32Array(4).fill(1), working = new Float32Array(4);
  multiplyColorMatrix(matrix, direction, working);
  let eigenvalue = previousEigenvalue;
  for (let iteration = 0; iteration < iterations; iteration++) {
    let squaredLength = 0;
    for (const coordinate of working) squaredLength += coordinate * coordinate;
    const length = f32(Math.sqrt(squaredLength));
    if (length > 0) {
      const reciprocal = f32(1 / length);
      for (let channel = 0; channel < 4; channel++) working[channel] *= reciprocal;
    }
    direction.set(working);
    multiplyColorMatrix(matrix, direction, working);
    let rayleigh = 0;
    for (let channel = 0; channel < 4; channel++) rayleigh += working[channel] * direction[channel];
    const estimate = f32(rayleigh);
    if (Math.abs(eigenvalue - estimate) < epsilon) return { eigenvalue, direction };
    eigenvalue = estimate;
  }
  return { eigenvalue, direction };
}

/** Internal pre-refinement shadow statistics over a rebuilt eight-neighbor
 * subpixel state. The caller owns prepared pixels, matching gradient maps and
 * the actual initial convergence float; no original zero-initialization rule
 * exists. Returning covariance/direction state keeps native intermediate
 * comparisons possible without duplicating the producer's arithmetic. */
export function measureShadowFeatures({ width, height, bgra }, { indices, regions },
  { gradient, blurGradient }, initialEigenvalue) {
  const features = regions.map(() => new Float64Array(42));
  const covariances = regions.map(() => new Float32Array(16));
  const means = regions.map(() => new Float32Array(4));
  const directions = [];
  // Eligible anchor order is stable throughout this immutable prepared state.
  const colorAnchors = regions.filter(statistic => statistic.interiorCount > 10);
  const mask = boundaryMask(width, height, indices), sample = new Float32Array(4);
  for (let pixel = 0; pixel < indices.length; pixel++) {
    const region = indices[pixel], values = features[region], matrix = covariances[region];
    values[0] += gradient[pixel]; values[1] += blurGradient[pixel];
    values[6] += Math.sqrt(gradient[pixel]); values[7] += blurGradient[pixel];
    if (mask[pixel] === 0) values[10]++;
    for (let channel = 0; channel < 4; channel++) sample[channel] = byteToUnit[bgra[pixel * 4 + channel]];
    for (let row = 0; row < 4; row++) {
      for (let column = 0; column < 4; column++) matrix[row * 4 + column] += f32(sample[row] * sample[column]);
    }
  }
  let eigenvalue = initialEigenvalue;
  for (const [region, statistic] of regions.entries()) {
    const reciprocal = f32(1 / f32(statistic.count)), mean = means[region], matrix = covariances[region];
    for (let channel = 0; channel < 4; channel++) mean[channel] = statistic.sums[channel] * reciprocal;
    for (let row = 0; row < 4; row++) {
      for (let column = 0; column < 4; column++) {
        const offset = row * 4 + column;
        matrix[offset] = f32(matrix[offset] * reciprocal) - f32(mean[row] * mean[column]);
      }
    }
    const principal = principalColorDirection(matrix, eigenvalue, 10, f32(1e-6));
    directions.push(principal.direction);
    eigenvalue = principal.eigenvalue;
  }
  const residual = new Float32Array(4);
  for (let pixel = 0; pixel < indices.length; pixel++) {
    const region = indices[pixel], mean = means[region], direction = directions[region];
    for (let channel = 0; channel < 4; channel++) {
      sample[channel] = byteToUnit[bgra[pixel * 4 + channel]];
      residual[channel] = sample[channel] - mean[channel];
    }
    const projection = dotFloat(residual, direction);
    let squaredError = 0;
    for (let channel = 0; channel < 4; channel++) {
      residual[channel] = sample[channel] - f32(f32(direction[channel] * projection) + mean[channel]);
      squaredError += residual[channel] * residual[channel];
    }
    features[region][41] += squaredError;
  }
  for (const [region, statistic] of regions.entries()) {
    const values = features[region], mean = means[region], color = statistic.color;
    values[2] = Math.sqrt(values[0]); values[3] = Math.sqrt(values[1]);
    values[4] = Math.sqrt(values[0] / statistic.count); values[5] = Math.sqrt(values[1] / statistic.count);
    values[8] = statistic.count; values[9] = statistic.interiorCount;
    for (let feature = 0; feature < 8; feature++) values[25 + feature] = values[feature] / statistic.count;
    values[39] = 1e100;
    for (const candidate of colorAnchors) {
      let distance = 0;
      for (let channel = 0; channel < 4; channel++) {
        const difference = f32(candidate.color[channel] - color[channel]);
        distance += difference * difference;
      }
      if (distance < values[39]) values[39] = distance;
    }
    // The producer uses different double addition orders for the source and
    // neighboring luminances. Rounding either to float32 changes comparisons.
    values[21] = (color[0] * 0.11 + color[1] * 0.59) + color[2] * 0.3;
    const reciprocal = f32(1 / f32(statistic.count));
    for (let channel = 0; channel < 4; channel++) {
      values[22] += f32(f32(statistic.squares[channel] * reciprocal) - f32(mean[channel] * mean[channel]));
    }
    values[23] = statistic.interiorCount / statistic.boundary.length;
    const neighbors = new Set();
    let minimumX = width, minimumY = height, maximumX = 0, maximumY = 0;
    for (const pixel of statistic.boundary) {
      neighbors.add(indices[pixel]);
      const x = pixel % width, y = Math.trunc(pixel / width);
      minimumX = Math.min(minimumX, x); minimumY = Math.min(minimumY, y);
      maximumX = Math.max(maximumX, x); maximumY = Math.max(maximumY, y);
    }
    values[24] = (maximumX - minimumX) / (maximumY - minimumY);
    values[11] = neighbors.size - 1;
    for (const neighbor of neighbors) {
      if (neighbor === region) continue;
      const candidate = regions[neighbor];
      if (candidate.interiorCount === 0) values[12]++;
      if (candidate.count > 4) values[13]++;
      if (candidate.count > 9) values[14]++;
      if (candidate.count > 14) values[15]++;
      if (candidate.interiorCount > 4) values[16]++;
      if (candidate.interiorCount > 9) values[17]++;
      if (candidate.interiorCount > 14) values[18]++;
      const neighborColor = candidate.color;
      const luminance = (neighborColor[2] * 0.3 + neighborColor[0] * 0.11) + neighborColor[1] * 0.59;
      values[luminance > values[21] ? 19 : 20]++;
    }
  }
  return { features, covariances, directions, eigenvalue };
}

const assignmentDirections = [[-1, 0], [1, 0], [0, -1], [0, 1]];

/** Internal four-neighbor proposal. A rejected proposal leaves the caller's
 * target untouched, as in the native cost/target pointer contract. */
export function writeShadowProposal({ width, height, bgra, indices, shadowFlags }, pixel, proposal) {
  const x = pixel % width, y = Math.trunc(pixel / width);
  proposal.cost = f32(1e10);
  let accepted = false;
  for (const [dx, dy] of assignmentDirections) {
    const column = x + dx, row = y + dy;
    if (column < 0 || column >= width || row < 0 || row >= height) continue;
    const neighbor = row * width + column, target = indices[neighbor];
    if (shadowFlags[target] & 1) continue;
    let cost = 0;
    for (let channel = 0; channel < 4; channel++) {
      const difference = f32(byteToUnit[bgra[neighbor * 4 + channel]] - byteToUnit[bgra[pixel * 4 + channel]]);
      cost = f32(cost + f32(difference * difference));
    }
    if (cost < proposal.cost) {
      proposal.cost = cost; proposal.target = target; accepted = true;
    }
  }
  return accepted;
}

/** Fill shadow pixels before acceptance. Strict comparisons preserve pending
 * pixel and neighbor order on ties. The copied map changes after each chosen
 * proposal, making the next scan observe the newly assigned frontier. */
export function reassignShadowPixels(raster, originalIndices, shadowFlags) {
  const indices = Int32Array.from(originalIndices), assignments = [];
  const context = { ...raster, indices, shadowFlags }, proposal = { cost:0, target:-1 };
  for (let region = 0; region < shadowFlags.length; region++) {
    if (!(shadowFlags[region] & 1)) continue;
    const pending = [];
    for (let pixel = 0; pixel < indices.length; pixel++) if (indices[pixel] === region) pending.push(pixel);
    while (pending.length > 0) {
      let selected = -1, target = -1, cost = Infinity;
      for (let position = 0; position < pending.length; position++) {
        if (writeShadowProposal(context, pending[position], proposal) && proposal.cost < cost) {
          selected = position; target = proposal.target; cost = proposal.cost;
        }
      }
      if (selected < 0) throw new Error('Shadow region has no non-shadow boundary.');
      const pixel = pending[selected];
      indices[pixel] = target;
      assignments.push({ pixel, target, cost });
      pending.splice(selected, 1);
    }
  }
  return { indices, assignments };
}
