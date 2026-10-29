import { boundaryMask } from './blending.mjs';
import { calculateMeasurement, detectHistogramPeaks } from './palette.mjs';

// The recovered image type/quality model consumes twenty float32 features.
// Tuples are [feature, original threshold, less-than child, other child].
// Equal and unordered comparisons take the other child. Extension tuples
// compare the exact string supplied by the original filename-stage producer.
const imageTypeQualityModel = [18, 0.0134977,
  [4, 0.960049,
    [2, 0.0197072,
      [18, 0.00219943,
        [3, 0.0923396,
          [16, 0.111529,
            "logo_aa 2",
            [11, 0.00792502,
              [13, 57.5, "logo_aa 0", "logo_aa 1"],
              [6, 0.00258916, "logo_aa 2", "logo_aa 2"]]],
          [2, 0.0111265,
            [0, 0.588894, "logo_aa 2", "logo_aa 2"],
            [8, 0.000665414,
              [9, 400368.0,
                "logo_aa 2",
                [19, 3.515e-07, "logo_aa 1", "logo_aa 2"]],
              "logo_aa 1"]]],
        [0, 0.406237,
          [2, 0.0162755, "logo_aa 2", "photo 1"],
          "logo_aa 2"]],
      [18, 0.00291393,
        [2, 0.0285461,
          [4, 0.674069,
            [2, 0.0238979, "logo_aa 2", "logo_aa 1"],
            [18, 0.00132773,
              ["extension", ".bmp",
                "logo_aa 1",
                ["extension", ".jpeg",
                  "logo_aa 1",
                  ["extension", ".png",
                    "logo_aa 1",
                    ["extension", ".tiff", "logo_aa 1", "logo_aa 2"]]]],
              "logo_aa 2"]],
          [5, 0.998571, "logo_aa 1", "logo_aa 1"]],
        [12, 2.63168,
          [2, 0.0540923, "photo 1", "logo_aa 1"],
          [2, 0.0380126,
            [0, 0.400423,
              [3, 0.136656,
                [8, 0.000193388, "photo 1", "logo_aa 2"],
                "logo_aa 1"],
              "logo_aa 1"],
            "logo_aa 1"]]]],
    [18, 5.16e-07,
      [2, 0.00130897, "logo 2", "logo_aa 2"],
      [0, 0.380912,
        [2, 0.00114401, "logo_aa 2", "logo_aa 1"],
        "logo_aa 1"]]],
  [5, 0.867741,
    [18, 0.0649332,
      [15, 0.0323685,
        "logo_aa 2",
        [10, 9497.5, "photo 1", "logo_aa 1"]],
      [0, 0.20806,
        [15, 0.158062,
          [4, 0.619184,
            [7, 0.00190362,
              [1, 2.355e-05, "logo_aa 2", "photo 1"],
              "photo 1"],
            "photo 1"],
          [12, 7.34716, "photo 1", "photo 1"]],
        [0, 0.928628, "photo 1", "logo_aa 0"]]],
    [2, 0.0124402,
      [4, 0.984352,
        [3, 0.091711,
          [7, 0.00301763, "logo_aa 2", "photo 1"],
          "logo_aa 2"],
        "logo 2"],
      [0, 0.241446,
        [3, 0.183605,
          [2, 0.0225216,
            [5, 0.915114,
              "photo 1",
              [4, 0.649337,
                "logo_aa 2",
                [13, 80543.5, "photo 1", "logo_aa 2"]]],
            [2, 0.103327,
              [15, 0.327859, "photo 1", "photo 1"],
              "logo_aa 1"]],
          [7, 0.00268996,
            "photo 1",
            [15, 1.19829, "logo_aa 1", "photo 1"]]],
        [3, 0.132534, "photo 1", "logo_aa 2"]]]]];

/** Internal decision kernel. The image feature producer owns twenty float32
 * inputs and the exact extension string; NaNs follow original unordered
 * branches. Extension normalization belongs to the filename stage. */
export function classifyImageFeatures(features, extension) {
  let node = imageTypeQualityModel;
  while (typeof node !== 'string') {
    const matches = node[0] === 'extension' ? extension === node[1] : features[node[0]] < node[1];
    node = matches ? node[2] : node[3];
  }
  return { label:node, imageType:node.startsWith('photo') ? 0 : node.startsWith('logo_aa') ? 1 : 2,
    quality:2 - Number(node.at(-1)) };
}

