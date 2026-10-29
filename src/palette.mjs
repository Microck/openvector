import { readPaletteInput, readPaletteColors, readPaletteScoreInput, validateBgra } from './pixel-input.mjs';
import { initializeContourCoordinates } from './contour-initialization.mjs';
import { smoothContourGeometry } from './contour-smoothing.mjs';
import { scorePreparedWeightedPalette } from './blending.mjs';
import { buildPaletteFeatures, classifyPaletteFeatures } from './automatic-colors.mjs';
import { analyzeImage, analyzePreparedImage } from './image-pipeline.mjs';
import { ImageDocument } from './image-document.mjs';

const f32 = Math.fround;
const byteScale = 1 / 255;
const byteToUnit = Float32Array.from({ length: 256 }, (_, value) => f32(value * f32(byteScale)));
// Enumerate four-dimensional offsets once, with B fastest as in the histogram.
const neighborOffsets = Array.from({ length: 81 }, (_, index) => [
  index % 3 - 1, Math.floor(index / 3) % 3 - 1,
  Math.floor(index / 9) % 3 - 1, Math.floor(index / 27) - 1,
]).filter(offset => offset.some(value => value !== 0));
const axisNeighborOffsets = neighborOffsets.filter(offset => offset.reduce((sum, value) => sum + Math.abs(value), 0) === 1);

// Match the original SSE instructions. Rounding only the final sum can change
// which color wins, especially for nearly equal colors and alpha differences.
function distance(palette, offset, blue, green, red, alpha) {
  const b = f32(blue - palette[offset]);
  const g = f32(green - palette[offset + 1]);
  const r = f32(red - palette[offset + 2]);
  const a = f32(alpha - palette[offset + 3]);
  let cost = f32(f32(b * b) + f32(g * g));
  cost = f32(cost + f32(r * r));
  return f32(cost + f32(a * a));
}

function assignIndices(bgra, colors, indices) {
  for (let pixel = 0; pixel < indices.length; pixel++) {
    const offset = pixel * 4;
    const blue = byteToUnit[bgra[offset]];
    const green = byteToUnit[bgra[offset + 1]];
    const red = byteToUnit[bgra[offset + 2]];
    const alpha = byteToUnit[bgra[offset + 3]];
    let best = 0;
    let bestCost = distance(colors, 0, blue, green, red, alpha);
    for (let color = 1; color < colors.length / 4; color++) {
      const cost = distance(colors, color * 4, blue, green, red, alpha);
      if (cost < bestCost) {
        best = color;
        bestCost = cost;
      }
    }
    indices[pixel] = best;
  }
}

/** Assign engine-format BGRA pixels to an explicitly supplied palette. */
export function assignPalette(input) {
  const { bgra, colors } = readPaletteInput(input);
  const indices = new Int32Array(bgra.length / 4);
  assignIndices(bgra, colors, indices);
  return indices;
}

/** Score explicit unweighted assignments, without changing the palette or map. */
export function scoreUnweightedPalette(input) {
  return scorePreparedUnweightedPalette(readPaletteScoreInput(input));
}

function scorePreparedUnweightedPalette({ bgra, colors, indices, costPerColor }) {
  const colorCount = colors.length / 4;
  let sum = 0;
  for (let pixel = 0; pixel < indices.length; pixel++) {
    const offset = pixel * 4;
    sum = f32(sum + distance(colors, indices[pixel] * 4, byteToUnit[bgra[offset]],
      byteToUnit[bgra[offset + 1]], byteToUnit[bgra[offset + 2]], byteToUnit[bgra[offset + 3]]));
  }
  // The original combines an x87 logarithm with an SSE float32 color penalty.
  // Generation computes this before quantizing its final palette colors.
  const error = f32(Math.log10(1 + sum));
  const colorCost = f32(f32(colorCount - 2) * f32(costPerColor));
  return { total: f32(error + colorCost), error, colors: colorCost };
}

function readMeasurementInput(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected an anti-alias measurement object.');
  const { width, height, bgra } = input;
  validateBgra(bgra);
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 2 || height < 2 ||
      width > 2147483647 || height > 2147483647 || width * height * 4 !== bgra.length) {
    throw new Error('Measurement needs dimensions of at least 2 by 2 with exactly width * height BGRA pixels.');
  }
  return { width, height, bgra };
}

