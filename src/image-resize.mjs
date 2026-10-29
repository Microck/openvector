const f32 = Math.fround;

function triangleWeights(sourceSize, targetSize) {
  const factor = f32(targetSize / sourceSize), inverse = f32(1 / factor);
  const support = Math.max(inverse, 1), scale = f32(1 / support);
  return Array.from({ length:targetSize }, (_, output) => {
    const centerDouble = (output + 0.5) * inverse, center = f32(centerDouble);
    // The lower bound uses the retained double center, while the upper bound
    // and tap distances read its float32 store. Keep the original epsilon.
    const start = Math.trunc(Math.max(centerDouble - support - 1e-6, 0) + 0.5);
    const end = Math.trunc(Math.min(center + support, sourceSize) + 0.5);
    const taps = [];
    let sum = 0;
    for (let pixel = start; pixel < end; pixel++) {
      const distance = f32(((pixel - center) + 0.5) * scale);
      const weight = f32(Math.max(0, 1 - Math.abs(distance)));
      taps.push([pixel, weight]);
      sum = f32(sum + weight);
    }
    if (sum !== 0 && sum !== 1) {
      const reciprocal = f32(1 / sum);
      for (const tap of taps) tap[1] = f32(tap[1] * reciprocal);
    }
    return taps;
  });
}

function trianglePass({ width, height, bgra }, targetSize, horizontal, matte) {
  const outputWidth = horizontal ? targetSize : width, outputHeight = horizontal ? height : targetSize;
  const resized = new Uint8Array(outputWidth * outputHeight * 4);
  const weights = triangleWeights(horizontal ? width : height, targetSize);
  const sums = new Float64Array(3);
  for (let y = 0; y < outputHeight; y++) {
    for (let x = 0; x < outputWidth; x++) {
      sums.fill(0);
      const taps = weights[horizontal ? x : y], groupedTaps = taps.length - taps.length % 4;
      let alpha = 0, firstAlpha = 0, opacitySum = 0;
      for (let tap = 0; tap < taps.length; tap++) {
        const [coordinate, weight] = taps[tap];
        const pixel = (horizontal ? y * width + coordinate : coordinate * width + x) * 4;
        const opacity = 255 - bgra[pixel + 3];
        const effectiveWeight = matte ? f32((255 - opacity) * weight * (1 / 255)) : weight;
        if (matte) {
          for (let channel = 0; channel < 3; channel++) sums[channel] = f32(sums[channel] + bgra[pixel + channel] * effectiveWeight);
        } else {
          for (let channel = 0; channel < 3; channel++) sums[channel] += bgra[pixel + channel] * weight;
          // In the non-matte loop red remains in x87 across the fourth and
          // trailing taps; green spills after the third and fourth taps.
          if (tap < groupedTaps && (tap % 4 === 1 || tap % 4 === 2)) sums[2] = f32(sums[2]);
          if (tap >= groupedTaps || tap % 4 === 2 || tap % 4 === 3) sums[1] = f32(sums[1]);
          if (tap >= groupedTaps || tap % 4 !== 1) sums[0] = f32(sums[0]);
        }
        opacitySum += opacity * weight;
        // Matte opacity spills after taps one and four in each unrolled
        // group. The middle two additions retain double precision.
        if (!matte || tap >= groupedTaps || tap % 4 === 0 || tap % 4 === 3) opacitySum = f32(opacitySum);
        // The original unrolled loop stores alpha after its second and third
        // taps, then retains the fourth addition in x87 double precision. Its
        // trailing one to three taps also accumulate without float32 stores.
        if (tap < groupedTaps) {
          if (tap % 4 === 0) firstAlpha = effectiveWeight;
          else if (tap % 4 === 1) alpha = f32(horizontal ? alpha + firstAlpha + effectiveWeight : effectiveWeight + alpha + firstAlpha);
          else if (tap % 4 === 2) alpha = f32(alpha + effectiveWeight);
          else alpha += effectiveWeight;
        } else alpha += effectiveWeight;
      }
      const reciprocal = matte && Math.abs(alpha) > 1e-6 ? 1 / alpha : 1;
      for (let channel = 0; channel < 4; channel++) {
        const value = channel === 3 ? opacitySum : sums[channel] * reciprocal;
        const byte = Math.min(255, Math.max(0, Math.trunc(value + 0.5)));
        resized[(y * outputWidth + x) * 4 + channel] = channel === 3 ? 255 - byte : byte;
      }
    }
  }
  return { width:outputWidth, height:outputHeight, bgra:resized };
}

