import { readImageKernel } from './image-measurement-kernel.mjs';
import { createContourRepair } from './smoothing-repair.mjs';
export { prepareContourNeighborhoods } from './contour-neighborhoods.mjs';
export { preadjustContours, prepareContourSmoothing, selectNearBorderNodes } from './contour-initialization.mjs';
export { punctureContourCorners } from './contour-corners.mjs';

function readPriorGeometry(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected smoothing-prior coordinates and controls.');
  const { points, sampleCounts } = input;
  if (!Array.isArray(points) || points.length !== 3) throw new Error('Smoothing prior needs three coordinate pairs.');
  for (const point of points) {
    if ((!Array.isArray(point) && !(point instanceof Float64Array)) || point.length !== 2 ||
        !Number.isFinite(point[0]) || !Number.isFinite(point[1])) {
      throw new Error('Smoothing coordinates must be finite double x/y pairs.');
    }
  }
  if ((!Array.isArray(sampleCounts) && !(sampleCounts instanceof Int32Array)) || sampleCounts.length !== 2 ||
      !Number.isInteger(sampleCounts[0]) || sampleCounts[0] < 1 || sampleCounts[0] > 2147483647 ||
      !Number.isInteger(sampleCounts[1]) || sampleCounts[1] < 1 || sampleCounts[1] > 2147483647) {
    throw new Error('Smoothing prior needs two positive int32 sample counts.');
  }
  return { points, sampleCounts };
}

/** Original type-3 prior before phase strength or total-objective assembly. */
export function curvaturePrior(input) {
  const { points, sampleCounts } = readPriorGeometry(input);
  const { lengthWeight } = input;
  if (!Number.isFinite(lengthWeight) || lengthWeight < 0) throw new Error('Curvature lengthWeight must be finite and nonnegative.');
  return curvatureTerms(points[0], points[1], points[2], sampleCounts, lengthWeight);
}

function curvatureTerms(first, middle, last, sampleCounts, lengthWeight, includeGradient = true) {
  const incomingX = middle[0] - first[0], incomingY = middle[1] - first[1];
  const outgoingX = last[0] - middle[0], outgoingY = last[1] - middle[1];
  const incomingLength = Math.sqrt(incomingX * incomingX + incomingY * incomingY);
  const outgoingLength = Math.sqrt(outgoingX * outgoingX + outgoingY * outgoingY);
  const incomingInverse = 1 / incomingLength, outgoingInverse = 1 / outgoingLength;
  const incomingUnitX = incomingX * incomingInverse, incomingUnitY = incomingY * incomingInverse;
  const outgoingUnitX = outgoingX * outgoingInverse, outgoingUnitY = outgoingY * outgoingInverse;
  const dot = outgoingUnitX * incomingUnitX + outgoingUnitY * incomingUnitY;
  const curvature = Math.sqrt((2 - (dot + dot)) + 0.001);
  const lengthDifference = outgoingLength / sampleCounts[1] - incomingLength / sampleCounts[0];
  const energy = (lengthDifference * lengthWeight) * lengthDifference + curvature;
  if (!includeGradient) return { energy };
  const curvatureInverse = 1 / curvature;

  // Assembly, rather than the decompiler's regrouped expressions, determines
  // the endpoint product order. Regrouping changes low bits of derivatives.
  let firstX = (-(incomingUnitX * dot) + outgoingUnitX) * incomingInverse * curvatureInverse;
  let firstY = (-(incomingUnitY * dot) + outgoingUnitY) * incomingInverse * curvatureInverse;
  let lastX = (-(outgoingUnitX * dot) + incomingUnitX) * outgoingInverse * -1 * curvatureInverse;
  let lastY = (-(outgoingUnitY * dot) + incomingUnitY) * outgoingInverse * -1 * curvatureInverse;
  const weightedDifference = lengthDifference * lengthWeight;
  const lengthDerivative = weightedDifference + weightedDifference;
  const incomingSampleInverse = 1 / sampleCounts[0], outgoingSampleInverse = 1 / sampleCounts[1];
  firstX = incomingUnitX * lengthDerivative * incomingSampleInverse + firstX;
  firstY = incomingUnitY * lengthDerivative * incomingSampleInverse + firstY;
  lastX = outgoingUnitX * lengthDerivative * outgoingSampleInverse + lastX;
  lastY = outgoingUnitY * lengthDerivative * outgoingSampleInverse + lastY;
  return { energy, firstGradient:Float64Array.of(firstX, firstY), lastGradient:Float64Array.of(lastX, lastY) };
}

