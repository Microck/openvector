import { readRasterInput, readRegionInput, rebuildConnectedRegions, repairDiagonalMap, paletteFromByteColors } from './region-data.mjs';
import { segmentSubpixelState, transferSubpixelPalette } from './subpixel.mjs';
import { readPaletteColors } from './pixel-input.mjs';
import { assignPalette } from './palette.mjs';

const f32 = Math.fround;
const byteScale = 1 / 255;
const floatByteScale = f32(byteScale);
const int32Max = 2147483647;

function squaredNorm(blue, green, red, alpha) {
  let squared = f32(f32(blue * blue) + f32(green * green));
  squared = f32(squared + f32(red * red));
  return f32(squared + f32(alpha * alpha));
}

function colorMergeCost(firstCount, first, secondCount, second) {
  const firstNorm = squaredNorm(first[0], first[1], first[2], first[3]);
  const secondNorm = squaredNorm(second[0], second[1], second[2], second[3]);
  const mergedNorm = squaredNorm(f32(first[0] + second[0]), f32(first[1] + second[1]),
    f32(first[2] + second[2]), f32(first[3] + second[3]));
  const separate = f32(f32(firstNorm / f32(firstCount)) + f32(secondNorm / f32(secondCount)));
  return f32(separate - f32(mergedNorm / f32(firstCount + secondCount)));
}

/** The color term of the original region merge decision, preserving SSE order. */
export function regionColorMergeCost(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected region counts and BGRA sums.');
  const { counts, sums } = input;
  if (!Array.isArray(counts) || counts.length !== 2 ||
      !Number.isInteger(counts[0]) || counts[0] < 1 || !Number.isInteger(counts[1]) || counts[1] < 1 ||
      counts[0] + counts[1] > int32Max) {
    throw new Error('Merge cost needs two positive int32 counts whose sum fits int32.');
  }
  if (!Array.isArray(sums) || sums.length !== 2) throw new Error('Merge cost needs two BGRA sum vectors.');
  const normalized = [];
  for (const channels of sums) {
    if ((!Array.isArray(channels) && !(channels instanceof Float32Array)) || channels.length !== 4) {
      throw new Error('Each region sum vector must contain four BGRA channels.');
    }
    for (const channel of channels) {
      if (!Number.isFinite(channel) || !Number.isFinite(f32(channel))) {
        throw new Error('Region sums must be finite float32 numbers.');
      }
    }
    normalized.push(Float32Array.from(channels));
  }
  return colorMergeCost(counts[0], normalized[0], counts[1], normalized[1]);
}

function readNonnegativeFloat(value, name) {
  if (!Number.isFinite(value) || value < 0 || !Number.isFinite(f32(value))) {
    throw new Error(`${name} must be a finite nonnegative float32 number.`);
  }
  return f32(value);
}

/** Reproduce the original recursive tiles before connected-region rebuilding. */
export function initializeRegions(input) {
  const { width, height, bgra } = readRasterInput(input);
  const threshold = readNonnegativeFloat(input.threshold, 'Region threshold');
  return initializeTileRegions(width, height, bgra, threshold);
}