function measurementContrast(bgra, center, neighbor) {
  // The measurement kernel accumulates double-precision squared differences;
  // palette assignment deliberately uses a separate float32 distance function.
  let contrast = 0;
  for (let channel = 0; channel < 4; channel++) {
    const difference = f32(byteToUnit[bgra[neighbor + channel]] - byteToUnit[bgra[center + channel]]);
    contrast += difference * difference;
  }
  return contrast;
}

/** Internal contrast kernel; callers own validated raster dimensions and bytes. */
export function calculateMeasurement(width, height, bgra) {
  const measurement = new Uint8Array(width * height);
  const normalization = Math.sqrt(12);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const center = (y * width + x) * 4;
      const top = ((y === 0 ? 1 : y - 1) * width + x) * 4;
      const bottom = ((y === height - 1 ? y - 1 : y + 1) * width + x) * 4;
      const left = (y * width + (x === 0 ? 1 : x - 1)) * 4;
      const right = (y * width + (x === width - 1 ? x - 1 : x + 1)) * 4;
      let total = measurementContrast(bgra, center, top);
      total += measurementContrast(bgra, center, bottom);
      total += measurementContrast(bgra, center, left);
      total += measurementContrast(bgra, center, right);
      measurement[y * width + x] = Math.trunc(Math.max(0, Math.min(255, Math.sqrt(total) * 255 / normalization)));
    }
  }
  return measurement;
}

function measurementWeights(measurement,edgeScale,edgeFloor) {
  // Constructor and resolved document profiles supply distinct edge controls.
  // Preserve the original multiplication order and double histogram weights.
  return Float64Array.from(measurement, value => Math.max(1 - value * edgeScale * byteScale, edgeFloor));
}

function calculateWeighting(width, height, bgra,edgeScale,edgeFloor) {
  const measurement = calculateMeasurement(width, height, bgra);
  return { measurement, pixelWeights: measurementWeights(measurement,edgeScale,edgeFloor) };
}

/** Measure the original reflected-neighbor contrast, preserving mixed precision. */
export function measureAntiAlias(input) {
  const { width, height, bgra } = readMeasurementInput(input);
  return calculateMeasurement(width, height, bgra);
}

function mergePeakRuns(left, right) {
  const merged = [];
  let leftIndex = 0;
  let rightIndex = 0;
  while (leftIndex < left.length && rightIndex < right.length) {
    const a = left[leftIndex];
    const b = right[rightIndex];
    // Preserve the original non-lexicographic channel test. Both directions
    // can compare true, making the merge schedule part of observable ordering.
    if (a.weight < b.weight || (a.weight === b.weight && a.color.some((channel, index) => channel < b.color[index]))) {
      merged.push(right[rightIndex++]);
    } else {
      merged.push(left[leftIndex++]);
    }
  }
  while (leftIndex < left.length) merged.push(left[leftIndex++]);
  while (rightIndex < right.length) merged.push(right[rightIndex++]);
  return merged;
}

function orderPeaks(peaks) {
  const runs = Array.from({ length: 26 }, () => []);
  let used = 0;
  for (const peak of peaks) {
    let carry = [peak];
    let run = 0;
    while (run < used && runs[run].length !== 0) {
      carry = mergePeakRuns(runs[run], carry);
      runs[run++] = [];
    }
    runs[run] = carry;
    if (run === used) used++;
  }
  for (let run = 1; run < used; run++) runs[run] = mergePeakRuns(runs[run], runs[run - 1]);
  return used === 0 ? [] : runs[used - 1];
}

function validateHistogramControls(bins, connected) {
  if (bins !== 8 && bins !== 16) throw new Error('Histogram bins must be 8 or 16.');
  if (typeof connected !== 'boolean') throw new Error('Histogram connected must be true or false.');
}

function readGenerationControls(input) {
  const { colorCount, bins, connected } = input;
  validateHistogramControls(bins, connected);
  if (!Number.isInteger(colorCount) || colorCount < 1 || colorCount > 2147483647) {
    throw new Error('colorCount must be a positive int32 integer.');
  }
  return { colorCount, bins, connected };
}