/** Original quadratic fallback for small regions and flagged inversion penalties. */
export function quadraticPrior(input) {
  const { points, sampleCounts } = readPriorGeometry(input);
  return quadraticTerms(points[0], points[1], points[2], sampleCounts);
}

function quadraticTerms(first, middle, last, sampleCounts, includeGradient = true) {
  const incomingInverse = 1 / sampleCounts[0], outgoingInverse = 1 / sampleCounts[1];
  const changeX = (last[0] - middle[0]) * outgoingInverse - (middle[0] - first[0]) * incomingInverse;
  const changeY = (last[1] - middle[1]) * outgoingInverse - (middle[1] - first[1]) * incomingInverse;
  const energy = changeY * changeY + changeX * changeX;
  if (!includeGradient) return { energy };
  const twiceX = changeX + changeX, twiceY = changeY + changeY;
  return { energy,
    firstGradient:Float64Array.of(twiceX * incomingInverse, twiceY * incomingInverse),
    lastGradient:Float64Array.of(twiceX * outgoingInverse, twiceY * outgoingInverse) };
}

function readObjectiveInput(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected prepared smoothing-objective state.');
  const { geometry, referencePositions, penaltyCounts, borderCounts, parameters, phase } = input;
  if (!Number.isInteger(phase) || phase < 0 || phase > 2) throw new Error('Smoothing phase must be 0, 1 or 2.');
  if (geometry === null || typeof geometry !== 'object' || !Array.isArray(geometry.nodes) || geometry.nodes.length < 4 ||
      !Array.isArray(geometry.regions) || geometry.regions.length === 0) throw new Error('Smoothing objective needs nodes and region contours.');
  const nodeCount = geometry.nodes.length;
  const points = geometry.nodes.map(node => {
    if (node === null || typeof node !== 'object' || !Number.isFinite(node.x) || !Number.isFinite(node.y) ||
        !Number.isInteger(node.flags) || node.flags < 0 || node.flags > 255) throw new Error('Smoothing nodes need finite x/y and byte flags.');
    return [node.x, node.y];
  });
  if ((!Array.isArray(referencePositions) && !(referencePositions instanceof Float32Array) && !(referencePositions instanceof Float64Array)) ||
      referencePositions.length !== nodeCount * 2) throw new Error('Reference positions need two finite float32 channels per node.');
  const references = new Float32Array(referencePositions.length);
  for (let index = 0; index < references.length; index++) {
    if (!Number.isFinite(referencePositions[index]) || !Number.isFinite(Math.fround(referencePositions[index]))) {
      throw new Error('Reference positions need two finite float32 channels per node.');
    }
    references[index] = referencePositions[index];
  }
  if ((!Array.isArray(penaltyCounts) && !(penaltyCounts instanceof Uint8Array)) || penaltyCounts.length !== nodeCount ||
      Array.from(penaltyCounts).some(count => !Number.isInteger(count) || count < 0 || count > 255)) {
    throw new Error('Smoothing penaltyCounts must contain one byte count per node.');
  }
  if ((!Array.isArray(borderCounts) && !(borderCounts instanceof Int32Array)) || borderCounts.length !== 4 ||
      Array.from(borderCounts).some(count => !Number.isInteger(count) || count < 0) ||
      borderCounts[0] < 2 || borderCounts[1] < 2 || Array.from(borderCounts).reduce((sum, count) => sum + count, 0) > nodeCount) {
    throw new Error('Smoothing borderCounts must describe the left, right, top and bottom node groups.');
  }
  for (const region of geometry.regions) {
    if (region === null || typeof region !== 'object' || !Number.isInteger(region.pixelCount) || region.pixelCount < 1 ||
        region.pixelCount > 2147483647 || (!Array.isArray(region.contour) && !(region.contour instanceof Int32Array)) ||
        region.contour.length < 3 || !Array.isArray(region.boundary) || region.boundary.length !== region.contour.length) {
      throw new Error('Smoothing regions need positive pixel counts and matching contour/boundary records.');
    }
    for (const node of region.contour) if (!Number.isInteger(node) || node < 0 || node >= nodeCount) throw new Error('Smoothing contours must identify existing nodes.');
    for (const edge of region.boundary) {
      if ((!Array.isArray(edge) && !(edge instanceof Int32Array)) || edge.length !== 3 || !Number.isInteger(edge[0]) ||
          edge[0] < 0 || edge[0] > 255 || !Number.isInteger(edge[2]) || edge[2] < 1 || edge[2] > 2147483647) {
        throw new Error('Smoothing boundaries need byte flags and positive int32 sample counts.');
      }
    }
  }
  if (parameters === null || typeof parameters !== 'object') throw new Error('Expected numeric smoothing parameters.');
  const phaseValue = name => {
    const values = parameters[`ContourSmoother::${name}`];
    if (!Array.isArray(values) || values.length !== 3) throw new Error(`${name} must supply three phase values.`);
    return values[phase];
  };
  const nonnegative = (value, name) => {
    if (!Number.isFinite(value) || value < 0) throw new Error(`${name} must be finite and nonnegative.`);
    return value;
  };
  const measurement = phaseValue('measurement_types'), prior = phaseValue('prior_types');
  if (measurement !== 0 && measurement !== 1 && measurement !== 2) throw new Error('This smoothing measurement remains under reconstruction.');
  if (prior !== 0 && prior !== 3) throw new Error('This smoothing prior remains under reconstruction.');
  const settings = { measurement, prior, strength:nonnegative(phaseValue('prior_strengths'), 'prior_strengths'),
    lengthWeight:nonnegative(phaseValue('length_penalty_weights'), 'length_penalty_weights'),
    pressure:nonnegative(parameters['ContourSmoother::air_pressure_weight'], 'air_pressure_weight'),
    inversionPrior:nonnegative(parameters['ContourSmoother::anti_inv_pot_prior_scale'], 'anti_inv_pot_prior_scale'),
    inversionMeasurement:nonnegative(parameters['ContourSmoother::anti_inv_pot_meas_scale'], 'anti_inv_pot_meas_scale') };
  const imageKernel = measurement === 1 ? readImageKernel(input.imageModel, geometry) : undefined;
  return { geometry, points, references, penaltyCounts, borderCounts, settings, imageKernel,
    imagePositions:imageKernel ? new Float64Array(nodeCount * 2) : undefined };
}