function sampleImage({ width, height, bgra, targetWidth, targetHeight }) {
  const sampled = new Uint8Array(targetWidth * targetHeight * 4);
  for (let y = 0; y < targetHeight; y++) {
    const row = Math.trunc(((y + 0.5) * height) / targetHeight);
    for (let x = 0; x < targetWidth; x++) {
      const column = Math.trunc(((x + 0.5) * width) / targetWidth);
      const source = (row * width + column) * 4, target = (y * targetWidth + x) * 4;
      for (let channel = 0; channel < 4; channel++) sampled[target + channel] = bgra[source + channel];
    }
  }
  return { width:targetWidth, height:targetHeight, bgra:sampled };
}

/** Private Q8 resize over validated prepared BGRA, positive dimensions and
 * explicit native filter/matte controls. Both paths own output bytes. Native
 * filtered resizing quantizes between passes, including same-size resizes. */
export function resizePreparedImage(input) {
  if (!input.filtered) return sampleImage(input);
  const { width, height, targetWidth, targetHeight, matte } = input;
  if ((width + targetWidth) * targetHeight < (height + targetHeight) * targetWidth) {
    const horizontal = trianglePass(input, targetWidth, true, matte);
    return trianglePass(horizontal, targetHeight, false, matte);
  }
  const vertical = trianglePass(input, targetHeight, false, matte);
  return trianglePass(vertical, targetWidth, true, matte);
}

/** Private native Geometry fit. Its string producer omits zero dimensions;
 * one-axis requests convert the requested integer to float32 before division,
 * while two-axis requests retain a double width ratio and float32 height ratio. */
export function fitResizeGeometry({ width, height, geometryWidth, geometryHeight }) {
  if ((!geometryWidth || geometryWidth === width) && (!geometryHeight || geometryHeight === height)) return [width, height];
  const scale = geometryWidth && geometryHeight ? Math.min(geometryWidth / width, f32(geometryHeight / height))
    : geometryWidth ? f32(geometryWidth) / width : f32(geometryHeight) / height;
  return [Math.max(1, Math.trunc(width * scale + 0.5)), Math.max(1, Math.trunc(height * scale + 0.5))];
}

/** Private working-size calculation. Math.imul preserves the original signed
 * int32 area comparison, rather than silently changing its overflow behavior. */
export function workingImageDimensions({ width, height }) {
  const area = Math.imul(width, height);
  if (area <= 200000) return [width, height];
  const scale = Math.sqrt(150000 / area);
  return fitResizeGeometry({ width, height, geometryWidth:Math.trunc(width * scale + 0.5),
    geometryHeight:Math.trunc(height * scale + 0.5) });
}

/** Private reduction of an already preprocessed raster, including empty
 * preprocessing outcomes. Native source cache ownership belongs to the image
 * workflow; this function produces only its owned working raster. */
export function reduceWorkingImage(input) {
  const [targetWidth, targetHeight] = workingImageDimensions(input);
  if (Math.imul(input.width, input.height) <= 200000) {
    return { width:input.width, height:input.height, bgra:Uint8Array.from(input.bgra) };
  }
  return resizePreparedImage({ ...input, targetWidth, targetHeight, filtered:input.antiAlias });
}
