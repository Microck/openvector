/** Shared boundaries for prepared byte pixels and owned normalized colors. */
export function imageSize(width,height) {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1 ||
      width > 2147483647 || height > 2147483647 || width * height > 2147483647) {
    throw new Error('Decoded image dimensions must be positive int32 values with an int32 pixel count.');
  }
  return width * height;
}

export function validateBgra(bgra) {
  if ((!Array.isArray(bgra) && !(bgra instanceof Uint8Array)) || bgra.length % 4 !== 0) {
    throw new Error('BGRA pixels must contain four byte channels per pixel.');
  }
  if (Array.isArray(bgra)) {
    for (const channel of bgra) {
      if (!Number.isInteger(channel) || channel < 0 || channel > 255) {
        throw new Error('BGRA channels must be integers from 0 to 255.');
      }
    }
  }
}

/** Convert validated, owned CMYK byte planes to RGB in place. Encoded JPEG
 * and TIFF imports share this source-color boundary; opacity stays separate. */
export function normalizeCmykPixels(bgra,black) {
  for (let pixel=0;pixel<black.length;pixel++) {
    const light=255-black[pixel],offset=pixel*4;
    for (let channel=0;channel<3;channel++) bgra[offset+channel]=Math.floor(((255-bgra[offset+channel])*light+127)/255);
  }
}

export function readPaletteColors(palette) {
  if ((!Array.isArray(palette) && !(palette instanceof Float32Array)) ||
      palette.length === 0 || palette.length % 4 !== 0) {
    throw new Error('Palette must contain four normalized BGRA channels per color.');
  }
  for (const channel of palette) {
    if (!Number.isFinite(channel) || channel < 0 || channel > 1) {
      throw new Error('Palette channels must be finite numbers from 0 to 1.');
    }
  }
  return Float32Array.from(palette);
}

/** QColor converts validated float32 unit channels through 16-bit storage. */
export function quantizeQColorChannels(channels) {
  return Float32Array.from(channels,value => Math.round(value*65535)/65535);
}

export function readPaletteInput(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected a palette assignment object.');
  const { bgra, palette } = input;
  validateBgra(bgra);
  return { bgra, colors: readPaletteColors(palette) };
}

export function readPaletteScoreInput(input) {
  const { bgra, colors } = readPaletteInput(input);
  const { indices, costPerColor } = input;
  const colorCount = colors.length / 4;
  if ((!Array.isArray(indices) && !(indices instanceof Int32Array)) || indices.length !== bgra.length / 4) {
    throw new Error('Score needs one palette index per BGRA pixel.');
  }
  for (const index of indices) {
    if (!Number.isInteger(index) || index < 0 || index >= colorCount) {
      throw new Error('Score indices must identify existing palette colors.');
    }
  }
  if (!Number.isFinite(costPerColor) || !Number.isFinite(Math.fround(costPerColor))) {
    throw new Error('costPerColor must be a finite float32 number.');
  }
  return { bgra, colors, indices, costPerColor };
}
