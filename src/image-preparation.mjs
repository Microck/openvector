import { premultiplyClassifierPixels } from './image-classification.mjs';

const enhanceWeights = [5,8,10,8,5,8,20,40,20,8,10,40,80,40,10,8,20,40,20,8,5,8,10,8,5];
const byteReciprocal = 1 / 255;

/** Private document composition over the original alpha-prepared backup.
 * Background components follow engine BGRA order and are quantized before
 * blending. The native operation does not premultiply background RGB by its
 * own alpha. Every arithmetic spill below matches the original SSE path. */
export function compositeSourceImage({ width, height, bgra }, { enabled, color }) {
  if (!enabled) return { width, height, bgra:Uint8Array.from(bgra) };
  const composed = new Uint8Array(bgra.length);
  const f32 = Math.fround, scale = f32(byteReciprocal);
  const backgroundUnits = Float32Array.from(color, value => (Math.trunc(f32(value) * 255 + 0.5) & 255) * scale);
  for (let pixel = 0; pixel < bgra.length; pixel += 4) {
    const remaining = f32(1 - f32(bgra[pixel + 3] * scale));
    for (let channel = 0; channel < 4; channel++) {
      const sourceUnit = f32(bgra[pixel + channel] * scale);
      const value = f32(f32(backgroundUnits[channel] * remaining) + sourceUnit);
      composed[pixel + channel] = Math.trunc(value * 255 + 0.5) & 255;
    }
  }
  return { width, height, bgra:composed };
}

/** Private EnhanceImage kernel over validated, already prepared Q8 BGRA.
 * ImageMagick stores opacity rather than alpha. Its opacity-dependent blue
 * distance formula must therefore be evaluated before converting the output
 * back to BGRA. The original returns an empty image for dimensions below 5. */
export function enhancePreparedImage({ width, height, bgra }) {
  if (width < 5 || height < 5) return { width:0, height:0, bgra:new Uint8Array() };
  const enhanced = new Uint8Array(bgra.length), sums = new Float32Array(4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const center = (y * width + x) * 4;
      const blue = bgra[center], green = bgra[center + 1], red = bgra[center + 2], opacity = 255 - bgra[center + 3];
      sums.fill(0);
      let count = 0, tap = 0;
      for (let dy = -2; dy <= 2; dy++) {
        const row = Math.min(height - 1, Math.max(0, y + dy));
        for (let dx = -2; dx <= 2; dx++) {
          const column = Math.min(width - 1, Math.max(0, x + dx));
          const neighbor = (row * width + column) * 4, weight = enhanceWeights[tap++];
          const neighborBlue = bgra[neighbor], neighborGreen = bgra[neighbor + 1];
          const neighborRed = bgra[neighbor + 2], neighborOpacity = 255 - bgra[neighbor + 3];
          const dr = red - neighborRed, dg = green - neighborGreen;
          const db = blue - neighborBlue, dopacity = opacity - neighborOpacity;
          // Keep the native x87 double operation order at the strict cutoff.
          let distance = (((red + neighborRed) * 0.5 + 512) * dr) * dr * byteReciprocal;
          distance += dg * dg * 4;
          distance += ((767 - (blue + neighborBlue) * 0.5) * db) * db * byteReciprocal;
          distance += ((767 - (opacity + neighborOpacity) * 0.5) * dopacity) * dopacity * byteReciprocal;
          if (distance < 2601) {
            sums[0] += neighborBlue * weight;
            sums[1] += neighborGreen * weight;
            sums[2] += neighborRed * weight;
            sums[3] += neighborOpacity * weight;
            count += weight;
          }
        }
      }
      // This is the original biased integer normalization, not Math.round.
      const halfCount = Math.fround(count * 0.5), reciprocal = 1 / count;
      for (let channel = 0; channel < 4; channel++) {
        const value = Math.trunc((sums[channel] + halfCount - 1) * reciprocal);
        enhanced[center + channel] = channel === 3 ? 255 - value : value;
      }
    }
  }
  return { width, height, bgra:enhanced };
}

/** Private mode-one preparation for decoded BGRA and an explicit amount 0..3.
 * Initial alpha multiplication occurs once; each repeated pass consumes the
 * previous owned output. The zero-pass case also owns its bytes. */
export function prepareEnhancedImage({ width, height, bgra, amount }) {
  return filterPreparedImage({ width,height,bgra:premultiplyClassifierPixels({ bgra }),mode:1,amount });
}

/** Private nonpeak median over validated prepared bytes and amount 0..3.
 * The original radius-zero Gaussian-width query selects the same 3x3 kernel
 * as radius one. Median selection avoids a unique-value minimum or maximum
 * when another value exists. Reversing odd samples from alpha to opacity and
 * back leaves this selection unchanged, so alpha can be processed directly. */
