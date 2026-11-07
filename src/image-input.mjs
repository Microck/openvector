import { inflateSync } from 'node:zlib';
import { decodeJpeg } from './jpeg-input.mjs';
import { decodeGif } from './gif-input.mjs';
import { decodeTiff } from './tiff-input.mjs';
import { decodeBmp } from './bmp-input.mjs';
import { imageSize } from './pixel-input.mjs';

const pngSignature = Buffer.from([137,80,78,71,13,10,26,10]);
const adam7 = [[0,0,8,8],[4,0,8,8],[0,4,4,8],[2,0,4,4],[0,2,2,4],[1,0,2,2],[0,1,1,2]];
const crcTable = Uint32Array.from({ length:256 }, (_,byte) => {
  let value = byte;
  for (let bit = 0; bit < 8; bit++) value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  return value >>> 0;
});

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = crcTable[(crc ^ byte) & 255] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function sampleAt(row, index, depth) {
  if (depth === 8) return row[index];
  if (depth === 16) return row[index * 2] * 256 + row[index * 2 + 1];
  const bit = index * depth;
  return (row[bit >>> 3] >>> (8 - depth - bit % 8)) & ((1 << depth) - 1);
}

function unfilterRow(encoded, offset, row, previous, bpp) {
  const filter = encoded[offset++];
  if (filter > 4) throw new Error('PNG row filter must be from 0 through 4.');
  for (let index = 0; index < row.length; index++) {
    const left = index >= bpp ? row[index - bpp] : 0;
    const above = previous[index], upperLeft = index >= bpp ? previous[index - bpp] : 0;
    let predictor = 0;
    if (filter === 1) predictor = left;
    else if (filter === 2) predictor = above;
    else if (filter === 3) predictor = (left + above) >>> 1;
    else if (filter === 4) {
      const estimate = left + above - upperLeft;
      const a = Math.abs(estimate - left), b = Math.abs(estimate - above), c = Math.abs(estimate - upperLeft);
      predictor = a <= b && a <= c ? left : b <= c ? above : upperLeft;
    }
    row[index] = encoded[offset + index] + predictor;
  }
  return offset + row.length;
}

/** Preserve noninterlaced Q8 row reduction and opacity quirks. Interlaced
 * 16-bit RGB/RGBA uses sample-domain decoding below instead of this import. */
function importLegacyPngRows(storage, bgra, width, height, type, transparency) {
  const channels = type === 2 ? 3 : 4, stride = width * channels * 2;
  const keys = transparency ? [0,2,4].map(offset => transparency.readUInt16BE(offset) & 255) : null;
  for (let y = 0; y < height; y++) {
    const row = storage.subarray(y * stride,(y + 1) * stride);
    if (type === 6) {
      for (let index = 0; index < width * 4; index++) row[index] = row[index * 2];
    } else {
      let read = 0, write = 0;
      for (let x = 0; x < width; x++) {
        row[write++] = row[read]; read += 2;
        row[write++] = row[read]; read += 2;
        row[write++] = row[read]; read += 2;
        // The native cheap-transparency comparison reads this same, already
        // modified row and truncates the original key samples to Quantum Q8.
        const transparent = keys && row[read - 6] * 256 + row[read - 5] === keys[0] &&
          row[read - 4] * 256 + row[read - 3] === keys[1] && row[read - 2] * 256 + row[read - 1] === keys[2];
        row[write++] = transparent ? 255 : 0;
      }
    }
    for (let x = 0; x < width; x++) {
      const source = x * 4, target = (y * width + x) * 4;
      bgra[target] = row[source + 2]; bgra[target + 1] = row[source + 1];
      bgra[target + 2] = row[source]; bgra[target + 3] = row[source + 3];
    }
  }
}