const f32 = Math.fround;
const X86_INDEFINITE_F32 = new Float32Array(new Uint32Array([0xffc00000]).buffer)[0];
const byteToUnit = Float32Array.from({ length:256 }, (_, byte) => f32(byte * f32(1 / 255)));

function createClassifierBlurKernel() {
  const weights = new Float64Array(25);
  let offset = 0;
  for (let y = -2; y <= 2; y++) {
    for (let x = -2; x <= 2; x++) {
      // The native Gaussian producer stores the exponent as float32 before
      // dividing by the double normalization constant.
      weights[offset++] = f32(Math.exp(-(x * x + y * y) / 1.125)) / (0.5625 * (2 * Math.PI));
    }
  }
  return weights;
}

const classifierBlurKernel = createClassifierBlurKernel();
const classifierBlurReciprocal = f32(1 / classifierBlurKernel.reduce((sum, weight) => sum + weight, 0));

/** Internal initial color preparation over validated decoded BGRA bytes.
 * Preserve the double multiply order and truncate each premultiplied channel;
 * integer division or float32 alpha normalization changes some byte results. */
export function premultiplyClassifierPixels({ bgra }) {
  const prepared = Uint8Array.from(bgra);
  for (let pixel = 0; pixel < prepared.length; pixel += 4) {
    const alpha = bgra[pixel + 3] * (1 / 255);
    for (let channel = 0; channel < 3; channel++) prepared[pixel + channel] = Math.trunc(bgra[pixel + channel] * alpha);
  }
  return prepared;
}

/** Internal fixed Gaussian over a validated prepared Q8 raster. Clamp edge
 * coordinates, preserve alpha and own the output. Float32 scratch stores after
 * every double multiply/add are part of the original convolution contract. */
export function blurClassifierPixels({ width, height, bgra }) {
  const blurred = Uint8Array.from(bgra), sums = new Float32Array(3);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      sums.fill(0);
      let kernel = 0;
      for (let dy = -2; dy <= 2; dy++) {
        const row = Math.min(height - 1, Math.max(y + dy, 0));
        for (let dx = -2; dx <= 2; dx++) {
          const column = Math.min(width - 1, Math.max(x + dx, 0));
          const pixel = (row * width + column) * 4, weight = classifierBlurKernel[kernel++];
          for (let channel = 0; channel < 3; channel++) sums[channel] += bgra[pixel + channel] * weight;
        }
      }
      for (let channel = 0; channel < 3; channel++) {
        const value = f32(sums[channel] * classifierBlurReciprocal);
        blurred[(y * width + x) * 4 + channel] = Math.min(255, Math.max(0, Math.trunc(value + 0.5)));
      }
    }
  }
  return blurred;
}

function bytePixelDistance(bgra, first, second) {
  let distance = 0;
  for (let channel = 0; channel < 4; channel++) {
    const difference = f32(byteToUnit[bgra[first + channel]] - byteToUnit[bgra[second + channel]]);
    distance = f32(distance + f32(difference * difference));
  }
  return distance;
}

/** Internal gradient kernel over a working raster and full source blur.
 * The producer owns valid rasters and working dimensions of at least 2 by 2.
 * Reflect neighbors at the working bounds and retain each grid's own row width;
 * the native source blur can be larger than the working image. */