function addEndpointGradient(gradient, first, middle, last, terms, scale) {
  const firstX = terms.firstGradient[0] * scale, firstY = terms.firstGradient[1] * scale;
  const lastX = terms.lastGradient[0] * scale, lastY = terms.lastGradient[1] * scale;
  gradient[first * 2] = firstX + gradient[first * 2];
  gradient[first * 2 + 1] = firstY + gradient[first * 2 + 1];
  // The original subtracts endpoint derivatives separately at the middle node.
  gradient[middle * 2] -= firstX;
  gradient[middle * 2 + 1] -= firstY;
  gradient[middle * 2] -= lastX;
  gradient[middle * 2 + 1] -= lastY;
  gradient[last * 2] = lastX + gradient[last * 2];
  gradient[last * 2 + 1] = lastY + gradient[last * 2 + 1];
}

function regionArea(points, contour) {
  let twiceArea = 0;
  for (let sample = 0; sample < contour.length; sample++) {
    const current = points[contour[sample]], next = points[contour[(sample + 1) % contour.length]];
    twiceArea = (next[1] * current[0] - current[1] * next[0]) + twiceArea;
  }
  return twiceArea * 0.5;
}

/** Original prepared objective, including optional persistent image measurement. */
export function smoothingObjective(input) {
  return objectiveTerms(readObjectiveInput(input));
}

