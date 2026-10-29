import { readRegionInput, rebuildConnectedRegions, repairDiagonalMap, paletteFromByteColors } from './region-data.mjs';
import { readPaletteColors } from './pixel-input.mjs';
import { acceptShadowFeatures, reassignShadowPixels, classifyShadowFeatures, measureShadowFeatures } from './shadow-classification.mjs';
import { blurClassifierPixels, measureClassifierGradients } from './image-classification.mjs';
import { collectBlendIndices, dotFloat, eightDirections, fitPreparedBlend,
  twentyFourDirections, writeBlendSample } from './blend-solver.mjs';

const f32 = Math.fround;
const byteScale = f32(1 / 255);
const parameterNames = ['SubPixelSegmentation::self_weight_eps',
  'SubPixelSegmentation::self_weight_penalty_low', 'SubPixelSegmentation::self_weight_penalty_high'];

function readSubpixelControls(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected subpixel region input.');
  const { parameters, neighborhood } = input;
  const directions = readNeighborhood(neighborhood);
  if (parameters === null || typeof parameters !== 'object') throw new Error('Expected numeric subpixel parameters.');
  const settings = parameterNames.map(name => {
    const value = parameters[name];
    if (!Number.isFinite(value) || value < 0 || !Number.isFinite(f32(value))) {
      throw new Error(`${name} must be a finite nonnegative float32 number.`);
    }
    return f32(value);
  });
  return { directions, settings };
}

function readNeighborhood(neighborhood) {
  if (neighborhood !== 'eight' && neighborhood !== 'twenty-four') {
    throw new Error('Subpixel neighborhood must be eight or twenty-four.');
  }
  return neighborhood === 'eight' ? eightDirections : twentyFourDirections;
}

function interiorError(statistic, color = statistic.color) {
  const count = statistic.interiorCount;
  if (count === 0) return 0;
  const reciprocal = 1 / count;
  let error = 0;
  for (let channel = 0; channel < 4; channel++) {
    const mean = statistic.interiorSums[channel] * reciprocal;
    let variance = statistic.interiorSquares[channel] - count * (mean * mean);
    // The native routine clamps BGR cancellation, but retains alpha residuals.
    if (channel < 3) variance = Math.max(variance, 0);
    const difference = color[channel] - mean;
    error += (difference * difference) * count + variance;
  }
  return error;
}

function refreshRegionError(statistic, costs) {
  statistic.interiorError = interiorError(statistic);
  let boundaryError = 0;
  for (const pixel of statistic.boundary) boundaryError += costs[pixel];
  statistic.error = statistic.interiorError + boundaryError;
  return statistic.error;
}

function refreshErrors({ regions, costs }) {
  for (const statistic of regions) refreshRegionError(statistic, costs);
}

// Reuse scratch storage across evaluations. Counterfactual labels affect both
// the center color and ordered neighbors, without changing the actual map.
// Return double energy; each consumer applies its native storage rounding.
function createPixelCost({ width, height, bgra }, { indices, regions }, directions, settings) {
  const sample = new Float64Array(4);
  const differences = new Float32Array(4);
  const [epsilon, lowPenalty, highPenalty] = settings;
  return (pixel, source = -1, target = -1) => {
    writeBlendSample(bgra, pixel * 4, sample);
    const selected = collectBlendIndices(indices, width, height, pixel, directions, source, target);
    const colors = new Float32Array(selected.length * 4);
    for (const [entry, neighbor] of selected.entries()) colors.set(regions[neighbor].color, entry * 4);
    const fitted = fitPreparedBlend(sample, colors);
    for (let channel = 0; channel < 4; channel++) differences[channel] = f32(f32(sample[channel]) - fitted.color[channel]);
    let penalty = Math.max(1 - fitted.selfWeight, 0) * lowPenalty;
    penalty += Math.max(epsilon - fitted.selfWeight, 0) * highPenalty;
    return penalty + dotFloat(differences, differences);
  };
}