function decodePng(encoded) {
  let width, height, depth, type, interlace, channels, palette, transparency, ended = false;
  const compressed = [];
  for (let offset = 8; offset < encoded.length;) {
    if (offset + 12 > encoded.length) throw new Error('PNG chunk header is incomplete.');
    const length = encoded.readUInt32BE(offset), end = offset + 8 + length;
    if (end + 4 > encoded.length) throw new Error('PNG chunk payload is incomplete.');
    const kind = encoded.toString('ascii',offset + 4,offset + 8), payload = encoded.subarray(offset + 8,end);
    if (crc32(encoded.subarray(offset + 4,end)) !== encoded.readUInt32BE(end)) throw new Error(`PNG ${kind} chunk CRC does not match.`);
    if (kind === 'IHDR') {
      if (width !== undefined || offset !== 8 || length !== 13) throw new Error('PNG requires one initial 13-byte IHDR.');
      width = payload.readUInt32BE(0); height = payload.readUInt32BE(4);
      depth = payload[8]; type = payload[9]; interlace = payload[12];
      channels = ({ 0:1,2:3,3:1,4:2,6:4 })[type];
      const validDepth = type === 0 ? [1,2,4,8,16] : type === 3 ? [1,2,4,8] : [8,16];
      if (!channels || !validDepth.includes(depth) || payload[10] !== 0 || payload[11] !== 0 || interlace > 1) {
        throw new Error('Unsupported PNG color, depth, compression, filter or interlace method.');
      }
      imageSize(width,height);
    } else if (width === undefined) throw new Error('PNG image header is missing.');
    else if (kind === 'PLTE') palette = payload;
    else if (kind === 'tRNS') transparency = payload;
    else if (kind === 'IDAT') compressed.push(payload);
    else if (kind === 'IEND') {
      if (length !== 0) throw new Error('PNG IEND must be empty.');
      ended = true; break;
    } else if ((encoded[offset + 4] & 32) === 0) throw new Error(`Unsupported PNG critical chunk ${kind}.`);
    offset = end + 4;
  }
  if (!ended || compressed.length === 0) throw new Error('PNG image data or trailer is missing.');
  if (type === 3 && (!palette || palette.length === 0 || palette.length % 3 || palette.length > 768)) {
    throw new Error('Indexed PNG requires a valid RGB palette.');
  }
  if (transparency && ((type === 0 && transparency.length !== 2) || (type === 2 && transparency.length !== 6))) {
    throw new Error('PNG transparency key has the wrong size.');
  }
  const passes = [];
  let expected = 0;
  for (const [x,y,dx,dy] of interlace ? adam7 : [[0,0,1,1]]) {
    const passWidth = Math.max(0,Math.ceil((width - x) / dx)), passHeight = Math.max(0,Math.ceil((height - y) / dy));
    const rowBytes = Math.ceil(passWidth * channels * depth / 8);
    passes.push({ x,y,dx,dy,passWidth,passHeight,rowBytes });
    if (passWidth && passHeight) expected += passHeight * (rowBytes + 1);
  }
  const inflated = inflateSync(Buffer.concat(compressed), { maxOutputLength:expected });
  if (inflated.length !== expected) throw new Error('PNG decoded scanline size does not match its dimensions.');
  const bgra = new Uint8Array(width * height * 4), bpp = Math.max(1,Math.ceil(channels * depth / 8));
  // Reduce Adam7 samples once, at their final pixel positions, rather than
  // reimporting retained rows and corrupting pixels supplied by earlier passes.
  const correctAdam7 = interlace === 1 && depth === 16 && (type === 2 || type === 6);
  const legacyRows = depth === 16 && (type === 2 || type === 6) && !correctAdam7 ? new Uint8Array(width * height * channels * 2) : null;
  const scale = depth === 16 ? value => value >>> 8 : depth === 8 ? value => value : value => value * (255 / ((1 << depth) - 1));
  let offset = 0;
  for (const pass of passes) {
    let previous = new Uint8Array(pass.rowBytes), row = new Uint8Array(pass.rowBytes);
    for (let y = 0; pass.passWidth && y < pass.passHeight; y++) {
      offset = unfilterRow(inflated,offset,row,previous,bpp);
      for (let x = 0; x < pass.passWidth; x++) {
        const sample = x * channels;
        if (legacyRows) {
          const target = ((pass.y + y * pass.dy) * width + pass.x + x * pass.dx) * channels * 2;
          for (let channelByte = 0; channelByte < channels * 2; channelByte++) legacyRows[target + channelByte] = row[sample * 2 + channelByte];
          continue;
        }
        let red, green, blue, alpha = 255;
        if (type === 3) {
          const color = sampleAt(row,sample,depth);
          if (color * 3 + 2 >= palette.length) throw new Error('PNG palette index is outside its color table.');
          red = palette[color * 3]; green = palette[color * 3 + 1]; blue = palette[color * 3 + 2];
          alpha = transparency && color < transparency.length ? transparency[color] : 255;
        } else if (type === 0 || type === 4) {
          const gray = sampleAt(row,sample,depth);
          red = green = blue = scale(gray);
          if (type === 4) alpha = scale(sampleAt(row,sample + 1,depth));
        } else {
          const r = sampleAt(row,sample,depth), g = sampleAt(row,sample + 1,depth), b = sampleAt(row,sample + 2,depth);
          red = scale(r); green = scale(g); blue = scale(b);
          if (type === 6) alpha = scale(sampleAt(row,sample + 3,depth));
          else if (correctAdam7 && transparency && r === transparency.readUInt16BE(0)
            && g === transparency.readUInt16BE(2) && b === transparency.readUInt16BE(4)) alpha = 0;
        }
        const pixel = ((pass.y + y * pass.dy) * width + pass.x + x * pass.dx) * 4;
        bgra[pixel] = blue; bgra[pixel + 1] = green; bgra[pixel + 2] = red; bgra[pixel + 3] = alpha;
      }
      [row,previous] = [previous,row];
    }
    if (legacyRows) importLegacyPngRows(legacyRows,bgra,width,height,type,transparency);
  }
  if (!correctAdam7 && transparency && (type === 0 || type === 2)) {
    const keys = type === 0 ? Array(3).fill(depth < 8 ? transparency.readUInt16BE(0) * 255 / ((1 << depth) - 1)
      : transparency.readUInt16BE(0) & 255) : [0,2,4].map(offset => transparency.readUInt16BE(offset) & 255);
    for (let y = 0; y < height; y++) {
      // The original final tRNS scan resets opacity only at each row's first
      // pixel; other pixels keep the preceding quantum import's alpha value.
      bgra[(y * width) * 4 + 3] = 255;
      for (let x = 0; x < width; x++) {
        const pixel = (y * width + x) * 4;
        if (bgra[pixel + 2] === keys[0] && bgra[pixel + 1] === keys[1] && bgra[pixel] === keys[2]) bgra[pixel + 3] = 0;
      }
    }
  }
  return { width,height,bgra,hasAlpha:type === 4 || type === 6 || transparency !== undefined,format:'png' };
}