function objectiveTerms({ geometry, points, references, penaltyCounts, borderCounts, settings, imageKernel, imagePositions },
  mode = 'both') {
  const includeGradient = mode !== 'energy', includeEnergy = mode !== 'gradient';
  const gradient = includeGradient ? new Float64Array(points.length * 2) : undefined;
  const barrierScale = settings.inversionPrior / settings.strength;
  let energy = 0;
  for (const region of geometry.regions) {
    const { contour, boundary } = region;
    for (let sample = 0; sample < contour.length; sample++) {
      const first = contour[(sample + contour.length - 1) % contour.length], middle = contour[sample];
      const nextSample = (sample + 1) % contour.length, last = contour[nextSample];
      const counts = [boundary[sample][2], boundary[nextSample][2]];
      let quadratic;
      if (geometry.nodes[middle].flags & 16) {
        quadratic = quadraticTerms(points[first], points[middle], points[last], counts, includeGradient);
        if (includeEnergy) energy = quadratic.energy * barrierScale + energy;
        if (includeGradient) addEndpointGradient(gradient, first, middle, last, quadratic, barrierScale);
      }
      let terms;
      if (region.pixelCount < 8 || (!(boundary[sample][0] & 1) && settings.prior === 0)) {
        terms = quadratic ?? quadraticTerms(points[first], points[middle], points[last], counts, includeGradient);
      } else if (!(boundary[sample][0] & 1)) {
        terms = curvatureTerms(points[first], points[middle], points[last], counts, settings.lengthWeight, includeGradient);
      }
      if (terms) {
        if (includeEnergy) energy = terms.energy + energy;
        if (includeGradient) addEndpointGradient(gradient, first, middle, last, terms, 1);
      }
    }
  }
  if (includeEnergy) energy *= settings.strength;
  if (includeGradient) for (let index = 0; index < gradient.length; index++) gradient[index] = settings.strength * gradient[index];
  const pressureScale = settings.strength * settings.pressure;
  for (const region of geometry.regions) {
    if (region.pixelCount >= 8) continue;
    const difference = regionArea(points, region.contour) - region.pixelCount;
    if (includeEnergy) energy = difference * difference * pressureScale + energy;
    if (!includeGradient) continue;
    const weighted = difference * pressureScale, factor = weighted + weighted;
    for (let sample = 0; sample < region.contour.length; sample++) {
      const current = region.contour[sample], next = region.contour[(sample + 1) % region.contour.length];
      gradient[current * 2] = points[next][1] * 0.5 * factor + gradient[current * 2];
      gradient[current * 2 + 1] = points[next][0] * -0.5 * factor + gradient[current * 2 + 1];
      gradient[next * 2] = points[current][1] * -0.5 * factor + gradient[next * 2];
      gradient[next * 2 + 1] = points[current][0] * 0.5 * factor + gradient[next * 2 + 1];
    }
  }
  if (settings.measurement === 1) {
    for (let node = 0; node < points.length; node++) {
      imagePositions[node * 2] = points[node][0];
      imagePositions[node * 2 + 1] = points[node][1];
    }
    if (includeEnergy) energy = imageKernel.energy(imagePositions) + energy;
    // Add pixel contributions into the existing prior gradient in native order.
    // Gradient-only refreshes reuse records from the preceding energy call.
    if (includeGradient) imageKernel.addGradient(gradient, imagePositions);
  } else for (let node = 0; node < points.length; node++) {
    const dx = points[node][0] - references[node * 2], dy = points[node][1] - references[node * 2 + 1];
    if (settings.measurement === 0) {
      if (includeEnergy) energy = (dy * dy + dx * dx) + energy;
      if (includeGradient) {
        gradient[node * 2] = dx + dx + gradient[node * 2];
        gradient[node * 2 + 1] = dy + dy + gradient[node * 2 + 1];
      }
    } else {
      const distance = Math.sqrt(dy * dy + dx * dx);
      if (includeEnergy) {
        const excess = distance - 0.7, penalty = excess <= 0 ? 0 : excess;
        energy = penalty * penalty + energy;
      }
      if (includeGradient && distance > 0.7) {
        const fraction = 1 - 0.7 / distance, factor = fraction + fraction;
        gradient[node * 2] = dx * factor + gradient[node * 2];
        gradient[node * 2 + 1] = dy * factor + gradient[node * 2 + 1];
      }
    }
  }
  for (let node = 0; node < points.length; node++) {
    if (penaltyCounts[node] === 0) continue;
    const dx = points[node][0] - references[node * 2], dy = points[node][1] - references[node * 2 + 1];
    const scale = penaltyCounts[node] * settings.inversionMeasurement;
    if (includeEnergy) energy = scale * (dy * dy + dx * dx) + energy;
    if (!includeGradient) continue;
    const factor = scale + scale;
    gradient[node * 2] = factor * dx + gradient[node * 2];
    gradient[node * 2 + 1] = factor * dy + gradient[node * 2 + 1];
  }
  if (!includeGradient) return { energy };
  const [left, right, top, bottom] = borderCounts;
  gradient[1] = 0;
  for (let node = 0; node < left + right; node++) gradient[node * 2] = 0;
  gradient[left * 2 - 1] = 0;
  gradient[left * 2 + 1] = 0;
  gradient[(left + right) * 2 - 1] = 0;
  for (let node = left + right; node < left + right + top + bottom; node++) gradient[node * 2 + 1] = 0;
  return includeEnergy ? { energy, gradient } : { gradient };
}

