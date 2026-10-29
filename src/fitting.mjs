import { fittingThresholdMultiplier } from './extended-arithmetic.mjs';

const quadraticDifferences = [[1], [-1, 1], [1, -2, 1]];
const cubicBinomial = [1, 3, 3, 1];
const cubicDifferences = [[], [-1], [1, -2], [-1, 3, -3]];

// Internal fitting kernels receive the stage's owned geometry. Validating or
// cloning the complete graph for every keeper proposal would repeat work in
// the fitter's hot loop; the eventual public stage owns that boundary once.
export function prepareFittingSamples({ nodes, contour, start, end, markInterior }) {
  const cycle = contour.length;
  while (end < start) end += cycle;
  const count = end - start - 1;
  if (count < 1) return { parameters:new Float64Array(), points:[] };
  const parameters = new Float64Array(count), points = [];
  let previous = nodes[contour[start % cycle]], length = 0;
  for (let sample = 0; sample < count; sample++) {
    const node = nodes[contour[(start + sample + 1) % cycle]];
    const dx = node.x - previous.x, dy = node.y - previous.y;
    length = Math.sqrt(dy * dy + dx * dx) + length;
    parameters[sample] = length;
    points.push([node.x, node.y]);
    if (markInterior) node.kind = 2;
    previous = node;
  }
  const last = nodes[contour[end % cycle]];
  const dx = last.x - previous.x, dy = last.y - previous.y;
  const total = Math.sqrt(dy * dy + dx * dx) + length;
  // Original 0x49c7b0 uses one only for a completely collapsed interval.
  // Preserve reciprocal multiplication instead of per-sample division.
  const inverse = 1 / (total === 0 ? 1 : total);
  for (let sample = 0; sample < count; sample++) parameters[sample] = inverse * parameters[sample];
  return { parameters, points };
}

// The original normal equations use a positive diagonal regularizer and
// LAPACK's unblocked LU path for these two/four-variable systems.
function solveNormalEquations(matrix, right) {
  const size = right.length;
  for (let column = 0; column < size; column++) {
    let pivot = column;
    for (let row = column + 1; row < size; row++) {
      if (Math.abs(matrix[row][column]) > Math.abs(matrix[pivot][column])) pivot = row;
    }
    if (pivot !== column) {
      [matrix[pivot], matrix[column]] = [matrix[column], matrix[pivot]];
      [right[pivot], right[column]] = [right[column], right[pivot]];
    }
    const inverse = 1 / matrix[column][column];
    for (let row = column + 1; row < size; row++) matrix[row][column] *= inverse;
    for (let next = column + 1; next < size; next++) {
      const multiplier = -matrix[column][next];
      for (let row = column + 1; row < size; row++) matrix[row][next] += matrix[row][column] * multiplier;
    }
  }
  for (let column = 0; column < size; column++) {
    if (right[column] !== 0) {
      for (let row = column + 1; row < size; row++) right[row] -= right[column] * matrix[row][column];
    }
  }
  for (let column = size - 1; column >= 0; column--) {
    if (right[column] !== 0) {
      right[column] /= matrix[column][column];
      for (let row = 0; row < column; row++) right[row] -= right[column] * matrix[row][column];
    }
  }
  return right;
}

function fitInteriorControls(first, last, { parameters, points }, quadratic) {
  const size = quadratic ? 2 : 4, design = [], residuals = [];
  for (let sample = 0; sample < points.length; sample++) {
    const t = parameters[sample], inverse = 1 - t;
    const firstWeight = quadratic ? inverse * inverse : inverse * inverse * inverse;
    const lastWeight = quadratic ? t * t : t * t * t;
    const left = quadratic ? inverse * t + inverse * t : inverse * inverse * t * 3;
    const right = t * t * inverse * 3;
    for (let channel = 0; channel < 2; channel++) {
      const row = new Float64Array(size);
      row[channel] = left;
      if (!quadratic) row[channel + 2] = right;
      design.push(row);
      residuals.push(points[sample][channel] - (lastWeight * last[channel] + firstWeight * first[channel]));
    }
  }
  const matrix = Array.from({ length:size }, () => new Float64Array(size)), right = new Float64Array(size);
  // Original 0x5bbd40 accumulates each transposed dot product from zero,
  // then adds the existing diagonal value, rather than seeding the sum with it.
  for (let column = 0; column < size; column++) {
    for (let row = 0; row < size; row++) {
      let sum = 0;
      for (let sample = 0; sample < design.length; sample++) sum = design[sample][row] * design[sample][column] + sum;
      matrix[row][column] = sum + (row === column ? 0.00001 : 0);
    }
    let sum = 0;
    for (let sample = 0; sample < design.length; sample++) sum = design[sample][column] * residuals[sample] + sum;
    right[column] = sum;
  }
  const controls = solveNormalEquations(matrix, right);
  return quadratic ? [first, [...controls], last] : [first, [...controls.slice(0, 2)], [...controls.slice(2)], last];
}

function evaluateCurve(curve, t) {
  const inverse = 1 - t;
  if (curve.length === 3) {
    return [0, 1].map(channel => (t * inverse + t * inverse) * curve[1][channel] +
      inverse * curve[0][channel] * inverse + t * t * curve[2][channel]);
  }
  const weights = [inverse * inverse * inverse, inverse * inverse * t * 3, t * t * inverse * 3, t * t * t];
  return [0, 1].map(channel => ((weights[0] * curve[0][channel] + weights[1] * curve[1][channel]) +
    weights[2] * curve[2][channel]) + weights[3] * curve[3][channel]);
}