export function measureClassifierGradients({ width, height, bgra }, blurred) {
  const blurredBgra = blurred.bgra, blurWidth = blurred.width;
  const gradient = new Float32Array(width * height);
  const blurGradient = new Float32Array(width * height);
  let squareSum = 0, blurSquareSum = 0;
  for (let y = 0; y < height; y++) {
    const bottomRow = y + 1 < height ? y + 1 : y - 1, topRow = y === 0 ? 1 : y - 1;
    for (let x = 0; x < width; x++) {
      const pixel = y * width + x, center = pixel * 4;
      const rightColumn = x + 1 < width ? x + 1 : x - 1, leftColumn = x === 0 ? 1 : x - 1;
      const right = (y * width + rightColumn) * 4, left = (y * width + leftColumn) * 4;
      const bottom = (bottomRow * width + x) * 4, top = (topRow * width + x) * 4;
      const blurCenter = (y * blurWidth + x) * 4;
      const blurRight = (y * blurWidth + rightColumn) * 4, blurLeft = (y * blurWidth + leftColumn) * 4;
      const blurBottom = (bottomRow * blurWidth + x) * 4, blurTop = (topRow * blurWidth + x) * 4;
      // The SSE code sums each neighbor's channels before adding that distance
      // to the running value. Adding channels directly changes float32 bits.
      let neighborCost = bytePixelDistance(bgra, center, right);
      neighborCost = f32(neighborCost + bytePixelDistance(bgra, center, left));
      neighborCost = f32(neighborCost + bytePixelDistance(bgra, center, bottom));
      neighborCost = f32(neighborCost + bytePixelDistance(bgra, center, top));
      let blurredCost = 0;
      for (let channel = 0; channel < 4; channel++) {
        let mean = f32(byteToUnit[blurredBgra[blurRight + channel]] + byteToUnit[blurredBgra[blurLeft + channel]]);
        mean = f32(mean + byteToUnit[blurredBgra[blurBottom + channel]]);
        mean = f32(mean + byteToUnit[blurredBgra[blurTop + channel]]);
        mean = f32(mean * 0.25);
        const residual = f32(byteToUnit[blurredBgra[blurCenter + channel]] - mean);
        blurredCost = f32(blurredCost + f32(residual * residual));
      }
      gradient[pixel] = neighborCost;
      blurGradient[pixel] = blurredCost;
      squareSum = f32(squareSum + f32(gradient[pixel] * gradient[pixel]));
      blurSquareSum = f32(blurSquareSum + f32(blurredCost * blurredCost));
    }
  }
  const pixels = f32(width * height);
  const rootMeanSquare = Float32Array.of(f32(Math.sqrt(f32(squareSum / pixels))),
    f32(Math.sqrt(f32(blurSquareSum / pixels))));
  return { gradient, blurGradient, rootMeanSquare };
}

function sampleLuminance(samples, offset) {
  // The native x87 expression keeps double precision until the difference is
  // stored in the float32 profile. Its addition order is G, R, then B.
  return samples[offset + 1] * 0.59 + samples[offset + 2] * 0.3 + samples[offset] * 0.11;
}

/** Internal six-pixel edge sampler. Caller-owned normalized BGRA samples and
 * profile storage avoid allocation in the image scan. Empty support retains
 * native signed NaNs after normalization, even when the edge is rejected. */
export function sampleClassifierEdge(samples, threshold, profile) {
  let previous = sampleLuminance(samples, 0);
  for (let index = 0; index < 5; index++) {
    const current = sampleLuminance(samples, (index + 1) * 4);
    profile[index] = current - previous;
    previous = current;
  }
  if (profile[2] < 0) {
    for (let index = 0; index < 5; index++) profile[index] = 0 - profile[index];
  }
  if (profile[3] < 0) { profile[3] = 0; profile[4] = 0; }
  else if (profile[4] < 0) profile[4] = 0;
  if (profile[1] < 0) { profile[1] = 0; profile[0] = 0; }
  else if (profile[0] < 0) profile[0] = 0;
  let maximum = profile[0], total = profile[0];
  for (let index = 1; index < 5; index++) {
    if (profile[index] > maximum) maximum = profile[index];
    total = f32(total + profile[index]);
  }
  const accepted = profile[2] > threshold && maximum === profile[2];
  const reciprocal = f32(1 / total);
  for (let index = 0; index < 5; index++) {
    // 0 * inf is an invalid operation. x86 yields the single-precision
    // indefinite NaN 0xFFC00000; AArch64 yields 0x7FC00000. Profiles are
    // compared bit-exactly, so generated NaNs take the x86 pattern.
    const product = profile[index] * reciprocal;
    profile[index] = Number.isNaN(product) && !Number.isNaN(profile[index]) && !Number.isNaN(reciprocal) ? X86_INDEFINITE_F32 : product;
  }
  return accepted;
}

/** Reproduce accepted-edge averaging for classifier slots 2, 3 and 4. The
 * six-pixel window runs from center-2 through center+3 on each axis. */
export function measureClassifierEdges({ width, height, bgra }) {
  const samples = new Float32Array(24), profile = new Float32Array(5), sums = new Float32Array(5);
  let accepted = 0;
  for (let y = 2; y < height - 3; y++) {
    for (let x = 2; x < width - 3; x++) {
      for (let direction = 0; direction < 2; direction++) {
        const stride = direction === 0 ? 4 : width * 4;
        const start = (y * width + x) * 4 - stride * 2;
        for (let pixel = 0; pixel < 6; pixel++) {
          for (let channel = 0; channel < 4; channel++) {
            samples[pixel * 4 + channel] = byteToUnit[bgra[start + pixel * stride + channel]];
          }
        }
        if (sampleClassifierEdge(samples, f32(0.1), profile)) {
          accepted++;
          for (let index = 0; index < 5; index++) sums[index] += profile[index];
        }
      }
    }
  }
  if (accepted !== 0) {
    const reciprocal = f32(1 / f32(accepted));
    for (let index = 0; index < 5; index++) sums[index] *= reciprocal;
  }
  return Float32Array.of(f32(f32(sums[0] + sums[4]) * 0.5), f32(f32(sums[1] + sums[3]) * 0.5), sums[2]);
}