function readLineSearchInput(input, state) {
  const { width, height, nearBorderNodes, direction, epsilon, parameters, phase } = input;
  if (!Number.isInteger(width) || width < 2 || width > 2147483647 ||
      !Number.isInteger(height) || height < 2 || height > 2147483647) throw new Error('Line search needs positive image dimensions of at least two pixels.');
  if (state.imageKernel && (width !== state.imageKernel.width || height !== state.imageKernel.height)) {
    throw new Error('Line-search dimensions must match the prepared image model.');
  }
  if ((!Array.isArray(direction) && !(direction instanceof Float64Array)) || direction.length !== state.points.length * 2 ||
      Array.from(direction).some(value => !Number.isFinite(value))) throw new Error('Line-search direction needs two finite double channels per node.');
  if (!Number.isFinite(epsilon) || epsilon <= 0) throw new Error('Line-search epsilon must be finite and positive.');
  const borderTotal = state.borderCounts.reduce((sum, count) => sum + count, 0);
  if ((!Array.isArray(nearBorderNodes) && !(nearBorderNodes instanceof Int32Array)) ||
      Array.from(nearBorderNodes).some(node => !Number.isInteger(node) || node < borderTotal || node >= state.points.length)) {
    throw new Error('Line search needs the prepared cache of near-border interior node identifiers.');
  }
  const phaseValue = name => parameters[`ContourSmoother::phase_${phase}.${name}`];
  const minimum = phaseValue('cg_min_eps'), maximum = phaseValue('cg_max_eps');
  const maxStep = phaseValue('cg_max_step_size'), fraction = phaseValue('cg_step_fraction');
  const tolerance = phaseValue('cg_quad_line_srch_rel_tol'), maxIterations = phaseValue('cg_quad_line_srch_max_iter');
  const useStep = phaseValue('cg_use_eps_from_step');
  if (!Number.isFinite(minimum) || minimum <= 0 || !Number.isFinite(maximum) || maximum < minimum ||
      !Number.isFinite(maxStep) || maxStep < 0 || !Number.isFinite(fraction) || fraction < 0 ||
      !Number.isFinite(tolerance) || tolerance < 0 || !Number.isInteger(maxIterations) || maxIterations < 1 ||
      maxIterations > 2147483647 || (useStep !== 0 && useStep !== 1)) throw new Error('Invalid quadratic line-search phase controls.');
  return { width, height, nearBorderNodes, direction, epsilon, minimum, maximum, maxStep, fraction, tolerance, maxIterations, useStep };
}

function clampInteriorCoordinate(coordinate, maximum) {
  // Keep the native ordered comparisons, including their NaN branch.
  if (0.01 <= coordinate) return maximum < coordinate ? maximum : coordinate;
  return 0.01;
}