function initializeTileRegions(width, height, bgra, threshold, onProgress) {
  const labels = new Int32Array(width * height);
  for (let pixel = 0; pixel < labels.length; pixel++) labels[pixel] = pixel;
  let nextLabel = labels.length;

  const paint = (x, y, size) => {
    const right = Math.min(width, x + size);
    const bottom = Math.min(height, y + size);
    for (let row = y; row < bottom; row++) {
      for (let column = x; column < right; column++) labels[row * width + column] = nextLabel;
    }
    // Even an off-image child consumes a label in the original paint routine.
    nextLabel++;
  };

  const visit = (x, y, size, topLevel) => {
    const sums = [0, 0, 0, 0];
    const squares = [0, 0, 0, 0];
    if (x >= width || y >= height) return { sums, squares, count: 0, cost: 0 };
    if (size === 1) {
      const offset = (y * width + x) * 4;
      for (let channel = 0; channel < 4; channel++) {
        // Initial tiling uses double normalization, unlike later float32 stats.
        sums[channel] = bgra[offset + channel] * byteScale;
        squares[channel] = sums[channel] * sums[channel];
      }
      return { sums, squares, count: 1, cost: 0 };
    }
    const half = size / 2;
    const quadrants = [[x, y], [x + half, y], [x, y + half], [x + half, y + half]];
    const children = [];
    let count = 0;
    for (const [column, row] of quadrants) {
      const child = visit(column, row, half, false);
      children.push(child);
      count += child.count;
      for (let channel = 0; channel < 4; channel++) {
        sums[channel] += child.sums[channel];
        squares[channel] += child.squares[channel];
      }
    }
    let squaredSum = 0;
    let norm = 0;
    for (let channel = 0; channel < 4; channel++) {
      squaredSum += squares[channel];
      norm += sums[channel] * sums[channel];
    }
    const cost = (squaredSum - norm / count) / count;
    if (cost > threshold) {
      for (let child = 0; child < children.length; child++) {
        if (children[child].cost <= threshold) paint(quadrants[child][0], quadrants[child][1], half);
      }
    } else if (topLevel) {
      paint(x, y, size);
    }
    return { sums, squares, count, cost };
  };

  for (let y = 0; y < height; y += 16) {
    onProgress?.(3,0,Math.trunc(((y*50)|0)/height+0.5)|0,{indices:labels});
    for (let x = 0; x < width; x += 16) visit(x, y, 16, true);
  }
  return labels;
}

/** Validate and copy caller storage once before constructing optimizer state. */
export function createRegionOptimizer(input) {
  const { width, height, bgra, regions } = readRegionInput(input);
  const { minimumPixels } = input;
  if (!Number.isInteger(minimumPixels) || minimumPixels < 0 || minimumPixels > int32Max) {
    throw new Error('minimumPixels must be a nonnegative int32 value.');
  }
  const finalLambda = readNonnegativeFloat(input.finalLambda, 'finalLambda');
  return new RegionOptimizerState({ width, height, bgra: Uint8Array.from(bgra),
    regions: Int32Array.from(regions), finalLambda, minimumPixels });
}

/** Internal constructor takes validated, owned storage from either boundary. */
class RegionOptimizerState {
  #width;
  #height;
  #bgra;
  #indices;
  #parents;
  #statistics;
  #finalLambda;
  #minimumPixels;
  #palette = null;

  constructor({ width, height, bgra, regions, finalLambda, minimumPixels, palette = null }) {
    this.#finalLambda = finalLambda;
    this.#minimumPixels = minimumPixels;
    this.#width = width;
    this.#height = height;
    this.#bgra = bgra;
    this.#indices = regions;
    this.#palette = palette;
    if (palette !== null) {
      this.#statistics = Array.from({ length: palette.length / 4 }, (_, colorIndex) => ({ colorIndex }));
    }
    this.#rebuildConnected();
  }

  #rebuildConnected() {
    const { indices, statistics } = rebuildConnectedRegions({ width: this.#width,
      bgra: this.#bgra, regions: this.#indices }, this.#palette === null ? null : this.#statistics);
    this.#indices = indices;
    this.#statistics = statistics;
    this.#parents = new Int32Array(statistics.length).fill(-1);
  }

  #root(pixel) {
    const original = this.#indices[pixel];
    let root = original;
    while (this.#parents[root] >= 0) root = this.#parents[root];
    if (root !== original) {
      this.#parents[original] = root;
      this.#indices[pixel] = root;
    }
    return root;
  }

  #merge(first, second) {
    this.#parents[second] = first;
    const target = this.#statistics[first];
    const source = this.#statistics[second];
    target.count += source.count;
    source.count = 0;
    for (let channel = 0; channel < 4; channel++) {
      target.sums[channel] += source.sums[channel];
      source.sums[channel] = 0;
    }
  }

