import {imageSize,validateBgra} from './pixel-input.mjs';
import {readFileSync} from 'node:fs';
import {crc32} from 'node:zlib';

function readBitmapPixels({width,height,bgra}) {
  const pixels=imageSize(width,height);
  validateBgra(bgra);
  if (bgra.length!==pixels*4) throw new Error('Bitmap encoding needs one BGRA pixel for each image position.');
  return {width,height,bgra,pixels};
}

let jpegCompressor;
/** Default native JPEG profile: quality 75, baseline RGB, 96-DPI JFIF.
 * Alpha is ignored; document storage/conversion policy belongs to bitmap-export. */
export function encodeJpeg(input) {
  const {width,height,bgra}=readBitmapPixels(input);
  if (width>65500 || height>65500) throw new Error('JPEG dimensions cannot exceed 65500 pixels.');
  jpegCompressor??=new WebAssembly.Instance(new WebAssembly.Module(
    readFileSync(new URL('./bitmap-jpeg.wasm',import.meta.url)))).exports;
  jpegCompressor.ov_reset();
  const source=jpegCompressor.malloc(bgra.length);
  if (!source) throw new Error('JPEG compression exceeds the available WebAssembly memory.');
  new Uint8Array(jpegCompressor.memory.buffer,source,bgra.length).set(bgra);
  let length;
  try {length=jpegCompressor.ov_jpeg_encode(source,width,height);}
  catch (error) {
    if (!(error instanceof WebAssembly.RuntimeError)) throw error;
    throw new Error(`JPEG compression failed with IJG status ${jpegCompressor.ov_jpeg_error()}.`,{cause:error});
  }
  return Buffer.from(new Uint8Array(jpegCompressor.memory.buffer,jpegCompressor.ov_jpeg_output(),length));
}

/** Store supplied BGR channels and discard alpha; bitmap-export owns storage policy. */
export function encodeBmp(input) {
  const {width,height,bgra}=readBitmapPixels(input),stride=Math.ceil(width*3/4)*4;
  const pixelBytes=stride*height,length=54+pixelBytes;
  if (length>0xffffffff) throw new Error('BMP output exceeds its 32-bit file size.');
  const encoded=Buffer.alloc(length);
  encoded.write('BM');encoded.writeUInt32LE(length,2);encoded.writeUInt32LE(54,10);
  encoded.writeUInt32LE(40,14);encoded.writeInt32LE(width,18);encoded.writeInt32LE(height,22);
  encoded.writeUInt16LE(1,26);encoded.writeUInt16LE(24,28);encoded.writeUInt32LE(pixelBytes,34);
  encoded.writeInt32LE(3780,38);encoded.writeInt32LE(3780,42);
  for (let y=0;y<height;y++) {
    let target=54+(height-1-y)*stride,source=y*width*4;
    for (let x=0;x<width;x++,source+=4,target+=3) {
      encoded[target]=bgra[source];encoded[target+1]=bgra[source+1];encoded[target+2]=bgra[source+2];
    }
  }
  return encoded;
}

/** Encode supplied channel bytes; scene composition belongs to the renderer. */
export function encodeTiff(input) {
  const {width,height,bgra,pixels}=readBitmapPixels(input),pixelBytes=pixels*4;
  // Nine entries, the entry count and the zero next-directory pointer.
  const directoryBytes=2+9*12+4;
  const directory=8+pixelBytes,bitsOffset=directory+directoryBytes,length=bitsOffset+8;
  if (length>0xffffffff) throw new Error('TIFF output exceeds its classic 32-bit offsets.');
  const encoded=Buffer.alloc(length);
  encoded.write('II');encoded.writeUInt16LE(42,2);encoded.writeUInt32LE(directory,4);
  for (let source=0,target=8;source<bgra.length;source+=4,target+=4) {
    encoded[target]=bgra[source+2];encoded[target+1]=bgra[source+1];
    encoded[target+2]=bgra[source];encoded[target+3]=bgra[source+3];
  }
  const entries=[
    [256,width<=65535?3:4,1,width],[257,height<=65535?3:4,1,height],
    [258,3,4,bitsOffset],[259,3,1,1],[262,3,1,2],[273,4,1,8],
    [277,3,1,4],[279,4,1,pixelBytes],[284,3,1,1],
  ];
  encoded.writeUInt16LE(entries.length,directory);
  for (const [index,[tag,type,count,value]] of entries.entries()) {
    const at=directory+2+index*12;
    encoded.writeUInt16LE(tag,at);encoded.writeUInt16LE(type,at+2);
    encoded.writeUInt32LE(count,at+4);encoded.writeUInt32LE(value,at+8);
  }
  for (let channel=0;channel<4;channel++) encoded.writeUInt16LE(8,bitsOffset+channel*2);
  return encoded;
}