function buildSubpixelState(raster, directions, settings, cachedCosts = null, progress) {
  const { width, height, bgra } = raster;
  const publish = (offset,counter,indices) => progress?.sink(4,progress.substep+offset,counter,{indices});
  publish(0,0,raster.regions);
  const { indices, statistics: regions } = rebuildConnectedRegions(raster);
  publish(0,1,indices);
  publish(0,2,indices);
  for (const statistic of regions) {
    statistic.squares = new Float32Array(4);
    statistic.interiorCount = 0;
    statistic.interiorSums = new Float32Array(4);
    statistic.interiorSquares = new Float32Array(4);
    statistic.color = new Float32Array(4);
    statistic.boundary = new Set();
  }
  const interiorBytes = regions.map(() => [[], [], [], []]);
  publish(0,3,indices);
  for (let pixel = 0; pixel < indices.length; pixel++) {
    if (progress && (pixel&8191)===0) publish(1,Math.trunc(((pixel*50)|0)/indices.length+0.5)|0,indices);
    const region = indices[pixel];
    const statistic = regions[region];
    const x = pixel % width, y = Math.floor(pixel / width);
    let boundary = false;
    for (const [dx, dy] of directions) {
      const column = x + dx, row = y + dy;
      if (column < 0 || column >= width || row < 0 || row >= height) continue;
      const neighbor = row * width + column;
      if (indices[neighbor] !== region) {
        statistic.boundary.add(neighbor);
        boundary = true;
      }
    }
    if (boundary) statistic.boundary.add(pixel);
    else statistic.interiorCount++;
    for (let channel = 0; channel < 4; channel++) {
      const byte = bgra[pixel * 4 + channel];
      const value = f32(byte * byteScale);
      const square = f32(value * value);
      statistic.squares[channel] += square;
      if (!boundary) {
        statistic.interiorSums[channel] += value;
        statistic.interiorSquares[channel] += square;
        interiorBytes[region][channel].push(byte);
      }
    }
  }
  publish(2,0,indices);
  for (const [region, statistic] of regions.entries()) {
    if (statistic.interiorCount < 3) {
      const reciprocal = f32(1 / f32(statistic.count));
      for (let channel = 0; channel < 4; channel++) statistic.color[channel] = f32(statistic.sums[channel] * reciprocal);
    } else {
      for (let channel = 0; channel < 4; channel++) {
        const bytes = interiorBytes[region][channel].sort((a, b) => a - b);
        statistic.color[channel] = f32(bytes[Math.floor(bytes.length / 2)] * byteScale);
      }
    }
    statistic.boundary = Int32Array.from([...statistic.boundary].sort((a, b) => a - b));
  }
  publish(2,1,indices);
  // Initialization clears the grid; later native rebuilds retain interior costs.
  const costs = cachedCosts ?? new Float32Array(indices.length);
  const pixelCost = createPixelCost(raster, { indices, regions }, directions, settings);
  for (const [region, statistic] of regions.entries()) {
    for (const pixel of statistic.boundary) {
      if (indices[pixel] !== region) continue;
      costs[pixel] = pixelCost(pixel);
    }
  }
  const state = { indices, costs, regions };
  refreshErrors(state);
  publish(2,2,indices);
  return state;
}

/** Rebuild the original anti-alias statistics from explicit prepared regions. */
export function buildSubpixelStatistics(input) {
  const { directions, settings } = readSubpixelControls(input);
  return buildSubpixelState(readRegionInput(input), directions, settings);
}

class SubpixelModel {
  #state;
  #pixelCost;
  #raster;
  #directions;
  #settings;
  #onProgress;