  #move(pixel, first, second) {
    this.#indices[pixel] = second;
    const source = this.#statistics[first];
    const target = this.#statistics[second];
    source.count--;
    target.count++;
    for (let channel = 0; channel < 4; channel++) {
      const value = f32(this.#bgra[pixel * 4 + channel] * floatByteScale);
      target.sums[channel] += value;
      source.sums[channel] -= value;
    }
  }

  #pixelCost(pixel, region) {
    const statistic = this.#statistics[region];
    // Native decisions scale the mean to byte space before subtracting bytes.
    // Normalizing the pixel first would produce different rounded decisions.
    const scale = f32(255 / f32(statistic.count));
    const offset = pixel * 4;
    return squaredNorm(f32(this.#bgra[offset] - f32(statistic.sums[0] * scale)),
      f32(this.#bgra[offset + 1] - f32(statistic.sums[1] * scale)),
      f32(this.#bgra[offset + 2] - f32(statistic.sums[2] * scale)),
      f32(this.#bgra[offset + 3] - f32(statistic.sums[3] * scale)));
  }

  #compare(pixel, neighbor, first, second, lambda, minimumPixels) {
    const a = this.#statistics[first];
    const b = this.#statistics[second];
    if (colorMergeCost(a.count, a.sums, b.count, b.sums) < lambda ||
        (lambda === this.#finalLambda && (a.count < minimumPixels || b.count < minimumPixels))) {
      this.#merge(first, second);
      return;
    }
    if (this.#pixelCost(pixel, second) < this.#pixelCost(pixel, first)) {
      this.#move(pixel, first, second);
    } else if (this.#pixelCost(neighbor, first) < this.#pixelCost(neighbor, second)) {
      this.#move(neighbor, second, first);
    }
  }

  iterate(input) {
    if (input === null || typeof input !== 'object') throw new Error('Expected optimizer pass settings.');
    const lambda = readNonnegativeFloat(input.lambda, 'lambda');
    let minimumPixels = this.#minimumPixels;
    if (input.minimumPixels !== undefined) {
      minimumPixels = input.minimumPixels;
      if (!Number.isInteger(minimumPixels) || minimumPixels < 0 || minimumPixels > int32Max) {
        throw new Error('Pass minimumPixels must be a nonnegative int32 value.');
      }
    }
    const { mode } = input;
    if (mode !== 'axis' && mode !== 'eight' && mode !== 'small') {
      throw new Error('Optimizer mode must be axis, eight, or small.');
    }
    const directions = mode === 'eight' ? [[1, 0], [0, 1], [-1, 0], [0, -1], [-1, 1], [1, -1], [-1, -1], [1, 1]] : [[1, 0], [0, 1]];
    for (let y = 0; y < this.#height; y++) {
      for (let x = 0; x < this.#width; x++) {
        const pixel = y * this.#width + x;
        let first = this.#root(pixel);
        for (const [dx, dy] of directions) {
          const column = x + dx, row = y + dy;
          if (column < 0 || column >= this.#width || row < 0 || row >= this.#height) continue;
          const neighbor = row * this.#width + column;
          const second = this.#root(neighbor);
          if (first === second) continue;
          if (mode === 'small') {
            if (this.#statistics[first].count < minimumPixels || this.#statistics[second].count < minimumPixels) {
              this.#merge(first, second);
            }
          } else {
            this.#compare(pixel, neighbor, first, second, lambda, minimumPixels);
          }
          first = this.#root(pixel);
        }
      }
    }
  }

  rebuild() {
    for (let pixel = 0; pixel < this.#indices.length; pixel++) this.#root(pixel);
    this.#rebuildConnected();
  }

  /** Repair diagonal-only contacts; the following rebuild refreshes statistics. */
  repairDiagonalContacts() {
    for (let pixel = 0; pixel < this.#indices.length; pixel++) this.#root(pixel);
    repairDiagonalMap({ width: this.#width, height: this.#height, bgra: this.#bgra }, this.#indices);
  }

  projectPalette({ diagonalExtraction = false } = {}) {
    if (typeof diagonalExtraction !== 'boolean') throw new Error('diagonalExtraction must be a boolean.');
    if (this.#palette !== null) throw new Error('Regions already have a projected palette.');
    if (this.#statistics.some(statistic => statistic.count === 0)) {
      throw new Error('Rebuild connected regions before projecting their palette.');
    }
    function* regionBytes(statistics) {
      for (const statistic of statistics) {
        const scale = f32((diagonalExtraction ? 255 : 1) / f32(statistic.count));
        const bytes = Array.from(statistic.sums, sum => {
          const mean = f32(sum * scale);
          return Math.trunc(diagonalExtraction ? mean : mean * 255 + 0.5) & 255;
        });
        yield bytes;
      }
    }
    const { palette, colorIndices } = paletteFromByteColors(regionBytes(this.#statistics));
    for (const [region, statistic] of this.#statistics.entries()) statistic.colorIndex = colorIndices[region];
    this.#palette = palette;
    if (diagonalExtraction) {
      // The native small-to-large color search starts at -10, so nonnegative
      // squared errors retain every original region color. Its next stage
      // labels pixels by color, then rebuilds their four-connected components.
      for (let pixel = 0; pixel < this.#indices.length; pixel++) {
        this.#indices[pixel] = this.#statistics[this.#root(pixel)].colorIndex;
      }
      this.#statistics = Array.from({ length: palette.length / 4 }, (_, colorIndex) => ({ count: 0, sums: new Float32Array(4), colorIndex }));
      this.#rebuildConnected();
    }
    return Float32Array.from(this.#palette);
  }

  labels() {
    return Int32Array.from(this.#indices, (_, pixel) => this.#root(pixel));
  }

  get regionCount() {return this.#statistics.length;}

  // Observing a native checkpoint must not compress parent links as labels()
  // does. This sink is internal to the canonical document pipeline.
  publish(onProgress,phase,substep,counter = 0,palette) {
    if (onProgress === undefined) return;
    const updated = {indices:this.#indices};
    if (palette !== undefined) updated.palette = palette;
    onProgress(phase,substep,counter,updated);
  }

  snapshot() {
    const indices = this.labels();
    const counts = Int32Array.from(this.#statistics, statistic => statistic.count);
    const colorIndices = Int32Array.from(this.#statistics, statistic => statistic.colorIndex);
    const sums = new Float32Array(counts.length * 4);
    for (let region = 0; region < counts.length; region++) sums.set(this.#statistics[region].sums, region * 4);
    return { indices, counts, sums, colorIndices, activeCount: counts.reduce((total, count) => total + (count > 0), 0) };
  }
}

function readSegmentationParameters(parameters) {
  if (parameters === null || typeof parameters !== 'object') throw new Error('Expected numeric segmentation parameters.');
  if (parameters['Segmenter::size_penalty_weight'] !== 0) {
    throw new Error('Nonzero size penalties remain under reconstruction.');
  }
  const diagonalExtraction = parameters['Segmenter::use_over_seg_diag_extr'];
  if (diagonalExtraction !== 0 && diagonalExtraction !== 1) throw new Error('Segmenter::use_over_seg_diag_extr must be 0 or 1.');
  const integers = {};
  for (const name of ['max_iterations', 'num_iter_btwn_renum', 'min_num_pixels']) {
    const value = parameters[`Segmenter::${name}`];
    if (!Number.isInteger(value) || value < (name === 'min_num_pixels' ? 0 : 1) || value > int32Max) {
      throw new Error(`Segmenter::${name} must be a valid ${name === 'min_num_pixels' ? 'nonnegative' : 'positive'} int32 value.`);
    }
    integers[name] = value;
  }
  const floats = {};
  for (const name of ['lambda_pre', 'lambda_initial', 'lambda_final', 'rise_fraction']) {
    floats[name] = readNonnegativeFloat(parameters[`Segmenter::${name}`], `Segmenter::${name}`);
    if (name !== 'lambda_pre' && floats[name] === 0) throw new Error(`Segmenter::${name} must be positive.`);
  }
  return { ...integers, ...floats, diagonalExtraction: diagonalExtraction === 1,
    antiAlias:parameters['Shared::is_anti_aliased']===1,parameterState:parameters };
}

function runBaseSegmentation({ width, height, bgra }, settings, diagonalExtraction, onProgress) {
  const regions = initializeTileRegions(width, height, bgra, settings.lambda_pre,onProgress);
  const optimizer = new RegionOptimizerState({ width, height, bgra: Uint8Array.from(bgra), regions, finalLambda: settings.lambda_final,
    minimumPixels: settings.min_num_pixels });
  const multiplier = f32(Math.exp(Math.log(settings.lambda_final / settings.lambda_initial) /
    (settings.max_iterations * settings.rise_fraction)));
  let lambda = settings.lambda_initial;
  // The native diagonal branch temporarily changes its live parameter field.
  // Cancellation leaves that write in place; profile loading resets it on retry.
  if (onProgress && diagonalExtraction) settings.parameterState['Segmenter::min_num_pixels'] = 0;
  for (let iteration = 0; iteration < settings.max_iterations; iteration++) {
    optimizer.publish(onProgress,3,1,Math.trunc(((iteration*50)|0)/settings.max_iterations+0.5)|0);
    optimizer.iterate({ lambda, mode: 'axis', minimumPixels: diagonalExtraction ? 0 : settings.min_num_pixels });
    if ((iteration + 1) % settings.num_iter_btwn_renum === 0) optimizer.rebuild();
    lambda = f32(lambda * multiplier);
    if (lambda > settings.lambda_final) lambda = settings.lambda_final;
  }
  if (diagonalExtraction) {
    optimizer.iterate({ lambda, mode: 'eight', minimumPixels: 0 });
    optimizer.repairDiagonalContacts();
    if (onProgress) settings.parameterState['Segmenter::min_num_pixels'] = settings.min_num_pixels;
  }
  optimizer.publish(onProgress,3,2);
  optimizer.rebuild();
  if (settings.min_num_pixels > 0) {
    optimizer.iterate({ lambda, mode: 'small' });
    optimizer.rebuild();
  }
  optimizer.publish(onProgress,3,3);
  return { optimizer, lambda };
}

/** Original 0x4ab730(0), before palette extraction or subpixel optimization. */
export function presegmentRegions(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected a segmentation object.');
  const settings = readSegmentationParameters(input.parameters);
  return runBaseSegmentation(readRasterInput(input), settings, false).optimizer.snapshot();
}

function segmentUnweightedState(input,onProgress) {
  if (input === null || typeof input !== 'object') throw new Error('Expected a segmentation object.');
  if (input.parameters?.['Shared::is_anti_aliased'] !== 0) throw new Error('segmentUnweighted requires anti-aliasing disabled.');
  const settings = readSegmentationParameters(input.parameters);
  if (input.palette !== undefined) {
    const raster = readRasterInput(input);
    const palette = readPaletteColors(input.palette);
    const labels = assignPalette({ bgra: raster.bgra, palette });
    onProgress?.(2,0,0,{indices:labels});
    if (onProgress !== undefined) {
      const initial = new RegionOptimizerState({...raster,bgra:Uint8Array.from(raster.bgra),regions:labels,palette,
        finalLambda:settings.lambda_final,minimumPixels:settings.min_num_pixels});
      initial.publish(onProgress,2,1);
    }
    // The second native assignment has the same immutable inputs. Retain its
    // raw color labels separately from the first connected-component map.
    onProgress?.(2,2,0,{indices:labels});
    // The native second assignment overwrites its initial connected map. Repair
    // that color map directly, then rebuild components with palette identifiers.
    repairDiagonalMap(raster, labels);
    onProgress?.(2,3,0,{indices:labels});
    onProgress?.(2,4);
    return finishPaletteSegmentation(raster, settings, labels, palette,onProgress);
  }
  const { optimizer, lambda } = runBaseSegmentation(readRasterInput(input), settings, settings.diagonalExtraction,onProgress);
  if (onProgress !== undefined && settings.diagonalExtraction) {
    for (let region=0;region<optimizer.regionCount;region+=4)
      optimizer.publish(onProgress,3,4,Math.trunc(((region*50)|0)/(optimizer.regionCount-1)+0.5)|0);
  }
  const palette = optimizer.projectPalette({ diagonalExtraction: settings.diagonalExtraction });
  optimizer.publish(onProgress,6,-1,0,palette);
  // Native palette assignment precedes a final minimum-region cleanup. Preserve
  // the absorbing region's color identifier through that cleanup and rebuild.
  if (settings.min_num_pixels > 0) {
    optimizer.iterate({ lambda, mode: 'small' });
    optimizer.rebuild();
  }
  const { indices, counts, sums, colorIndices } = optimizer.snapshot();
  const result = { indices, counts, sums, palette, colorIndices };
  onProgress?.(6,-2,0,segmentationResult(result));
  return result;
}

function segmentationResult({ indices, counts, palette, colorIndices }) {
  return { indices, counts, palette, colorIndices };
}

/** Complete the recovered unweighted branch, including optional diagonals. */
export function segmentUnweighted(input) {
  return segmentationResult(segmentUnweightedState(input));
}

function finishPaletteSegmentation(raster, settings, labels, palette,onProgress) {
  const transferred = new RegionOptimizerState({ ...raster, bgra: Uint8Array.from(raster.bgra),
    regions: labels, palette, finalLambda: settings.lambda_final, minimumPixels: settings.min_num_pixels });
  if (settings.antiAlias) transferred.publish(onProgress,4,-2,0,palette);
  transferred.publish(onProgress,6,-1,0,palette);
  if (settings.min_num_pixels > 0) {
    transferred.iterate({ lambda: settings.lambda_final, mode: 'small' });
    transferred.rebuild();
  }
  const { indices, counts, sums, colorIndices } = transferred.snapshot();
  const result = { indices, counts, sums, palette, colorIndices };
  onProgress?.(6,-2,0,segmentationResult(result));
  return result;
}

/** Internal full segmentation state. Preserve already computed region moments
 * for image classification instead of repeating its pixel/statistic pass. */
export function segmentWithMoments(input, source, onProgress) {
  if (input === null || typeof input !== 'object') throw new Error('Expected a segmentation object.');
  const antiAlias = input.parameters?.['Shared::is_anti_aliased'];
  if (antiAlias === 0) return segmentUnweightedState(input,onProgress);
  if (antiAlias !== 1) throw new Error('Shared::is_anti_aliased must be 0 or 1.');
  if (input.parameters.clusterColors !== undefined && typeof input.parameters.clusterColors !== 'boolean') {
    throw new Error('clusterColors must be a boolean when supplied.');
  }
  const settings = readSegmentationParameters(input.parameters);
  const raster = readRasterInput(input);
  const suppliedPalette = input.palette === undefined ? undefined : readPaletteColors(input.palette);
  const initial = runBaseSegmentation(raster, settings, false,onProgress).optimizer.labels();
  const optimized = segmentSubpixelState({ ...raster, regions:initial, parameters:input.parameters, palette:suppliedPalette }, source,onProgress);
  const { palette, indices } = transferSubpixelPalette(optimized, suppliedPalette);
  return finishPaletteSegmentation(raster, settings, indices, palette,onProgress);
}

/** Select the original automatic or supplied-palette branch for prepared pixels. */
export function segment(input) {
  return segmentationResult(segmentWithMoments(input, input));
}