/** Find and order four-dimensional color-histogram peaks as the original does. */
export function findHistogramPeaks(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected a histogram object.');
  const { histogram, bins, connected } = input;
  validateHistogramControls(bins, connected);
  if ((!Array.isArray(histogram) && !(histogram instanceof Float64Array)) || histogram.length !== bins ** 4) {
    throw new Error('Histogram must contain bins^4 weights.');
  }
  for (const weight of histogram) {
    if (!Number.isFinite(weight) || weight < 0) throw new Error('Histogram weights must be finite and nonnegative.');
  }
  return detectHistogramPeaks(histogram, bins, connected);
}

/** Internal peak kernel; callers own validated histogram storage and controls. */
export function detectHistogramPeaks(histogram, bins, connected) {
  const peaks = [];
  const neighbors = connected ? neighborOffsets : axisNeighborOffsets;
  for (let index = 0; index < histogram.length; index++) {
    const weight = histogram[index];
    if (weight < 2) continue;
    const b = index % bins;
    const g = Math.floor(index / bins) % bins;
    const r = Math.floor(index / bins ** 2) % bins;
    const a = Math.floor(index / bins ** 3);
    let maximum = true;
    for (const [db, dg, dr, da] of neighbors) {
      const nb = b + db, ng = g + dg, nr = r + dr, na = a + da;
      if (nb < 0 || nb >= bins || ng < 0 || ng >= bins || nr < 0 || nr >= bins || na < 0 || na >= bins) continue;
      if (histogram[((na * bins + nr) * bins + ng) * bins + nb] > weight) {
        maximum = false;
        break;
      }
    }
    if (maximum) peaks.push({ weight, color: [b, g, r, a].map(value => value * (256 / bins) + 128 / bins) });
  }
  const ordered = orderPeaks(peaks);
  const bgra = new Uint8Array(ordered.length * 4);
  const weights = new Float64Array(ordered.length);
  for (let peak = 0; peak < ordered.length; peak++) {
    bgra.set(ordered[peak].color, peak * 4);
    weights[peak] = ordered[peak].weight;
  }
  return { bgra, weights };
}

function prepareHistogramPeaks(bgra, bins, connected, weighting) {
  const histogram = new Float64Array(bins ** 4);
  const shift = bins === 8 ? 5 : 4;
  for (let pixel = 0; pixel < bgra.length; pixel += 4) {
    const index = (((bgra[pixel + 3] >> shift) * bins + (bgra[pixel + 2] >> shift)) * bins +
      (bgra[pixel + 1] >> shift)) * bins + (bgra[pixel] >> shift);
    histogram[index] += weighting === null ? 1 : weighting.pixelWeights[pixel / 4];
  }
  return detectHistogramPeaks(histogram, bins, connected);
}

function finalizePalette(generated) {
  // Original finalization quantizes colors after scoring and the last
  // assignment pass. The x87 multiply/add use double precision here.
  const colors = generated.palette;
  for (let channel = 0; channel < colors.length; channel++) {
    colors[channel] = byteToUnit[Math.trunc(colors[channel] * 255 + 0.5) & 255];
  }
  return generated;
}

function readColorAnalysisInput(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected an automatic color analysis object.');
  const { bgra, width, height, bins, connected, antiAlias, costPerColor } = input;
  validateBgra(bgra);
  validateHistogramControls(bins, connected);
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1 ||
      width > 2147483647 || height > 2147483647 || width * height > 2147483647 ||
      width * height * 4 !== bgra.length) {
    throw new Error('Automatic color analysis needs positive int32 dimensions and exactly width * height BGRA pixels.');
  }
  if (typeof antiAlias !== 'boolean' || (antiAlias && (width < 2 || height < 2))) {
    throw new Error('antiAlias must be boolean, and anti-alias analysis needs dimensions of at least 2 by 2.');
  }
  if (!Number.isFinite(costPerColor) || !Number.isFinite(f32(costPerColor))) {
    throw new Error('costPerColor must be a finite float32 number.');
  }
  return { bgra,width,height,bins,connected,antiAlias,costPerColor,edgeScale:50,edgeFloor:0.01 };
}