export function reducePreparedNoise({ width, height, bgra, amount }) {
  const radius = Math.max(1, amount), kernelWidth = radius * 2 + 1;
  if (width < kernelWidth || height < kernelWidth) throw new RangeError('ImageSmallerThanKernelRadius');
  const sampleCount = kernelWidth * kernelWidth, medianIndex = Math.floor(sampleCount / 2);
  const reduced = new Uint8Array(bgra.length);
  const channelSamples = Array.from({ length:4 }, () => new Uint8Array(sampleCount));
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      let sample = 0;
      // Gather every channel in one neighborhood pass. The four independent
      // sort buffers are reused after all their samples have been replaced.
      for (let dy = -radius; dy <= radius; dy++) {
        const row = Math.min(height - 1, Math.max(0, y + dy));
        for (let dx = -radius; dx <= radius; dx++) {
          const column = Math.min(width - 1, Math.max(0, x + dx)), neighbor = (row * width + column) * 4;
          for (let channel = 0; channel < 4; channel++) channelSamples[channel][sample] = bgra[neighbor + channel];
          sample++;
        }
      }
      for (let channel = 0; channel < 4; channel++) {
        const samples = channelSamples[channel];
        samples.sort();
        let selected = samples[medianIndex];
        if (selected === samples[0]) {
          let next = medianIndex + 1;
          while (next < samples.length && samples[next] === selected) next++;
          if (next < samples.length) selected = samples[next];
        } else if (selected === samples[samples.length - 1]) {
          let previous = medianIndex - 1;
          while (samples[previous] === selected) previous--;
          selected = samples[previous];
        }
        reduced[(y * width + x) * 4 + channel] = selected;
      }
    }
  }
  return { width, height, bgra:reduced };
}

function hullPlane(plane, scratch, width, height, delta, polarity) {
  const stride = width + 2, positive = polarity > 0;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const pixel = (y + 1) * stride + x + 1, center = plane[pixel], neighbor = plane[pixel + delta];
      const change = positive ? neighbor >= center + 2 : neighbor <= center - 2;
      scratch[pixel] = change ? center + polarity : center;
    }
  }
  // Both neighbors must come from the completed scratch pass, never partially
  // updated source pixels. The one-byte exterior remains zero in both planes.
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const pixel = (y + 1) * stride + x + 1, center = scratch[pixel];
      const backward = scratch[pixel - delta], forward = scratch[pixel + delta];
      const change = positive ? backward >= center + 2 && forward > center : backward <= center - 2 && forward < center;
      plane[pixel] = change ? center + polarity : center;
    }
  }
}

/** Private hull despeckle over validated Q8 BGRA, including one-dimensional
 * rasters. Every channel fills the whole interior and each hull overwrites
 * the whole scratch interior, so the two zero-bordered planes can be reused. */
export function despecklePreparedImage({ width, height, bgra }) {
  const stride = width + 2, plane = new Uint8Array(stride * (height + 2)), scratch = new Uint8Array(plane.length);
  const despeckled = new Uint8Array(bgra.length);
  for (const channel of [2,1,0,3]) {
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const value = bgra[(y * width + x) * 4 + channel];
        plane[(y + 1) * stride + x + 1] = channel === 3 ? 255 - value : value;
      }
    }
    for (const delta of [stride,1,stride + 1,stride - 1]) {
      hullPlane(plane, scratch, width, height, delta, 1);
      hullPlane(plane, scratch, width, height, -delta, 1);
      hullPlane(plane, scratch, width, height, -delta, -1);
      hullPlane(plane, scratch, width, height, delta, -1);
    }
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const value = plane[(y + 1) * stride + x + 1];
        despeckled[(y * width + x) * 4 + channel] = channel === 3 ? 255 - value : value;
      }
    }
  }
  return { width, height, bgra:despeckled };
}

/** Private decoded-pixel preprocessing with explicit native mode/amount.
 * Keep one initial-alpha path and preserve each filter's own empty/error
 * outcome. File decoding, working resize and image cache ownership are separate. */
export function prepareImagePixels(input) {
  const { width, height, bgra, mode, amount } = input;
  return filterPreparedImage({ width,height,bgra:premultiplyClassifierPixels({ bgra }),mode,amount });
}

/** Filter an owned, already alpha-prepared source. Document composition must
 * enter here so it never multiplies alpha a second time. */
export function filterPreparedImage({ width, height, bgra, mode, amount }) {
  let prepared = { width, height, bgra };
  if (mode === 0) return prepared;
  if (mode === 1) {
    for (let pass = 0; pass < amount; pass++) prepared = enhancePreparedImage(prepared);
    return prepared;
  }
  if (mode === 2) return reducePreparedNoise({ ...prepared, amount });
  if (mode === 3) return despecklePreparedImage(prepared);
  throw new RangeError('Expected an image preprocessing mode from 0 through 3.');
}
