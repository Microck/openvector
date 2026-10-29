/** GIF89a byte structure with original Q8 first-frame pixel behavior.
 * Keep intrinsic matte separate from alpha: the original casts its absent
 * transparency sentinel -1 to byte255 before comparing palette indices. */
export function decodeGif(encoded) {
  let offset = 6;
  const take = count => {
    if (offset + count > encoded.length) throw new Error('GIF data is incomplete.');
    const bytes = encoded.subarray(offset,offset + count);
    offset += count;
    return bytes;
  };
  const screen = take(7);
  let palette = screen[4] & 128 ? take(3 * (2 << (screen[4] & 7))) : null;
  let transparency = -1;
  const blocks = () => {
    const chunks = [];
    for (let count = take(1)[0]; count; count = take(1)[0]) chunks.push(take(count));
    return Buffer.concat(chunks);
  };
  while (offset < encoded.length) {
    const marker = take(1)[0];
    if (marker === 0x3b) break;
    if (marker === 0x21) {
      const label = take(1)[0], extension = blocks();
      if (label === 0xf9) {
        if (extension.length !== 4) throw new Error('GIF graphic control requires four bytes.');
        if (extension[0] & 1) transparency = extension[3];
      }
      continue;
    }
    if (marker !== 0x2c) throw new Error('GIF image descriptor is missing.');
    const descriptor = take(9);
    const width = descriptor[4] | descriptor[5] << 8, height = descriptor[6] | descriptor[7] << 8;
    if (!width || !height || width * height > 2147483647) throw new Error('GIF dimensions require a positive int32 pixel count.');
    const flags = descriptor[8];
    if (flags & 128) palette = take(3 * (2 << (flags & 7)));
    if (!palette) throw new Error('GIF requires an active color table.');
    if (transparency >= palette.length / 3) transparency = -1;
    const minimumBits = take(1)[0], compressed = blocks();
    const indices = decodeLzw(compressed,minimumBits,width * height);
    const bgra = new Uint8Array(width * height * 4), opacityIndex = transparency & 255;
    const starts = [0,4,2,1], steps = [8,8,4,2];
    let read = 0, y = 0, pass = 0;
    for (let row = 0; row < height; row++) {
      // The original advances one pass at a time, even when the next pass has
      // no rows. Heights2/3/4 therefore fail instead of skipping to pass4.
      if (y >= height) throw new Error('GIF interlace selects a row outside the image.');
      for (let x = 0; x < width; x++) {
        const index = indices[read++], color = index * 3, pixel = (y * width + x) * 4;
        if (color >= palette.length) throw new Error('GIF palette index is outside its color table.');
        bgra[pixel] = palette[color + 2]; bgra[pixel + 1] = palette[color + 1];
        bgra[pixel + 2] = palette[color]; bgra[pixel + 3] = index === opacityIndex ? 0 : 255;
      }
      if (flags & 64) {
        y += steps[pass];
        if (y >= height && pass < 3) y = starts[++pass];
      } else y++;
    }
    return {width,height,bgra,hasAlpha:transparency >= 0,format:'gif'};
  }
  throw new Error('GIF contains no image frame.');
}

/** The bounded phrase dictionary keeps LZW expansion separate from palette
 * import and row placement, including deferred clear at the 4096-code limit. */
function decodeLzw(compressed, minimumBits, count) {
  if (minimumBits < 2 || minimumBits > 8) throw new Error('GIF LZW minimum code size must be from 2 through 8.');
  const clear = 1 << minimumBits, end = clear + 1, pixels = new Uint8Array(count);
  const dictionary = new Array(4096);
  for (let index = 0; index < clear; index++) dictionary[index] = Uint8Array.of(index);
  let bits = minimumBits + 1, next = clear + 2, bit = 0, written = 0, previous = null;
  while (written < count) {
    if (bit + bits > compressed.length * 8) throw new Error('GIF LZW pixels are incomplete.');
    let code = 0;
    for (let shift = 0; shift < bits; shift++,bit++) code |= ((compressed[bit >>> 3] >>> (bit & 7)) & 1) << shift;
    if (code === clear) { bits = minimumBits + 1; next = clear + 2; previous = null; continue; }
    if (code === end) throw new Error('GIF LZW stream ended before all pixels were decoded.');
    let entry;
    if (code < next && dictionary[code]) entry = dictionary[code];
    else if (code === next && previous) {
      entry = new Uint8Array(previous.length + 1); entry.set(previous); entry[previous.length] = previous[0];
    } else throw new Error('GIF LZW code is outside the current dictionary.');
    if (entry.length > count - written) throw new Error('GIF LZW code exceeds the image pixel count.');
    pixels.set(entry,written); written += entry.length;
    if (previous && next < 4096) {
      const phrase = new Uint8Array(previous.length + 1); phrase.set(previous); phrase[previous.length] = entry[0];
      dictionary[next++] = phrase;
      if (next === 1 << bits && bits < 12) bits++;
    }
    previous = entry;
  }
  return pixels;
}