function movePoints(state, controls, step) {
  const { points, borderCounts } = state;
  for (let node = 0; node < points.length; node++) {
    points[node][0] = step * controls.direction[node * 2] + points[node][0];
    points[node][1] = step * controls.direction[node * 2 + 1] + points[node][1];
  }
  // The original model callback runs even for a zero move or after restoring
  // a backup. Its prepared cache stays fixed throughout the line search.
  const [left, right, top, bottom] = borderCounts;
  points[0][1] = 0;
  for (let node = 0; node < left; node++) points[node][0] = 0;
  points[left - 1][1] = controls.height;
  points[left][1] = 0;
  for (let node = left; node < left + right; node++) points[node][0] = controls.width;
  points[left + right - 1][1] = controls.height;
  for (let node = left + right; node < left + right + top; node++) points[node][1] = 0;
  for (let node = left + right + top; node < left + right + top + bottom; node++) points[node][1] = controls.height;
  for (const node of controls.nearBorderNodes) {
    points[node][0] = clampInteriorCoordinate(points[node][0], controls.width - 0.01);
    points[node][1] = clampInteriorCoordinate(points[node][1], controls.height - 0.01);
  }
}

/** Original quadratic line search on an explicit prepared geometric objective. */
export function smoothingLineSearch(input) {
  const state = readObjectiveInput(input), controls = readLineSearchInput(input, state);
  const output = quadraticLine(state, controls, objectiveTerms(state, 'energy').energy);
  return { positions:Float64Array.from(state.points.flat()), ...output };
}

function quadraticLine(state, controls, initialEnergy) {
  const { minimum, maximum, maxStep, fraction, tolerance, maxIterations, useStep } = controls;
  let maxDirection = 0;
  for (const value of controls.direction) if (Math.abs(value) > maxDirection) maxDirection = Math.abs(value);
  let epsilon = controls.epsilon;
  if (maxStep > 0 && maxStep < maxDirection * epsilon) epsilon = maxStep / maxDirection;
  const initialEpsilon = epsilon;
  let energy = initialEnergy, baseEnergy, step = 0, iterations = 0;
  do {
    const backup = state.points.map(point => point.slice());
    baseEnergy = energy;
    movePoints(state, controls, epsilon);
    const firstEnergy = objectiveTerms(state, 'energy').energy;
    movePoints(state, controls, epsilon);
    const secondEnergy = objectiveTerms(state, 'energy').energy;
    const squared = epsilon * epsilon;
    const curvature = ((secondEnergy - (firstEnergy + firstEnergy)) + baseEnergy) / (squared + squared);
    let offset = 0;
    if (!(curvature <= 0)) {
      const product = curvature * epsilon;
      offset = (-1 / (curvature + curvature)) * (product + product + (secondEnergy - baseEnergy) / (epsilon + epsilon));
    }
    if (maxStep > 0) {
      if (maxStep < offset * maxDirection) offset = maxStep / maxDirection;
      if (maxStep < -(offset * maxDirection)) offset = (-1 / maxDirection) * maxStep;
    }
    let candidateEnergy = baseEnergy + 1;
    if (offset !== 0) {
      movePoints(state, controls, offset);
      candidateEnergy = objectiveTerms(state, 'energy').energy;
    }
    let bestEnergy = baseEnergy, bestIndex = 0;
    if (firstEnergy < baseEnergy) { bestEnergy = firstEnergy; bestIndex = 1; }
    if (secondEnergy < bestEnergy) { bestEnergy = secondEnergy; bestIndex = 2; }
    let acceptedStep;
    if (candidateEnergy < bestEnergy) {
      acceptedStep = epsilon + epsilon + offset;
      energy = candidateEnergy;
    } else {
      acceptedStep = bestIndex * epsilon;
      state.points = backup;
      movePoints(state, controls, acceptedStep);
      energy = objectiveTerms(state, 'energy').energy;
    }
    step = acceptedStep + step;
    epsilon *= 0.5;
    if (epsilon < minimum) epsilon = minimum;
    if (maximum < epsilon) epsilon = maximum;
    iterations++;
  } while (baseEnergy !== 0 && tolerance < (baseEnergy - energy) / baseEnergy && iterations < maxIterations);
  if (useStep) {
    epsilon = Math.abs(step) * fraction;
    if (epsilon < minimum) epsilon = minimum;
    if (epsilon > maximum) epsilon = maximum;
  } else epsilon = initialEpsilon;
  return { energy, delta:energy - baseEnergy, epsilon, step, iterations };
}