/** Internal region kernel for slots 5..13. The caller supplies the prepared
 * dispatch's dense labels and moments, before native cache release. Region
 * preparation, including shadow refinement, remains a separate stage. */
export function measureClassifierRegions({ width, height, bgra }, { indices, counts, sums }) {
  const mask = boundaryMask(width, height, indices);
  const means = new Float32Array(sums.length);
  for (let region = 0; region < counts.length; region++) {
    const reciprocal = f32(1 / f32(counts[region]));
    for (let channel = 0; channel < 4; channel++) means[region * 4 + channel] = sums[region * 4 + channel] * reciprocal;
  }
  let interiorSum = 0, boundarySum = 0, totalSum = 0, interiorCount = 0, boundaryCount = 0;
  const colors = new Set();
  for (let pixel = 0; pixel < indices.length; pixel++) {
    let error = 0;
    for (let channel = 0; channel < 4; channel++) {
      const residual = f32(byteToUnit[bgra[pixel * 4 + channel]] - means[indices[pixel] * 4 + channel]);
      error = f32(error + f32(residual * residual));
    }
    if (mask[pixel] === 0) { interiorSum = f32(interiorSum + error); interiorCount++; }
    else { boundarySum = f32(boundarySum + error); boundaryCount++; }
    totalSum = f32(totalSum + error);
    const offset = pixel * 4;
    colors.add((bgra[offset] | bgra[offset + 1] << 8 | bgra[offset + 2] << 16 | bgra[offset + 3] << 24) >>> 0);
  }
  const interiorMean = interiorCount === 0 ? interiorSum : f32(interiorSum / f32(interiorCount));
  const boundaryMean = boundaryCount === 0 ? boundarySum : f32(boundarySum / f32(boundaryCount));
  const meanSum = f32(boundaryMean + interiorMean), pixels = width * height;
  return Float32Array.of(meanSum > 0 ? f32(boundaryMean / meanSum) : 0, f32(totalSum / f32(pixels)),
    boundaryMean, interiorMean, f32(pixels), f32(counts.length), f32(f32(counts.length) / f32(pixels)),
    f32(counts.length / Math.sqrt(pixels)), f32(colors.size));
}

function histogramIndex(bgra, offset) {
  return (bgra[offset] >> 4) + ((bgra[offset + 1] >> 4) << 4) +
    ((bgra[offset + 2] >> 4) << 8) + ((bgra[offset + 3] >> 4) << 12);
}

function histogramWeight(histogram) {
  let weight = 0;
  for (const bin of histogram) weight += bin;
  return weight;
}

function clearPeakNeighborhood(histogram, peaks, offset) {
  const blue = peaks[offset] >> 4, green = peaks[offset + 1] >> 4;
  const red = peaks[offset + 2] >> 4, alpha = peaks[offset + 3] >> 4;
  for (let b = Math.max(0, blue - 1); b <= Math.min(15, blue + 1); b++) {
    for (let g = Math.max(0, green - 1); g <= Math.min(15, green + 1); g++) {
      for (let r = Math.max(0, red - 1); r <= Math.min(15, red + 1); r++) {
        for (let a = Math.max(0, alpha - 1); a <= Math.min(15, alpha + 1); a++) histogram[b + (g << 4) + (r << 8) + (a << 12)] = 0;
      }
    }
  }
}

function clearPeakLines(histogram, peaks) {
  const start = new Float32Array(4), difference = new Float32Array(4), sample = new Uint8Array(4);
  for (let first = 0; first < peaks.length; first += 4) {
    for (let channel = 0; channel < 4; channel++) start[channel] = byteToUnit[peaks[first + channel]];
    for (let second = first + 4; second < peaks.length; second += 4) {
      let squaredDistance = 0;
      for (let channel = 0; channel < 4; channel++) {
        difference[channel] = byteToUnit[peaks[second + channel]] - start[channel];
        squaredDistance += difference[channel] * difference[channel];
      }
      const intervals = 2 - Math.trunc(Math.sqrt(squaredDistance) * -16);
      for (let index = 0; index <= intervals; index++) {
        const fraction = f32(f32(index) / f32(intervals));
        for (let channel = 0; channel < 4; channel++) {
          const coordinate = f32(f32(difference[channel] * fraction) + start[channel]);
          sample[channel] = Math.trunc(coordinate * 255 + 0.5) & 255;
        }
        histogram[histogramIndex(sample, 0)] = 0;
      }
    }
  }
}