function pngChunk(kind,payload) {
  const chunk=Buffer.alloc(payload.length+12);
  chunk.writeUInt32BE(payload.length);chunk.write(kind,4);payload.copy(chunk,8);
  chunk.writeUInt32BE(crc32(chunk.subarray(4,-4)),chunk.length-4);
  return chunk;
}

function paeth(left,above,corner) {
  const estimate=left+above-corner;
  const a=Math.abs(estimate-left),b=Math.abs(estimate-above),c=Math.abs(estimate-corner);
  return a<=b && a<=c?left:b<=c?above:corner;
}

let pngDeflater;
function compressPng(filtered) {
  // The captured zlib 1.2.3 stream differs from current Node zlib on larger
  // images. One pinned compressor owns PNG output on every host platform.
  pngDeflater??=new WebAssembly.Instance(new WebAssembly.Module(
    readFileSync(new URL('./bitmap-deflate.wasm',import.meta.url)))).exports;
  const capacity=filtered.length+Math.floor(filtered.length/4096)+Math.floor(filtered.length/16384)+11;
  pngDeflater.ov_reset();
  const source=pngDeflater.malloc(filtered.length),target=pngDeflater.malloc(capacity);
  if (!source || !target) throw new Error('PNG compression exceeds the available WebAssembly memory.');
  new Uint8Array(pngDeflater.memory.buffer,source,filtered.length).set(filtered);
  const length=pngDeflater.ov_deflate(source,filtered.length,target,capacity);
  if (length<=0) throw new Error(`PNG compression failed with zlib status ${length}.`);
  return Buffer.from(new Uint8Array(pngDeflater.memory.buffer,target,length));
}

/** PNG preserves the renderer's RGB/RGBA storage choice, even for opaque RGBA. */
export function encodePng(input) {
  const {width,height,bgra}=readBitmapPixels(input),{hasAlpha=false}=input;
  if (typeof hasAlpha!=='boolean') throw new Error('PNG hasAlpha must be a boolean.');
  const channels=hasAlpha?4:3,rowBytes=width*channels;
  const filtered=Buffer.alloc((rowBytes+1)*height);
  let previous=Buffer.alloc(rowBytes),row=Buffer.alloc(rowBytes);
  const candidates=Array.from({length:5},()=>Buffer.alloc(rowBytes));
  for (let y=0;y<height;y++) {
    for (let x=0;x<width;x++) {
      const source=(y*width+x)*4,target=x*channels;
      row[target]=bgra[source+2];row[target+1]=bgra[source+1];row[target+2]=bgra[source];
      if (hasAlpha) row[target+3]=bgra[source+3];
    }
    let bestScore=Infinity,bestFilter=0,bestRow;
    // Choose the first minimum signed-byte sum in PNG's filter order.
    for (let filter=0;filter<=4;filter++) {
      const candidate=candidates[filter];let score=0;
      for (let column=0;column<rowBytes;column++) {
        const left=column<channels?0:row[column-channels],above=previous[column];
        const corner=column<channels?0:previous[column-channels];
        let predictor=0;
        if (filter===1) predictor=left;
        else if (filter===2) predictor=above;
        else if (filter===3) predictor=Math.floor((left+above)/2);
        else if (filter===4) predictor=paeth(left,above,corner);
        const residual=(row[column]-predictor)&255;
        candidate[column]=residual;score+=Math.min(residual,256-residual);
      }
      if (score<bestScore) {bestScore=score;bestFilter=filter;bestRow=candidate;}
    }
    const target=y*(rowBytes+1);filtered[target]=bestFilter;bestRow.copy(filtered,target+1);
    const swap=previous;previous=row;row=swap;
  }
  const compressed=compressPng(filtered);
  // PNG advertises the smallest window covering its whole filtered stream.
  // This changes CMF/FCHECK only; the compressor still uses its 32 KiB window.
  const windowBits=Math.max(8,Math.min(15,Math.ceil(Math.log2(filtered.length))));
  compressed[0]=((windowBits-8)<<4)|8;compressed[1]&=0xe0;
  compressed[1]+=(31-(compressed[0]*256+compressed[1])%31)%31;
  const header=Buffer.alloc(13);header.writeUInt32BE(width);header.writeUInt32BE(height,4);
  header[8]=8;header[9]=hasAlpha?6:2;
  const resolution=Buffer.alloc(9);resolution.writeUInt32BE(3780);resolution.writeUInt32BE(3780,4);resolution[8]=1;
  const chunks=[Buffer.from([137,80,78,71,13,10,26,10]),pngChunk('IHDR',header),pngChunk('pHYs',resolution)];
  for (let offset=0;offset<compressed.length;offset+=8192) chunks.push(pngChunk('IDAT',compressed.subarray(offset,offset+8192)));
  chunks.push(pngChunk('IEND',Buffer.alloc(0)));
  return Buffer.concat(chunks);
}