function cachedLine(state, controls, cache, initialEnergy, previousStep, iteration) {
  cache.position %= cache.seeing + cache.blind;
  let energy = initialEnergy;
  let line = { energy, epsilon:controls.epsilon, step:previousStep, iterations:0 };
  if (cache.position >= cache.seeing) {
    const backup = state.points.map(point => point.slice());
    let offset = cache.average, maxDirection = 0;
    for (const value of controls.direction) if (Math.abs(value) > maxDirection) maxDirection = Math.abs(value);
    if (controls.maxStep > 0) {
      if (controls.maxStep < offset * maxDirection) offset = controls.maxStep / maxDirection;
      if (controls.maxStep < -(offset * maxDirection)) offset = -(controls.maxStep / maxDirection);
    }
    movePoints(state, controls, offset * 0.8);
    energy = objectiveTerms(state, 'energy').energy;
    if (initialEnergy < energy) {
      // Native blind-step rejection restores coordinates without invoking the
      // post-move callback. A seeing move follows in this same iteration.
      state.points = backup;
      energy = initialEnergy;
      cache.position = 0;
    }
    line.energy = energy;
  }
  if (cache.position < cache.seeing) {
    line = quadraticLine(state, controls, energy);
    const retained = (1 - cache.blend) * cache.average;
    cache.average = iteration === 0 ? line.step : line.step * cache.blend + retained;
  }
  cache.position++;
  return line;
}

function readPreparedRepair(input, state) {
  if (input.nearbyEdges === undefined) return undefined;
  const { nearbyEdges, repairInterval } = input;
  const { nodes, regions } = state.geometry;
  if (!Array.isArray(nearbyEdges) || nearbyEdges.length !== nodes.length ||
      !Number.isInteger(repairInterval) || repairInterval < 1 || repairInterval > 2147483647) {
    throw new Error('Contour repair needs fixed nearby edges per node and a positive int32 repair interval.');
  }
  for (const edges of nearbyEdges) {
    if (!Array.isArray(edges)) throw new Error('Nearby contour edges must be [region, sample] pairs.');
    for (const edge of edges) if (!Array.isArray(edge) || edge.length !== 2 ||
        !Number.isInteger(edge[0]) || edge[0] < 0 || edge[0] >= regions.length ||
        !Number.isInteger(edge[1]) || edge[1] < 0 || edge[1] >= regions[edge[0]].contour.length) {
      throw new Error('Nearby contour edges must identify existing region samples.');
    }
  }
  state.imageKernel ??= readImageKernel(input.imageModel, state.geometry);
  state.geometry = { ...state.geometry, nodes:nodes.map(node => ({ ...node })) };
  state.penaltyCounts = Uint8Array.from(state.penaltyCounts);
  return createContourRepair(state, nearbyEdges.map(edges => edges.map(edge => edge.slice())));
}