/** Decode supported original image encodings into owned RGB BGRA bytes.
 * Structural parsing follows the formats; original Q8 byte behavior governs
 * sample reduction and legacy alpha handling. Other codecs fail explicitly. */
export function decodeImage(input) {
  if (!(input instanceof Uint8Array)) throw new Error('Encoded image data must be a Uint8Array.');
  const encoded = Buffer.from(input.buffer,input.byteOffset,input.byteLength);
  if (encoded.length >= 8 && encoded.subarray(0,8).equals(pngSignature)) return decodePng(encoded);
  if (encoded.length >= 2 && ['BM','BA','CI'].includes(encoded.toString('ascii',0,2))) return decodeBmp(encoded);
  if (encoded.length >= 2 && encoded[0] === 0xff && encoded[1] === 0xd8) return decodeJpeg(encoded);
  if (encoded.length >= 6 && ['GIF87a','GIF89a'].includes(encoded.toString('latin1',0,6))) return decodeGif(encoded);
  if (encoded.length >= 4 && ['II*\x00','MM\x00*','II+\x00','MM\x00+'].includes(encoded.toString('latin1',0,4))) return decodeTiff(encoded);
  throw new Error('Unsupported image format. PNG, BMP, JPEG, GIF and TIFF are currently implemented.');
}


/** Native source-name metadata, distinct from the encoding detected in bytes.
 * Its parser prefers / whenever present, then considers backslashes. Preserve
 * case, dotfiles and mixed-separator behavior rather than normalizing a path. */
export function resolveImageFilename(filename) {
  if (typeof filename !== 'string') throw new TypeError('Image filename must be a string.');
  let separator = filename.lastIndexOf('/');
  if (separator < 0) separator = filename.lastIndexOf('\\');
  const dot = filename.lastIndexOf('.');
  let extension = dot >= 0 && dot >= separator ? filename.slice(dot) : '';
  if (extension === '.jpg') extension = '.jpeg';
  else if (extension === '.tif') extension = '.tiff';
  const shadowControl = extension === '.gif' ? 0 : extension === '.jpeg' ? 1 : extension === '.png' ? 2 : -1;
  return {extension,shadowControl};
}