/** Retains the original application-level random sequence across operations. */
export class CoreSession {
  #randomState = 1;

  get randomState() { return this.#randomState; }

  /** Analyze decoded pixels through automatic type/quality, working reduction
   * and color-count evaluation on this session's retained random stream. */
  analyzeImage(input) {
    return analyzeImage(input, this.#imageAnalysisServices(input?.parameters));
  }

  #imageAnalysisServices(parameters) {
    return {analyzeColorCounts:input => this.#analyzeColorCounts({...readColorAnalysisInput(input),
      edgeScale:parameters['EdgeImage::edge_weight_scale'],edgeFloor:parameters['EdgeImage::edge_weight_floor']}),
      randomState:() => this.randomState};
  }

  /** Create an owned image document on this retained application random stream. */
  createDocument(input) {
    return new ImageDocument(input,{ prepare:controls => this.#preparePaletteAnalysis(controls),
      classify:controls => analyzePreparedImage(controls,this.#imageAnalysisServices(controls.parameters)),
      // A document keeps mutations and random draws when a native business
      // method is interrupted. Standalone smoothContours owns its rollback.
      smooth:(controls,onProgress) => smoothContourGeometry(controls, () => this.#nextRandom(), onProgress),
      randomState:() => this.randomState });
  }

  #nextRandom() {
    this.#randomState = this.#randomState * 2003 % 9973;
    // Smoothing consumes the raw double. Palette writes round through their
    // float32 color arrays, without changing the shared random stream.
    return this.#randomState * (1 / 9973);
  }

  initializeContours(input) {
    return initializeContourCoordinates(input, () => this.#nextRandom());
  }

  smoothContours(input) {
    const before = this.#randomState;
    try {
      return smoothContourGeometry(input, () => this.#nextRandom());
    } catch (error) {
      // A failed library operation must not consume part of the application
      // stream. Geometry and measurement caches are already owned by the call.
      this.#randomState = before;
      throw error;
    }
  }

  #randomColor(colors, offset) {
    // Native random consumption uses R, G, B despite BGRA storage order.
    colors[offset + 2] = this.#nextRandom();
    colors[offset + 1] = this.#nextRandom();
    colors[offset] = this.#nextRandom();
    colors[offset + 3] = 1;
  }

  refineUnweighted(input) {
    const { bgra, colors } = readPaletteInput(input);
    return this.#refine(bgra, colors);
  }

  refineWeighted(input) {
    const { width, height, bgra } = readMeasurementInput(input);
    const colors = readPaletteColors(input.palette);
    return this.#refine(bgra, colors, calculateWeighting(width, height, bgra,50,0.01));
  }

  generateUnweighted(input) {
    if (input === null || typeof input !== 'object') throw new Error('Expected a palette generation object.');
    const { bgra } = input;
    validateBgra(bgra);
    const { colorCount, bins, connected } = readGenerationControls(input);
    const peaks = prepareHistogramPeaks(bgra, bins, connected, null);
    return finalizePalette(this.#generate(bgra, colorCount, peaks.bgra));
  }

  generateWeighted(input) {
    const { width, height, bgra } = readMeasurementInput(input);
    const { colorCount, bins, connected } = readGenerationControls(input);
    const weighting = calculateWeighting(width, height, bgra,50,0.01);
    const peaks = prepareHistogramPeaks(bgra, bins, connected, weighting);
    return finalizePalette(this.#generate(bgra, colorCount, peaks.bgra, weighting));
  }

  /** Analyze all original 2..12 candidates using one histogram and session
   * random stream. Image decoding, type/quality detection and final document
   * selection are separate stages and are not implied by this analysis. */
  analyzeColorCounts(input) {
    return this.#analyzeColorCounts(readColorAnalysisInput(input));
  }

  #analyzeColorCounts(controls) {
    const prepared = this.#preparePaletteAnalysis(controls);
    const candidates = [];
    for (let colorCount = 2; colorCount <= 12; colorCount++) candidates.push(prepared.evaluate(colorCount));
    return { candidates,peakWeights:prepared.peakWeights,...prepared.classify(candidates) };
  }

  #preparePaletteAnalysis({ bgra,width,height,bins,connected,antiAlias,costPerColor,edgeScale,edgeFloor,onProgress }) {
    const weighting = antiAlias ? calculateWeighting(width, height, bgra,edgeScale,edgeFloor) : null;
    const peaks = prepareHistogramPeaks(bgra, bins, connected, weighting);
    const evaluate = colorCount => {
      const generated = this.#generate(bgra, colorCount, peaks.bgra, weighting,
        onProgress === undefined ? undefined : (iteration,live) => onProgress(colorCount,iteration,live));
      const scoringInput = { width, height, bgra, colors:generated.palette, indices:generated.indices, costPerColor };
      const score = antiAlias ? scorePreparedWeightedPalette(scoringInput) : scorePreparedUnweightedPalette(scoringInput);
      return { colorCount,...finalizePalette(generated),score,randomState:this.randomState };
    };
    const classify = candidates => {
      const features = buildPaletteFeatures(bgra,candidates,peaks.weights);
      return { features,classification:classifyPaletteFeatures(features) };
    };
    return { peakWeights:peaks.weights,evaluate,classify };
  }

  #generate(bgra, colorCount, peaks, weighting = null, progress) {
    const colors = new Float32Array(colorCount * 4);
    for (let color = 0; color < colorCount; color++) {
      const offset = color * 4;
      if (offset < peaks.length) {
        for (let channel = 0; channel < 4; channel++) colors[offset + channel] = byteToUnit[peaks[offset + channel]];
      } else {
        this.#randomColor(colors, offset);
      }
    }
    return this.#refine(bgra, colors, weighting,progress);
  }

  #refine(bgra, colors, weighting = null, progress) {
    const indices = new Int32Array(bgra.length / 4);
    const means = new Float32Array(colors.length);
    const counts = new Float32Array(colors.length / 4);
    const weights = new Float64Array(counts.length);
    let converged = false;
    let passes = 0;
    for (let iteration = 0; iteration < 10; iteration++) {
      // The document observes the actual candidate before assignment. A new
      // indices allocation has no produced labels until the first pass ends.
      progress?.(iteration,{palette:colors,indices:iteration === 0 ? null : indices});
      assignIndices(bgra, colors, indices);
      passes++;
      if (iteration === 9 || converged) {
        weights.fill(0);
        for (let pixel = 0; pixel < indices.length; pixel++) {
          // Membership collection intentionally uses raw contrast rather than
          // the reduced histogram/refinement weight in the anti-alias branch.
          weights[indices[pixel]] += weighting === null ? 1 : weighting.measurement[pixel] * byteScale;
        }
      }
      if (converged) break;

      means.fill(0);
      counts.fill(0);
      for (let pixel = 0; pixel < indices.length; pixel++) {
        const color = indices[pixel];
        const weight = weighting === null ? 1 : f32(weighting.pixelWeights[pixel]);
        for (let channel = 0; channel < 4; channel++) {
          means[color * 4 + channel] += f32(byteToUnit[bgra[pixel * 4 + channel]] * weight);
        }
        counts[color] += weight;
      }
      for (let color = 0; color < counts.length; color++) {
        const offset = color * 4;
        if (counts[color] > 0) {
          const reciprocal = f32(1 / counts[color]);
          for (let channel = 0; channel < 4; channel++) means[offset + channel] *= reciprocal;
        } else {
          this.#randomColor(means, offset);
        }
      }
      let change = 0;
      for (let channel = 0; channel < colors.length; channel++) {
        change = Math.max(change, Math.abs(f32(colors[channel] - means[channel])));
      }
      colors.set(means);
      converged = change < f32(0.01);
    }
    // The last assignment can precede the final color update at the iteration
    // limit. Preserve that ordering and the original empty-color cleanup.
    for (let color = 0; color < counts.length; color++) {
      if (counts[color] === 0) colors.fill(0, color * 4, color * 4 + 4);
    }
    return { palette: colors, indices, weights, passes };
  }
}
