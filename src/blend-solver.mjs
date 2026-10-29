const f32 = Math.fround;
const byteScale = f32(1 / 255);

export const eightDirections = [[1, 0], [0, 1], [-1, 0], [0, -1], [-1, 1], [1, -1], [-1, -1], [1, 1]];
export const twentyFourDirections = [...eightDirections,
  [-2, -2], [-2, -1], [-2, 0], [-2, 1], [-2, 2],
  [2, -2], [2, -1], [2, 0], [2, 1], [2, 2],
  [-1, -2], [0, -2], [1, -2], [-1, 2], [0, 2], [1, 2]];

/** Internal kernels take validated storage and never repeat public checks. */
export function writeBlendSample(bgra, offset, sample) {
  for (let channel = 0; channel < 4; channel++) {
    const value = bgra[offset + channel] * byteScale;
    sample[channel] = channel < 2 ? value : f32(value);
  }
}

export function collectBlendIndices(indices, width, height, pixel, directions, source = -1, target = -1) {
  const selected = [indices[pixel] === source ? target : indices[pixel]];
  const x = pixel % width, y = Math.floor(pixel / width);
  for (const [dx, dy] of directions) {
    const column = x + dx, row = y + dy;
    if (column < 0 || column >= width || row < 0 || row >= height) continue;
    const region = indices[row * width + column];
    const color = region === source ? target : region;
    if (!selected.includes(color)) {
      selected.push(color);
      if (selected.length === 9) break;
    }
  }
  return selected;
}

export function dotFloat(first, second) {
  let value = f32(first[0] * second[0]);
  for (let channel = 1; channel < 4; channel++) value = f32(value + f32(first[channel] * second[channel]));
  return value;
}

function fitTwoColors(pixel, colors) {
  const difference = Float32Array.from({ length: 4 }, (_, channel) => f32(colors[channel + 4] - colors[channel]));
  const norm = dotFloat(difference, difference);
  if (norm <= 0) return { color: colors.slice(0, 4), weights: new Float64Array([0.5, 0.5]), selfWeight: 0.5 };
  const relativePixel = Float32Array.from(pixel, (value, channel) => f32(f32(value) - colors[channel]));
  let secondWeight = dotFloat(difference, relativePixel) / norm;
  secondWeight = (1 - (secondWeight + secondWeight)) * (0.01 / (norm + 0.02)) + secondWeight;
  secondWeight = Math.max(0, Math.min(1, secondWeight));
  const firstWeight = 1 - secondWeight;
  const roundedWeight = f32(secondWeight);
  const color = Float32Array.from(difference, (value, channel) => f32(colors[channel] + f32(roundedWeight * value)));
  return { color, weights: new Float64Array([firstWeight, secondWeight]), selfWeight: firstWeight };
}

// Column-major storage and loop order follow the native small-matrix routines.
function factorPositiveMatrix(matrix, size) {
  const factor = Float64Array.from(matrix);
  for (let row = 0; row < size; row++) {
    for (let column = 0; column < row; column++) {
      const offset = column * size + row;
      for (let previous = 0; previous < column; previous++) {
        factor[offset] -= factor[previous * size + row] * factor[previous * size + column];
      }
      factor[offset] /= factor[column * size + column];
    }
    const diagonal = row * size + row;
    for (let column = 0; column < row; column++) {
      const value = factor[column * size + row];
      factor[diagonal] -= value * value;
    }
    if (factor[diagonal] <= 0) throw new Error('Color-blend matrix is not positive definite.');
    factor[diagonal] = Math.sqrt(factor[diagonal]);
  }
  return factor;
}

function solveFactored(factor, rightHandSide, size) {
  const solved = Float64Array.from(rightHandSide);
  for (let row = 0; row < size; row++) {
    // Native forward substitution subtracts previous terms in reverse order.
    let value = solved[row];
    for (let column = row - 1; column >= 0; column--) value -= solved[column] * factor[column * size + row];
    solved[row] = value / factor[row * size + row];
  }
  for (let row = size - 1; row >= 0; row--) {
    let value = solved[row];
    for (let column = row + 1; column < size; column++) value -= solved[column] * factor[row * size + column];
    solved[row] = value / factor[row * size + row];
  }
  return solved;
}

function fitMultipleColors(pixel, colors) {
  const count = colors.length / 4;
  const gram = new Float64Array(count * count);
  const projection = new Float64Array(count);
  for (let column = 0; column < count; column++) {
    for (let row = 0; row < count; row++) {
      let value = 0;
      for (let channel = 0; channel < 3; channel++) value += colors[row * 4 + channel] * colors[column * 4 + channel];
      gram[column * count + row] = value + (row === column ? 0.01 : 0);
    }
    for (let channel = 0; channel < 3; channel++) projection[column] += colors[column * 4 + channel] * pixel[channel];
  }
  const factor = factorPositiveMatrix(gram, count);
  const constrained = new Set();
  const constraints = [new Float64Array(count).fill(1)];
  // The factor and existing constraints stay fixed as new bounds are added.
  const solvedConstraints = [solveFactored(factor, constraints[0], count)];
  let weights;
  while (true) {
    const constraintCount = constraints.length;
    const schur = new Float64Array(constraintCount * constraintCount);
    const rightHandSide = new Float64Array(constraintCount);
    for (let column = 0; column < constraintCount; column++) {
      for (let row = 0; row < constraintCount; row++) {
        for (let color = 0; color < count; color++) {
          schur[column * constraintCount + row] += constraints[row][color] * solvedConstraints[column][color];
        }
      }
      for (let color = 0; color < count; color++) rightHandSide[column] += solvedConstraints[column][color] * projection[color];
    }
    rightHandSide[0] -= 1;
    const multipliers = solveFactored(factorPositiveMatrix(schur, constraintCount), rightHandSide, constraintCount);
    const adjustedProjection = Float64Array.from(projection);
    for (let constraint = 0; constraint < constraintCount; constraint++) {
      const multiplier = -multipliers[constraint];
      for (let color = 0; color < count; color++) adjustedProjection[color] += multiplier * constraints[constraint][color];
    }
    weights = solveFactored(factor, adjustedProjection, count);
    let mostNegative = -1;
    let minimumWeight = 0;
    for (let color = 0; color < count; color++) {
      if (!constrained.has(color) && weights[color] < minimumWeight) {
        minimumWeight = weights[color];
        mostNegative = color;
      }
    }
    if (mostNegative < 0 || weights[mostNegative] >= -0.0001) break;
    constrained.add(mostNegative);
    const constraint = new Float64Array(count);
    constraint[mostNegative] = 1;
    constraints.push(constraint);
    solvedConstraints.push(solveFactored(factor, constraint, count));
  }
  const color = Float32Array.from(pixel);
  for (let channel = 0; channel < 3; channel++) {
    let value = 0;
    for (let entry = 0; entry < count; entry++) value += weights[entry] * colors[entry * 4 + channel];
    color[channel] = value;
  }
  return { color, weights, selfWeight: weights[0] };
}

export function fitPreparedBlend(pixel, colors) {
  if (colors.length === 4) return { color: colors, weights: new Float64Array([1]), selfWeight: 1 };
  if (colors.length === 8) return fitTwoColors(pixel, colors);
  return fitMultipleColors(pixel, colors);
}