/** Original prepared Fletcher-Reeves controller, before multi-phase smoothing. */
export function smoothingOptimize(input, onIteration) {
  const state = readObjectiveInput(input);
  const phaseValue = name => input.parameters[`ContourSmoother::phase_${input.phase}.${name}`];
  const optimizerTypes = input.parameters['ContourSmoother::optimizer_type'];
  const lineType = phaseValue('cg_line_search_type');
  if (!Array.isArray(optimizerTypes) || optimizerTypes.length !== 3 || optimizerTypes[input.phase] !== 0 ||
      (lineType !== 0 && lineType !== 5)) throw new Error('Prepared smoothing optimization requires optimizer 0 and line-search type 0 or 5.');
  const direction = new Float64Array(state.points.length * 2);
  const controls = readLineSearchInput({ ...input, direction, epsilon:phaseValue('cg_max_eps') }, state);
  const minIterations = phaseValue('cg_min_iter'), maxIterations = phaseValue('cg_max_iter');
  const restartInterval = phaseValue('cg_iter_for_restart'), knockout = phaseValue('cg_knock_out_count_down');
  const relativeTolerance = phaseValue('cg_rel_tol'), absoluteTolerance = phaseValue('cg_abs_tol');
  if (!Number.isInteger(minIterations) || minIterations < 0 || minIterations > 2147483647 ||
      !Number.isInteger(maxIterations) || maxIterations < 1 || maxIterations > 2147483647 ||
      !Number.isInteger(restartInterval) || restartInterval < 1 || restartInterval > 2147483647 ||
      !Number.isInteger(knockout) || knockout < 0 || knockout > 2147483647 ||
      !Number.isFinite(relativeTolerance) || relativeTolerance < 0 ||
      !Number.isFinite(absoluteTolerance) || absoluteTolerance < 0) throw new Error('Invalid conjugate-gradient phase controls.');
  let cache;
  if (lineType === 5) {
    const seeing = phaseValue('cg_num_seeing_steps'), blind = phaseValue('cg_num_blind_steps');
    const blend = phaseValue('cg_blind_blend_latest');
    if (!Number.isInteger(seeing) || seeing < 1 || seeing > 2147483647 || !Number.isInteger(blind) || blind < 0 ||
        seeing + blind > 2147483647 || !Number.isFinite(blend) || blend < 0 || blend > 1) {
      throw new Error('Invalid seeing/blind line-search controls.');
    }
    cache = { seeing, blind, blend, position:0, average:0 };
  }
  const repair = readPreparedRepair(input, state);
  let { energy, gradient:storedGradient } = objectiveTerms(state);
  let norm = 0;
  for (let channel = 0; channel < direction.length; channel++) {
    direction[channel] = -storedGradient[channel];
    norm = direction[channel] * direction[channel] + norm;
  }
  let baseline = energy, epsilon = controls.epsilon, step = 0, beta = 0;
  let iterations = 0, restarts = 0, lineIterations = 0, countdown = 0;
  while (iterations < maxIterations) {
    if (energy < baseline) baseline = energy;
    controls.epsilon = epsilon;
    const line = cache ? cachedLine(state, controls, cache, energy, step, iterations) : quadraticLine(state, controls, energy);
    energy = line.energy;
    epsilon = line.epsilon;
    step = line.step;
    lineIterations += line.iterations;
    if (repair) {
      const changed = iterations % input.repairInterval === input.repairInterval - 1
        ? repair.all() : repair.invalid(state.imageKernel.takeInvalidRegions());
      if (changed) {
        beta = 0;
        energy = objectiveTerms(state, 'energy').energy;
        baseline = energy;
        for (let channel = 0; channel < direction.length; channel++) direction[channel] = -storedGradient[channel];
        restarts++;
      }
    }
    // Native 0x4939a0 reports after line movement and repair, before the
    // convergence check increments the controller's iteration field.
    onIteration?.(iterations, state);
    const improvement = baseline - energy;
    const relative = baseline <= 0 ? 0 : improvement / baseline;
    if (relative < relativeTolerance || (improvement < absoluteTolerance && improvement >= 0)) {
      countdown++;
      // The native controller checks before refreshing the stored gradient and
      // incrementing its iteration field. That field is not the line-call count.
      if (knockout < countdown && minIterations < iterations) break;
    } else countdown = 0;
    storedGradient = objectiveTerms(state, 'gradient').gradient;
    const previousNorm = norm;
    norm = 0;
    for (const value of storedGradient) norm = value * value + norm;
    beta = Math.abs(previousNorm) >= 1e-30 ? norm / previousNorm : 0;
    let alignment = 0;
    for (let channel = 0; channel < direction.length; channel++) {
      direction[channel] = beta * direction[channel] - storedGradient[channel];
      alignment = alignment - storedGradient[channel] * direction[channel];
    }
    if (alignment <= 0 || iterations % restartInterval === 0) {
      beta = 0;
      for (let channel = 0; channel < direction.length; channel++) direction[channel] = -storedGradient[channel];
      restarts++;
    }
    iterations++;
  }
  // Entry 0x4adf00 evaluates energy once more after the controller returns.
  energy = objectiveTerms(state, 'energy').energy;
  const output = { positions:Float64Array.from(state.points.flat()), energy, baseline, epsilon, step, beta,
    iterations, restarts, lineIterations, countdown, storedGradient, direction };
  if (repair) output.repairState = repair.nodeState();
  return output;
}