/** Internal weighted-color kernel for classifier slots 14..19. Explicit edge
 * controls belong to the prepared classification context, not GUI defaults. */
export function measureClassifierColors({ width, height, bgra }, { edgeScale, edgeFloor }) {
  const measurement = calculateMeasurement(width, height, bgra), pixels = width * height;
  const weights = Float64Array.from(measurement, value => Math.max(1 - value * edgeScale * (1 / 255), edgeFloor));
  const histogram = new Float64Array(65536);
  for (let pixel = 0; pixel < pixels; pixel++) histogram[histogramIndex(bgra, pixel * 4)] += weights[pixel];
  let peaks = detectHistogramPeaks(histogram, 16, true).bgra;
  const initialWeight = histogramWeight(histogram);
  for (let offset = 0; offset < peaks.length; offset += 4) histogram[histogramIndex(peaks, offset)] = 0;
  const binRemainder = histogramWeight(histogram), afterBins = histogram.slice();
  for (let offset = 0; offset < peaks.length; offset += 4) clearPeakNeighborhood(histogram, peaks, offset);
  const neighborhoodRemainder = histogramWeight(histogram);
  histogram.set(afterBins);
  clearPeakLines(histogram, peaks);
  const lineRemainder = histogramWeight(histogram);
  if (peaks.length === 0) peaks = new Uint8Array(4);
  const peakCount = Math.min(peaks.length / 4, 12), assignments = new Uint8Array(pixels);
  const sums = new Float32Array(peakCount * 4), counts = new Float32Array(peakCount), sample = new Float32Array(4);
  let totalWeight = 0;
  for (let pixel = 0; pixel < pixels; pixel++) {
    for (let channel = 0; channel < 4; channel++) sample[channel] = byteToUnit[bgra[pixel * 4 + channel]];
    // Original peak bytes are deliberately not normalized in this comparison.
    // Normalizing them changes assignments and therefore all residual moments.
    let selected = 0, minimum = Infinity;
    for (let peak = 0; peak < peakCount; peak++) {
      let distance = 0;
      for (let channel = 0; channel < 4; channel++) {
        const difference = f32(sample[channel] - peaks[peak * 4 + channel]);
        distance += difference * difference;
      }
      if (distance < minimum) { minimum = distance; selected = peak; }
    }
    assignments[pixel] = selected;
    for (let channel = 0; channel < 4; channel++) sums[selected * 4 + channel] += f32(sample[channel] * weights[pixel]);
    counts[selected] += weights[pixel];
    totalWeight = f32(totalWeight + weights[pixel]);
  }
  for (let peak = 0; peak < peakCount; peak++) {
    const reciprocal = f32(1 / counts[peak]);
    for (let channel = 0; channel < 4; channel++) sums[peak * 4 + channel] *= reciprocal;
  }
  let mean = f32(1e-6), fourth = 0;
  for (let pixel = 0; pixel < pixels; pixel++) {
    let residual = f32(byteToUnit[bgra[pixel * 4]] - sums[assignments[pixel] * 4]);
    for (let channel = 1; channel < 4; channel++) {
      residual = f32(residual + f32(byteToUnit[bgra[pixel * 4 + channel]] - sums[assignments[pixel] * 4 + channel]));
    }
    const square = f32(residual * residual), weight = f32(weights[pixel]);
    mean = f32(mean + f32(square * weight));
    fourth = f32(fourth + f32(f32(square * square) * weight));
  }
  const reciprocal = f32(1 / totalWeight);
  mean = f32(mean * reciprocal); fourth = f32(fourth * reciprocal);
  const weightReciprocal = 1 / initialWeight;
  return { measurement, features:Float32Array.of(f32(fourth / (mean * mean) - 3), mean, fourth,
    f32(binRemainder * weightReciprocal), f32(neighborhoodRemainder * weightReciprocal), f32(lineRemainder * weightReciprocal)) };
}
