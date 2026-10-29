import { readPaletteColors, readPaletteScoreInput, validateBgra } from './pixel-input.mjs';
import { collectBlendIndices, dotFloat, eightDirections, fitPreparedBlend, writeBlendSample } from './blend-solver.mjs';

const f32 = Math.fround;

function readBlendInput(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected a color-blend object.');
  const { bgra, colors } = input;
  validateBgra(bgra);
  if (bgra.length !== 4) throw new Error('Blend sample must contain one BGRA byte pixel.');
  const normalizedColors = readPaletteColors(colors);
  if (normalizedColors.length > 36) throw new Error('Blend colors must contain one through nine BGRA colors.');
  // Multi-color sampling retains B/G products in x87 registers, but stores
  // R/A through float32. The two-color SSE path rounds all four channels.
  const pixel = new Float64Array(4);
  writeBlendSample(bgra, 0, pixel);
  return { pixel, colors: normalizedColors };
}

/** Fit ordered local colors using the original blend model's precision rules. */
export function fitColorBlend(input) {
  const { pixel, colors } = readBlendInput(input);
  return fitPreparedBlend(pixel, colors);
}

/** Internal radius-two mask shared by palette scoring and image classification. */
export function boundaryMask(width, height, indices) {
  const mask = new Uint8Array(indices.length);
  for (let y = 0; y + 1 < height; y++) {
    for (let x = 0; x + 1 < width; x++) {
      const pixel = y * width + x;
      if (indices[pixel] !== indices[pixel + 1] || indices[pixel] !== indices[pixel + width] ||
          indices[pixel] !== indices[pixel + width + 1]) {
        mask[pixel] = mask[pixel + 1] = mask[pixel + width] = mask[pixel + width + 1] = 2;
      }
    }
  }
  for (let y = 0; y + 1 < height; y++) {
    for (let x = 0; x + 1 < width; x++) {
      const pixel = y * width + x;
      if (mask[pixel] > 1 || mask[pixel + 1] > 1 || mask[pixel + width] > 1 || mask[pixel + width + 1] > 1) {
        if (mask[pixel] === 0) mask[pixel] = 1;
        if (mask[pixel + 1] === 0) mask[pixel + 1] = 1;
        if (mask[pixel + width] === 0) mask[pixel + width] = 1;
        if (mask[pixel + width + 1] === 0) mask[pixel + width + 1] = 1;
      }
    }
  }
  return mask;
}

/** Score explicit anti-alias assignments using the original local blend fit. */
export function scoreWeightedPalette(input) {
  const { bgra, colors, indices, costPerColor } = readPaletteScoreInput(input);
  const { width, height } = input;
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 0 || height < 0 ||
      width > 2147483647 || height > 2147483647 || width * height > 2147483647 ||
      width * height * 4 !== bgra.length) {
    throw new Error('Weighted score needs nonnegative int32 dimensions with exactly width * height BGRA pixels.');
  }
  return scorePreparedWeightedPalette({ width, height, bgra, colors, indices, costPerColor });
}

/** Internal scoring kernel shared with candidate generation. Its caller owns
 * validation; colors must still contain their unquantized refinement values. */
export function scorePreparedWeightedPalette({ width, height, bgra, colors, indices, costPerColor }) {
  const mask = boundaryMask(width, height, indices);
  const sample = new Float64Array(4);
  const differences = new Float32Array(4);
  let sum = 0;
  for (let pixel = 0; pixel < indices.length; pixel++) {
    writeBlendSample(bgra, pixel * 4, sample);
    let predicted = colors;
    let predictedOffset = indices[pixel] * 4;
    if (mask[pixel] !== 0) {
      const selected = collectBlendIndices(indices, width, height, pixel, eightDirections);
      const localColors = new Float32Array(selected.length * 4);
      for (const [entry, color] of selected.entries()) localColors.set(colors.subarray(color * 4, color * 4 + 4), entry * 4);
      predicted = fitPreparedBlend(sample, localColors).color;
      predictedOffset = 0;
    }
    for (let channel = 0; channel < 4; channel++) differences[channel] = f32(predicted[predictedOffset + channel] - f32(sample[channel]));
    sum = f32(sum + dotFloat(differences, differences));
  }
  const error = f32(Math.log10(1 + sum));
  const colorCost = f32(f32(colors.length / 4 - 2) * f32(costPerColor));
  return { total: f32(error + colorCost), error, colors: colorCost };
}