function elevateQuadratic(quadratic) {
  const coefficients = Array.from({ length:4 }, () => [0, 0]);
  for (let power = 0; power < 3; power++) {
    for (let channel = 0; channel < 2; channel++) {
      let sum = 0;
      for (let term = 0; term <= power; term++) sum += quadraticDifferences[power][term] * quadratic[term][channel];
      coefficients[power][channel] = sum * (power === 1 ? 2 : 1);
    }
  }
  const curve = [];
  for (let point = 0; point < 4; point++) {
    curve.push(coefficients[point].map(value => value * (1 / cubicBinomial[point])));
    for (let term = 0; term < point; term++) {
      for (let channel = 0; channel < 2; channel++) curve[point][channel] -= cubicDifferences[point][term] * curve[term][channel];
    }
  }
  return curve;
}

export function fitContourInterval(input) {
  const { nodes, contour, start } = input;
  let end = input.end;
  while (end < start) end += contour.length;
  const at = sample => { const node = nodes[contour[sample % contour.length]]; return [node.x, node.y]; };
  let curve = [at(start), at(start + 1), at(end + contour.length - 1), at(end)];
  const samples = prepareFittingSamples(input), count = samples.points.length;
  if (count === 0) return { cost:0, curve };
  if (count <= 2) {
    // The original short-interval branch uses quarter/three-quarter controls,
    // but measures only the first sample against its straight interpolation.
    const first = curve[0], last = curve[3];
    const interpolate = t => [t * last[0] + (1 - t) * first[0], (1 - t) * first[1] + t * last[1]];
    curve[1] = interpolate(0.25); curve[2] = interpolate(0.75);
    const point = interpolate(samples.parameters[0]);
    const dx = point[0] - samples.points[0][0], dy = point[1] - samples.points[0][1];
    return { cost:dy * dy + dx * dx, curve };
  }
  const fitted = fitInteriorControls(curve[0], curve[3], samples, count === 3);
  let cost = 0;
  for (let sample = 0; sample < count; sample++) {
    const point = evaluateCurve(fitted, samples.parameters[sample]);
    const dx = samples.points[sample][0] - point[0], dy = samples.points[sample][1] - point[1];
    cost = (dy * dy + (dx * dx + 0)) + cost;
  }
  curve = count === 3 ? elevateQuadratic(fitted) : fitted;
  return { cost, curve };
}

/** Original keeper partition before shared-node marking and cubic allocation. */
export function partitionContourInterval(input) {
  const { start, contour, initialThreshold, finalThreshold, maxIterations, fraction } = input;
  let end = input.end;
  while (end < start) end += contour.length;
  const slots = end - start + 1;
  const records = Array.from({ length:slots }, (_, slot) => ({ slot, start:slot, count:1,
    next:slot + 1 < slots ? slot + 1 : -1, cost:0, dirty:true }));
  const proposal = { nodes:input.nodes, contour, start:0, end:0, markInterior:false };
  const evaluate = (record, firstDelta = 0, lastDelta = 0) => {
    // Fit calls are synchronous and retain no input object. Reuse the same
    // proposal storage for cached, merged and redistributed intervals.
    proposal.start = start + record.start + firstDelta;
    proposal.end = start + record.start + record.count + lastDelta;
    return fitContourInterval(proposal).cost;
  };
  const cached = record => {
    if (record.dirty) { record.cost = evaluate(record); record.dirty = false; }
    return record.cost;
  };
  const redistributed = (record, firstDelta, lastDelta) => {
    const previous = cached(record), changed = evaluate(record, firstDelta, lastDelta);
    const improvement = previous - changed;
    return lastDelta !== firstDelta && lastDelta - firstDelta >= 0 ? -improvement : improvement;
  };
  let threshold = initialThreshold;
  if (maxIterations > 0) {
    const multiplier = fittingThresholdMultiplier(initialThreshold, finalThreshold, maxIterations, fraction);
    for (let iteration = 0; iteration < maxIterations; iteration++) {
      for (let slot = 0; records[slot].next !== -1 && records[records[slot].next].next !== -1; slot = records[slot].next) {
        const first = records[slot], second = records[first.next];
        const firstCost = cached(first), secondCost = cached(second);
        const merged = evaluate(first, 0, second.count);
        if (merged - (secondCost + firstCost) < threshold) {
          first.count += second.count; first.next = second.next; first.dirty = true;
        } else if (redistributed(second, -1, 0) - redistributed(first, 0, -1) < 0) {
          second.count++; first.count--; second.start--;
          second.dirty = true; first.dirty = true;
          if (second.start <= first.start) throw new Error('Original keeper partition crossed its left boundary.');
        } else if (redistributed(first, 0, 1) - redistributed(second, 1, 0) < 0) {
          first.count++; second.count--; second.start++;
          first.dirty = true; second.dirty = true;
          if ((second.next === -1 || records[second.next].start <= second.start) && slots <= second.start) {
            throw new Error('Original keeper partition crossed its right boundary.');
          }
        }
      }
      threshold = maxIterations * fraction <= iteration ? finalThreshold : multiplier * threshold;
    }
  }
  const active = [];
  for (let slot = 0; slot !== -1; slot = records[slot].next) active.push(records[slot]);
  return { records:active, threshold };
}