  constructor(input,onProgress) {
    const { directions, settings } = readSubpixelControls(input);
    const { width, height, bgra, regions } = readRegionInput(input);
    // Persistent evaluations own pixels; the synchronous rebuild API can borrow.
    this.#raster = { width, height, bgra: Uint8Array.from(bgra) };
    this.#directions = directions;
    this.#settings = settings;
    this.#onProgress = onProgress;
    this.#state = buildSubpixelState({ ...this.#raster, regions }, directions, settings,undefined,
      onProgress ? {sink:onProgress,substep:0} : undefined);
    this.#pixelCost = createPixelCost(this.#raster, this.#state, directions, settings);
  }

  snapshot() {
    refreshErrors(this.#state);
    return structuredClone(this.#state);
  }

  get regionCount() {return this.#state.regions.length;}

  publish(substep,counter = 0) {
    this.#onProgress?.(4,substep,counter,{indices:this.#state.indices});
  }

  rebuild(controls,substep) {
    if (controls === null || typeof controls !== 'object') throw new Error('Expected subpixel rebuild controls.');
    const directions = readNeighborhood(controls.neighborhood);
    this.#directions = directions;
    this.#state = buildSubpixelState({ ...this.#raster, regions: this.#state.indices },
      directions, this.#settings, this.#state.costs,
      this.#onProgress && substep!==undefined ? {sink:this.#onProgress,substep} : undefined);
    this.#pixelCost = createPixelCost(this.#raster, this.#state, directions, this.#settings);
  }

  /** Change only labels, matching the native stage before its next rebuild. */
  repairDiagonalContacts() {
    repairDiagonalMap(this.#raster, this.#state.indices);
  }

  /** Fixed-palette projection copies colors without rebuilding the pixel cache. */
  projectPalette(controls) {
    if (controls === null || typeof controls !== 'object' || !Number.isInteger(controls.minimumInteriorPixels) ||
        controls.minimumInteriorPixels < 0 || controls.minimumInteriorPixels > 2147483647) {
      throw new Error('Palette projection needs a nonnegative int32 minimumInteriorPixels.');
    }
    const palette = readPaletteColors(controls.palette);
    for (const statistic of this.#state.regions) {
      if (statistic.colorIndex >= 0 || statistic.interiorCount < controls.minimumInteriorPixels || statistic.count === 0) continue;
      let best = 0, bestCost = Infinity;
      for (let color = 0; color < palette.length / 4; color++) {
        let cost = 0;
        for (let channel = 0; channel < 4; channel++) {
          const difference = f32(statistic.color[channel] - palette[color * 4 + channel]);
          // Original x87 control word 0x027f selects double-precision products
          // and sums after SSE channel subtraction.
          // Reusing the float32 pixel-assignment distance changes close winners.
          cost += difference * difference;
        }
        if (cost < bestCost) { best = color; bestCost = cost; }
      }
      statistic.color.set(palette.subarray(best * 4, best * 4 + 4));
      statistic.colorIndex = best;
    }
  }

  /** Internal complete shadow stage. First color selection/projection occurs
   * in the native dispatcher; this method owns transfer, preparation and the
   * state/cache transition before the dispatcher's second color selection. */
  runShadowStage(source, parameters, palette) {
    let indices = this.#state.indices;
    let transferredPalette;
    if (parameters.clusterColors || palette !== undefined) {
      const transferred = transferSubpixelPalette(this.#state, palette);
      indices = rebuildConnectedRegions({...this.#raster,regions:transferred.indices}).indices;
      transferredPalette = transferred.palette;
    }
    const sink = this.#onProgress && ((phase,substep,counter,updated) =>
      this.#onProgress(phase,substep,counter,{...updated,...(transferredPalette ? {palette:transferredPalette} : {})}));
    // Native feature preparation clears color IDs and rebuilds eight-neighbor
    // statistics, while retaining cached costs at now-interior pixels.
    this.#directions = eightDirections;
    this.#state = buildSubpixelState({ ...this.#raster, regions:indices },
      this.#directions, this.#settings, this.#state.costs,sink ? {sink,substep:27} : undefined);
    this.publish(30,0);
    const blurredBgra = blurClassifierPixels(source);
    this.publish(30,1);
    const gradients = measureClassifierGradients(this.#raster, { ...source, bgra:blurredBgra });
    this.publish(30,2);
    // Zero gives every observed feature/direction/flag byte from the original
    // uninitialized first scalar. Explicit retained-state kernels remain tested
    // separately; this does not assert that the native stack was initialized.
    const { features } = measureShadowFeatures(this.#raster, this.#state, gradients, 0);
    this.publish(30,3);
    const shadowFlags = Uint8Array.from(features, values => Number(classifyShadowFeatures(values)));
    this.publish(30,4);
    this.publish(31,0);
    this.publish(31,1);
    this.#state = refineShadowState({ ...this.#raster, features, shadowFlags, parameters,
      control:parameters.shadowControl }, this.#state,this.#onProgress).state;
    this.#pixelCost = createPixelCost(this.#raster, this.#state, this.#directions, this.#settings);
  }

  #readPair(proposal) {
    if (proposal === null || typeof proposal !== 'object') throw new Error('Expected a subpixel merge proposal.');
    const { source, target } = proposal;
    const { regions } = this.#state;
    if (!Number.isInteger(source) || !Number.isInteger(target) || source < 0 || target < 0 ||
        source >= regions.length || target >= regions.length || regions[source].count === 0 || regions[target].count === 0) {
      throw new Error('Merge source and target must be active region identifiers in the model.');
    }
    return { source, target };
  }

  mergeCost(proposal) {
    const { source, target } = this.#readPair(proposal);
    const { lambda } = proposal;
    if (!Number.isFinite(lambda) || lambda < 0 || !Number.isFinite(f32(lambda))) {
      throw new Error('Merge lambda must be a finite nonnegative float32 number.');
    }
    return this.#cost(source, target, f32(lambda));
  }

  #cost(source, target, lambda) {
    const { regions, costs } = this.#state;
    const statistic = regions[source];
    const lowerBound = interiorError(statistic, regions[target].color) - refreshRegionError(statistic, costs);
    if (lowerBound > lambda) return lowerBound;
    let boundaryError = 0;
    for (const pixel of statistic.boundary) boundaryError += f32(this.#pixelCost(pixel, source, target));
    return lowerBound + boundaryError;
  }

  boundaryCost(proposal) {
    const { source, target } = this.#readPair(proposal);
    const { budget } = proposal;
    if (!Number.isFinite(budget) || budget < 0) throw new Error('Boundary budget must be a finite nonnegative number.');
    return this.#boundedBoundaryCost(source, target, budget);
  }

  #boundedBoundaryCost(source, target, budget) {
    let error = 0;
    for (const pixel of this.#state.regions[source].boundary) {
      // Unlike mergeCost, this native path retains the unrounded pixel energy.
      error += this.#pixelCost(pixel, source, target);
      if (error > budget) break;
    }
    return error;
  }

  selectColors(controls,substep) {
    if (controls === null || typeof controls !== 'object' || !Number.isFinite(controls.margin) ||
        controls.margin < 0 || !Number.isFinite(f32(controls.margin))) {
      throw new Error('Color selection needs a finite nonnegative float32 margin.');
    }
    const margin = f32(controls.margin);
    const { regions, costs } = this.#state;
    const candidates = [];
    for (const [region, statistic] of regions.entries()) {
      if (statistic.interiorCount >= 3) candidates.push(region);
    }
    candidates.sort((a, b) => regions[b].interiorCount - regions[a].interiorCount || b - a);
    candidates.length = Math.min(candidates.length, 100);
    if (substep!==undefined) this.publish(substep);
    for (const [source, statistic] of regions.entries()) {
      if (substep!==undefined && (source&3)===0)
        this.publish(substep,Math.trunc(((source*50)|0)/(regions.length-1)+0.5)|0);
      if (statistic.interiorCount >= 3) continue;
      let bestCost = refreshRegionError(statistic, costs) - margin;
      let selected = source;
      for (const target of candidates) {
        const interior = interiorError(statistic, regions[target].color);
        if (!(interior < bestCost)) continue;
        const error = interior + this.#boundedBoundaryCost(source, target, bestCost - interior);
        if (error < bestCost) {
          bestCost = error;
          selected = target;
        }
      }
      if (selected !== source) {
        // Keep cached pixels unchanged; later source fits see these new colors.
        statistic.color.set(regions[selected].color);
        statistic.colorIndex = regions[selected].colorIndex;
      }
    }
  }

  merge(proposal) {
    const { source, target } = this.#readPair(proposal);
    if (source === target) throw new Error('An applied merge needs distinct source and target regions.');
    this.#merge(source, target);
  }

  #merge(source, target) {
    const { indices, costs, regions } = this.#state;
    const from = regions[source], into = regions[target];
    const { width, height, bgra } = this.#raster;
    for (let pixel = 0; pixel < indices.length; pixel++) if (indices[pixel] === source) indices[pixel] = target;
    into.count += from.count;
    into.interiorCount += from.interiorCount;
    for (const field of ['sums', 'squares', 'interiorSums', 'interiorSquares']) {
      for (let channel = 0; channel < 4; channel++) into[field][channel] += from[field][channel];
    }
    from.count = 0;
    from.sums.fill(0);
    from.interiorCount = 0;
    from.interiorSums.fill(0);
    from.interiorSquares.fill(0);
    // Source squared sums and both model colors survive the native merge.
    for (const pixel of from.boundary) costs[pixel] = this.#pixelCost(pixel);
    const boundary = new Set(into.boundary);
    for (const pixel of from.boundary) {
      if (!boundary.has(pixel)) {
        boundary.add(pixel);
        continue;
      }
      if (indices[pixel] !== target) continue;
      const x = pixel % width, y = Math.floor(pixel / width);
      let interior = true;
      for (const [dx, dy] of this.#directions) {
        const column = x + dx, row = y + dy;
        if (column >= 0 && column < width && row >= 0 && row < height && indices[row * width + column] !== target) {
          interior = false;
          break;
        }
      }
      if (!interior) continue;
      into.interiorCount++;
      for (let channel = 0; channel < 4; channel++) {
        const value = f32(bgra[pixel * 4 + channel] * byteScale);
        into.interiorSums[channel] += value;
        into.interiorSquares[channel] += f32(value * value);
      }
      boundary.delete(pixel);
    }
    into.boundary = Int32Array.from([...boundary].sort((a, b) => a - b));
    from.boundary = new Int32Array(0);
    // Error fields are projections: proposal reads refresh their source,
    // and snapshots refresh all records after shared cached pixels change.
  }

  iterate(controls) {
    if (controls === null || typeof controls !== 'object') throw new Error('Expected subpixel pass controls.');
    const { lambda, maximumSourcePixels, maximumTargetPixels, sourceCount } = controls;
    if (!Number.isFinite(lambda) || lambda < 0 || !Number.isFinite(f32(lambda))) {
      throw new Error('Pass lambda must be a finite nonnegative float32 number.');
    }
    if (!Number.isInteger(maximumSourcePixels) || !Number.isInteger(maximumTargetPixels) ||
        maximumSourcePixels < 0 || maximumTargetPixels < 0 ||
        maximumSourcePixels > 2147483647 || maximumTargetPixels > 2147483647) {
      throw new Error('Pass source and target size limits must be nonnegative int32 values.');
    }
    if (sourceCount !== 'all' && sourceCount !== 'interior') throw new Error('Pass sourceCount must be all or interior.');
    const threshold = f32(lambda);
    const { indices, regions } = this.#state;
    const examined = new Set();
    let merges = 0;
    for (const [source, statistic] of regions.entries()) {
      const count = sourceCount === 'all' ? statistic.count : statistic.interiorCount;
      if (statistic.count === 0 || count > maximumSourcePixels) continue;
      let bestTarget = -1, bestCost = threshold;
      for (const pixel of statistic.boundary) {
        const target = indices[pixel];
        // Preserve the original uint32 packing, including large-ID collisions.
        const pair = ((source << 16) | target) >>> 0;
        if (target === source || examined.has(pair) || regions[target].count > maximumTargetPixels) continue;
        const cost = this.#cost(source, target, threshold);
        if (cost < bestCost) {
          bestCost = cost;
          bestTarget = target;
        }
        examined.add(pair);
      }
      if (bestTarget >= 0) {
        this.#merge(source, bestTarget);
        merges++;
      }
    }
    return merges;
  }
}

/** Own prepared regions, evaluate directed costs, and apply explicit merges. */
export function createSubpixelModel(input,onProgress) {
  return new SubpixelModel(input,onProgress);
}

/** Run the shared subpixel optimizer before dispatcher color/shadow stages. */
function optimizeSubpixelModel(input,onProgress) {
  if (input === null || typeof input !== 'object' || input.parameters === null || typeof input.parameters !== 'object') {
    throw new Error('Expected subpixel optimization input and numeric parameters.');
  }
  const { parameters } = input;
  const lambda = parameters['SubPixelSegmenter::lambda_f'];
  const wide = parameters['SubPixelSegmenter::use_connected_24_iteration'];
  if (parameters.clusterColors !== undefined && typeof parameters.clusterColors !== 'boolean') {
    throw new Error('clusterColors must be a boolean when supplied.');
  }
  if (!Number.isFinite(lambda) || lambda < 0 || !Number.isFinite(f32(lambda))) {
    throw new Error('SubPixelSegmenter::lambda_f must be a finite nonnegative float32 number.');
  }
  if (wide !== 0 && wide !== 1) throw new Error('SubPixelSegmenter::use_connected_24_iteration must be 0 or 1.');
  onProgress?.(4,-1,0,{indices:input.regions});
  const model = createSubpixelModel({ ...input, neighborhood: 'eight' },onProgress);
  const maximum = 2147483647;
  const limit = 20;
  const iterate = (source, target, sourceCount = 'all') => model.iterate({ lambda,
    maximumSourcePixels: source, maximumTargetPixels: target, sourceCount });
  const publish = (substep,iteration = 0) => model.publish(substep,Math.trunc(((iteration*50)|0)/limit+0.5)|0);
  publish(3);
  let changes = iterate(1, 1);
  for (let iteration = 0; changes !== 0 && iteration < limit; iteration++) {publish(3,iteration);changes = iterate(iteration + 2, iteration + 2);}
  model.rebuild({ neighborhood: 'eight' },4);
  publish(7);
  changes = iterate(1, maximum);
  for (let iteration = 0; changes !== 0 && iteration < limit; iteration++) {publish(7,iteration);changes = iterate(iteration + 2, maximum);}
  publish(8);
  changes = iterate(maximum, maximum);
  for (let iteration = 0; changes !== 0 && iteration < limit; iteration++) {publish(8,iteration);changes = iterate(maximum, maximum);}
  if (wide) {
    model.rebuild({ neighborhood: 'twenty-four' },9);
    publish(12);
    changes = iterate(limit, maximum, 'interior');
    for (let iteration = 0; changes !== 0 && iteration < limit; iteration++) {publish(12,iteration);changes = iterate(limit, maximum, 'interior');}
    model.rebuild({ neighborhood: 'eight' },13);
  }
  // Native thin cleanup rejects every valid zero-interior region when it
  // encounters an outside boundary pixel, so it makes no label/cache writes.
  const regionCount = model.regionCount;
  for (let region = 0; region<regionCount; region+=4) model.publish(16,Math.trunc(((region*50)|0)/(regionCount-1)+0.5)|0);
  model.repairDiagonalContacts();
  model.publish(17);
  model.rebuild({ neighborhood: 'eight' },18);
  publish(21);
  changes = iterate(4, maximum);
  for (let iteration = 0; changes !== 0 && iteration < limit; iteration++) {publish(21,iteration);changes = iterate(4, maximum);}
  model.rebuild({ neighborhood: 'eight' },22);
  model.publish(25);
  return model;
}

function selectSubpixelColors(model, input) {
  if (input.parameters.clusterColors || input.palette !== undefined) {
    model.selectColors({ margin: input.parameters['SubPixelSegmenter::color_cluster_margin'] },46);
  }
  if (input.palette !== undefined) model.projectPalette({ palette: input.palette, minimumInteriorPixels:0 });
}

/** Complete original subpixel optimization and its requested color selection.
 * This component excludes the dispatcher's separate shadow stage. */
export function optimizeSubpixel(input) {
  const model = optimizeSubpixelModel(input);
  selectSubpixelColors(model, input);
  return model.snapshot();
}

/** Internal canonical anti-alias dispatcher, including explicit shadow state. */
export function segmentSubpixelState(input, source, onProgress) {
  const { parameters } = input;
  if (parameters.shadowing !== undefined && typeof parameters.shadowing !== 'boolean') {
    throw new Error('shadowing must be a boolean when supplied.');
  }
  if (parameters.shadowing) {
    if (!Number.isInteger(parameters.shadowControl) || parameters.shadowControl < -2147483648 || parameters.shadowControl > 2147483647) {
      throw new Error('Enabled shadowing requires an explicit int32 shadowControl.');
    }
    if (input.width < 2 || input.height < 2) throw new Error('Shadowing requires a prepared image of at least 2 by 2.');
  }
  const model = optimizeSubpixelModel(input,onProgress);
  if (parameters.shadowing) {
    if (input.palette !== undefined) model.projectPalette({ palette:input.palette, minimumInteriorPixels:0 });
    else if (parameters.clusterColors) {
      model.selectColors({ margin:parameters['SubPixelSegmenter::color_cluster_margin'] },26);
    }
    model.runShadowStage(source, parameters, input.palette);
  }
  selectSubpixelColors(model, input);
  return model.snapshot();
}

/** Internal native palette transfer shared by first and final dispatch stages. */
export function transferSubpixelPalette(state, suppliedPalette) {
  function* regionBytes() {
    for (const statistic of state.regions) {
      yield Array.from(statistic.color, channel => Math.trunc(channel * 255 + 0.5) & 255);
    }
  }
  const { palette, colorIndices } = suppliedPalette === undefined ? paletteFromByteColors(regionBytes()) :
    { palette:suppliedPalette, colorIndices:Int32Array.from(state.regions, statistic => statistic.colorIndex) };
  return { palette, indices:Int32Array.from(state.indices, region => colorIndices[region]) };
}

/** Internal shadow refinement over already prepared eight-neighbor state.
 * The caller supplies recovered feature rows/flags and native int32 control.
 * Work on owned caches and labels so earlier stage snapshots remain reusable. */
export function refineShadowState(raster, prepared,onProgress) {
  const { features, shadowFlags, parameters, control } = raster;
  const settings = parameterNames.map(name => f32(parameters[name]));
  const measured = features.map(values => Float64Array.from(values));
  const accepted = new Uint8Array(shadowFlags.length);
  const shadowRegions = [];
  for (let region = 0; region < shadowFlags.length; region++) {
    if (shadowFlags[region] & 1) shadowRegions.push(region);
  }
  if (shadowRegions.length === 0) {
    return { state:structuredClone(prepared), features:measured, accepted, assignments:[] };
  }
  const before = buildSubpixelState({ ...raster, regions:prepared.indices }, twentyFourDirections,
    settings, Float32Array.from(prepared.costs),onProgress ? {sink:onProgress,substep:32} : undefined);
  onProgress?.(4,35,0,{indices:before.indices});
  const oldEnergies = [],pixelsByShadow = [],outsideByShadow = [];
  for (const [ordinal,region] of shadowRegions.entries()) {
    const statistic = before.regions[region],pixels = [],outside = [];
    for (let pixel=0;pixel<before.indices.length;pixel++) if (before.indices[pixel]===region) pixels.push(pixel);
    for (const pixel of statistic.boundary) if (before.indices[pixel]!==region) outside.push(pixel);
    pixelsByShadow.push(pixels);outsideByShadow.push(outside);
    // Native old energies precede the fill. Preserve the ordinal getter and
    // float32 summation order while retaining this value for later acceptance.
    let oldEnergy = f32(before.regions[ordinal].interiorError);
    for (const pixel of statistic.boundary) oldEnergy = f32(oldEnergy+before.costs[pixel]);
    oldEnergies.push(oldEnergy);
  }
  onProgress?.(4,35,1,{indices:before.indices});
  onProgress?.(4,35,2,{indices:before.indices});
  const fill = reassignShadowPixels(raster, before.indices, shadowFlags);
  onProgress?.(4,35,3,{indices:fill.indices});
  onProgress?.(4,35,4,{indices:fill.indices});
  // The native default rebuild clears its cost grid and restores eight-neighbor
  // statistics before the wider cost evaluation. Keep that ordering explicit.
  const eight = buildSubpixelState({ ...raster, regions:fill.indices }, eightDirections, settings,undefined,
    onProgress ? {sink:onProgress,substep:36} : undefined);
  const after = buildSubpixelState({ ...raster, regions:eight.indices }, twentyFourDirections, settings, eight.costs,
    onProgress ? {sink:onProgress,substep:39} : undefined);
  onProgress?.(4,42,0,{indices:after.indices});
  const pixelCost = createPixelCost(raster, after, twentyFourDirections, settings);
  for (const [ordinal, region] of shadowRegions.entries()) {
    const pixels = pixelsByShadow[ordinal],outside = outsideByShadow[ordinal],oldEnergy = oldEnergies[ordinal];
    let newEnergy = 0;
    for (const pixel of pixels) newEnergy = f32(newEnergy + pixelCost(pixel));
    for (const pixel of outside) newEnergy = f32(newEnergy + pixelCost(pixel));
    const count = f32(pixels.length + outside.length), reciprocal = f32(1 / count);
    const values = measured[region], difference = newEnergy - oldEnergy;
    values[33] = oldEnergy; values[34] = newEnergy;
    values[35] = reciprocal * oldEnergy; values[36] = reciprocal * newEnergy;
    values[37] = difference; values[38] = difference / count;
  }
  // Native 0x4aa613 restores the saved filled map with original region IDs.
  // Dense IDs from the cost-fitting rebuild would make rejected pixels merge
  // into unrelated regions. All energies also precede every acceptance write.
  const indices = Int32Array.from(fill.indices);
  onProgress?.(4,42,1,{indices});
  for (const [ordinal, region] of shadowRegions.entries()) {
    if (acceptShadowFeatures(measured[region], control)) accepted[region] = 1;
    else for (const pixel of pixelsByShadow[ordinal]) indices[pixel] = region;
  }
  onProgress?.(4,42,2,{indices});
  const connected = rebuildConnectedRegions({...raster,regions:indices}).indices;
  const state = buildSubpixelState({ ...raster, regions:connected }, eightDirections, settings,undefined,
    onProgress ? {sink:onProgress,substep:43} : undefined);
  return { state, features:measured, accepted, assignments:fill.assignments, before, after };
}
