import {inflateSync} from 'node:zlib';
import {decodeFax} from './tiff-fax.mjs';
import {decodeNext,decodeThunder} from './tiff-legacy-codecs.mjs';
import {normalizeCmykPixels} from './pixel-input.mjs';
import {readJpegSamples,convertJpegYCbCr} from './jpeg-input.mjs';

/** Classic TIFF's first directory. Offset/count validation belongs here;
 * decompression and sample import consume only bounded directory views. */
function readDirectory(encoded) {
  const little=encoded[0]===0x49;
  const view=new DataView(encoded.buffer,encoded.byteOffset,encoded.byteLength);
  const bounded=(offset,count)=>{
    if (offset<0 || count<0 || offset+count>encoded.length) throw new Error('TIFF field data is incomplete.');
    return encoded.subarray(offset,offset+count);
  };
  bounded(0,8);
  if (view.getUint16(2,little)!==42) throw new Error('Only classic TIFF directories are currently implemented.');
  const directory=view.getUint32(4,little);bounded(directory,2);
  const count=view.getUint16(directory,little);bounded(directory+2,count*12+4);
  const fields=new Map(),sizes={1:1,2:1,3:2,4:4,5:8,6:1,7:1,8:2,9:4,10:8,11:4,12:8};
  for (let index=0;index<count;index++) {
    const entry=directory+2+index*12,tag=view.getUint16(entry,little),type=view.getUint16(entry+2,little);
    const items=view.getUint32(entry+4,little),size=sizes[type];
    if (!size) throw new Error('Unsupported TIFF field type.');
    const bytes=items*size,offset=bytes<=4 ? entry+8 : view.getUint32(entry+8,little);
    bounded(offset,bytes);
    const values=[];
    for (let item=0;item<items;item++) {
      const at=offset+item*size;
      if ([1,2,6,7].includes(type)) values.push(type===6 ? view.getInt8(at) : view.getUint8(at));
      else if (type===3) values.push(view.getUint16(at,little));
      else if (type===4) values.push(view.getUint32(at,little));
      else if (type===8) values.push(view.getInt16(at,little));
      else if (type===9) values.push(view.getInt32(at,little));
      else if (type===11) values.push(view.getFloat32(at,little));
      else if (type===12) values.push(view.getFloat64(at,little));
      else values.push(type===5 ? view.getUint32(at,little)/view.getUint32(at+4,little)
        : view.getInt32(at,little)/view.getInt32(at+4,little));
    }
    fields.set(tag,values);
  }
  return {little,fields,bounded};
}

/** TIFF LZW differs from GIF in both packing and early width changes. Its
 * bounded phrase dictionary expands before row prediction and sample import. */
function decodeLzw(encoded,count) {
  const dictionary=Array.from({length:4096},(_,index)=>index<256 ? Uint8Array.of(index) : null);
  const output=new Uint8Array(count);
  let next=258,bits=9,bit=0,written=0,previous=null;
  while (written<count) {
    if (bit+bits>encoded.length*8) throw new Error('TIFF LZW pixels are incomplete.');
    let code=0;
    for (let step=0;step<bits;step++,bit++) code=code*2+((encoded[bit>>>3]>>>(7-(bit&7)))&1);
    if (code===256) {next=258;bits=9;previous=null;continue;}
    if (code===257) throw new Error('TIFF LZW ended before all strip pixels.');
    let phrase;
    if (code<next && dictionary[code]) phrase=dictionary[code];
    else if (code===next && previous) {
      phrase=new Uint8Array(previous.length+1);phrase.set(previous);phrase[previous.length]=previous[0];
    } else throw new Error('TIFF LZW code is outside the dictionary.');
    if (written+phrase.length>count) throw new Error('TIFF LZW phrase exceeds the strip.');
    output.set(phrase,written);written+=phrase.length;
    if (previous && next<4096) {
      const entry=new Uint8Array(previous.length+1);entry.set(previous);entry[previous.length]=phrase[0];dictionary[next++]=entry;
      if (next===(1<<bits)-1 && bits<12) bits++;
    }
    previous=phrase;
  }
  return output;
}

function decodeStrip(encoded,compression,count,layout) {
  if (compression===32766) return decodeNext(encoded,layout);
  if (compression===32809) return decodeThunder(encoded,layout);
  if ([2,3,4,32771].includes(compression)) return decodeFax(encoded,{...layout,compression});
  if (compression===1) {
    if (encoded.length<count) throw new Error('TIFF uncompressed strip is incomplete.');
    return Uint8Array.from(encoded.subarray(0,count));
  }
  if (compression===5) return decodeLzw(encoded,count);
  if (compression===8 || compression===32946) {
    const output=inflateSync(encoded);
    if (output.length<count) throw new Error('TIFF deflate strip is incomplete.');
    return output.subarray(0,count);
  }
  if (compression!==32773) throw new Error('Unsupported TIFF compression.');
  const output=new Uint8Array(count);
  let read=0,written=0;
  while (written<count) {
    if (read>=encoded.length) throw new Error('TIFF PackBits strip is incomplete.');
    const control=encoded[read++];
    if (control===128) continue;
    const length=control<128 ? control+1 : 257-control;
    const unitEnd=Math.min(count,(Math.floor(written/layout.readUnitBytes)+1)*layout.readUnitBytes);
    if (written+length>unitEnd) throw new Error('TIFF PackBits packet crosses an image row or block boundary.');
    if (control<128) {
      if (read+length>encoded.length) throw new Error('TIFF PackBits literal is incomplete.');
      output.set(encoded.subarray(read,read+length),written);read+=length;
    } else {
      if (read>=encoded.length) throw new Error('TIFF PackBits repeat byte is missing.');
      output.fill(encoded[read++],written,written+length);
    }
    written+=length;
  }
  return output;
}

/** Shared tables precede each abbreviated strip frame. TIFF consumes raw
 * component samples, including CMYK ink, instead of JPEG file colors. */
function decodeJpegBlock(encoded,tables,{columns,rows,samples,photometric}) {
  if (encoded.length<2||encoded[0]!==255||encoded[1]!==216) throw new Error('TIFF JPEG block requires an SOI marker.');
  if (tables) {
    const merged=new Uint8Array(tables.length+encoded.length-4);
    merged.set(tables.subarray(0,-2));merged.set(encoded.subarray(2),tables.length-2);encoded=merged;
  }
  const decoded=readJpegSamples(encoded);
  if (decoded.width!==columns||decoded.height!==rows||decoded.planes.length!==samples) {
    throw new Error('TIFF JPEG frame dimensions or components disagree with its block.');
  }
  if (photometric===6) convertJpegYCbCr(decoded.planes);
  const output=new Uint8Array(columns*rows*samples);
  for (let pixel=0;pixel<columns*rows;pixel++) for (let sample=0;sample<samples;sample++) {
    output[pixel*samples+sample]=decoded.planes[sample][pixel];
  }
  return output;
}

/** Undo prediction over complete decoded storage rows before any delegate
 * cropping, orientation or legacy gray16 byte-stride behavior. */
function undoHorizontalPredictor(bytes,{columns,rows,depth,samples,little}) {
  const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength),size=depth/8;
  const read=depth===8 ? offset=>bytes[offset] : depth===16 ? offset=>view.getUint16(offset,little) : offset=>view.getUint32(offset,little);
  const write=depth===8 ? (offset,value)=>{bytes[offset]=value;} : depth===16
    ? (offset,value)=>view.setUint16(offset,value,little) : (offset,value)=>view.setUint32(offset,value,little);
  const rowBytes=columns*samples*size;
  for (let row=0;row<rows;row++) for (let index=samples;index<columns*samples;index++) {
    const offset=row*rowBytes+index*size;
    write(offset,read(offset)+read(offset-samples*size));
  }
}

/** Floating prediction accumulates the shuffled byte planes across the
 * whole row, including plane boundaries, before transposing each sample. */
function undoFloatingPredictor(bytes,{columns,rows,depth,samples}) {
  const size=depth/8,rowSamples=columns*samples,rowBytes=rowSamples*size;
  const planes=new Uint8Array(rowBytes);
  for (let row=0;row<rows;row++) {
    const start=row*rowBytes;
    for (let index=samples;index<rowBytes;index++) bytes[start+index]+=bytes[start+index-samples];
    planes.set(bytes.subarray(start,start+rowBytes));
    for (let sample=0;sample<rowSamples;sample++) for (let byte=0;byte<size;byte++) {
      const plane=size-1-byte;
      bytes[start+sample*size+byte]=planes[plane*rowSamples+sample];
    }
  }
}

function sampleQuantum(number,depth,format,gray16) {
  if (gray16) return number>>>8;
  if (format!==3&&depth===32) return Math.floor(((number+0x808080)>>>0)/0x1010101);
  const scaled=number*255/(format===3 ? 1 : 2**depth-1);
  // The native importer calls its float32 rounding helper for both float
  // depths. Preserve that storage boundary and the uint32 addition above.
  return Math.max(0,Math.min(255,Math.floor((format===3 ? Math.fround(scaled) : scaled)+0.5)));
}

/** These codecs consume native MSB-first commands after TIFF's byte reversal. */
function reverseBitOrder(bytes) {
  return Uint8Array.from(bytes,byte=>{
    byte=(byte&0x55)<<1|(byte>>>1)&0x55;
    byte=(byte&0x33)<<2|(byte>>>2)&0x33;
    return (byte<<4|byte>>>4)&255;
  });
}

/** Import decoded storage into owned component planes. Prediction precedes
 * strip/plane/tile addressing and the original's separate Q8 import paths. */
function readSamples({fields,bounded,little},{width,height,samples,depth,format,hasAlpha}) {
  const value=(tag,fallback)=>fields.get(tag)?.[0]??fallback;
  const tiled=fields.has(322),planar=value(284,1),compression=value(259,1),predictor=value(317,1);
  if (compression===7&&depth!==8) throw new Error('TIFF JPEG requires eight-bit samples.');
  if (fields.has(317)&&![5,8,32946].includes(compression)) throw new Error('TIFF predictor tag is not registered for this compression.');
  if (![1,2].includes(planar)||![1,2,3].includes(predictor)) throw new Error('Unsupported TIFF planar configuration or predictor.');
  if (predictor===2&&![8,16].includes(depth)) throw new Error('Unsupported TIFF horizontal predictor sample type.');
  if (predictor===3&&(format!==3||![32,64].includes(depth))) throw new Error('Unsupported TIFF floating predictor sample type.');
  if (tiled&&![1,2,4,8,16].includes(depth)) throw new Error('Original TIFF tile delegate does not support this sample depth.');
  const columns=tiled ? value(322,0) : width,blockHeight=tiled ? value(323,0) : value(278,height);
  if (!columns||!blockHeight) throw new Error('TIFF block dimensions must be positive.');
  const photometric=value(262,1),singleSample=!tiled&&[0,1,3].includes(photometric);
  // The original reads row0 of each plane, then row1 of each plane. Its
  // compressed scanline decoder cannot seek inside a newly selected strip.
  if (!tiled&&!singleSample&&planar===2&&samples>1&&compression!==1&&blockHeight>1&&height>1) {
    throw new Error('TIFF compressed planar strips do not support native row access.');
  }
  const across=Math.ceil(width/columns),down=Math.ceil(height/blockHeight),planes=planar===2 ? samples : 1;
  const offsets=fields.get(tiled ? 324 : 273),counts=fields.get(tiled ? 325 : 279),blocks=across*down;
  if (!offsets||!counts||offsets.length<blocks*planes||counts.length<blocks*planes) throw new Error('TIFF block directory is incomplete.');
  const grayScanline=!tiled&&[0,1].includes(photometric)&&samples>1&&depth>=8;
  const grayChannels=hasAlpha ? 2 : 1;
  const jpegTables=compression===7&&fields.has(347) ? Uint8Array.from(fields.get(347)) : null;
  if (jpegTables&&(jpegTables.length<4||jpegTables[0]!==255||jpegTables[1]!==216||jpegTables.at(-2)!==255||jpegTables.at(-1)!==217)) {
    throw new Error('TIFF JPEG tables require SOI and EOI markers.');
  }
  // Values are already Q8 after import. Float storage retains NaN until the
  // existing pixel operations; palette indices retain their integer range.
  const Storage=photometric===3 ? Uint32Array : format===3 ? Float32Array : Uint8Array;
  const output=new Storage(width*height*samples),blockSamples=planar===2 ? 1 : samples,range=2**depth;
  const orientation=(value(274,1)-1)&3;
  const delegate=tiled||photometric===6;
  const fillOrder=value(266,1);
  if (![1,2].includes(fillOrder)) throw new Error('Unsupported TIFF fill order.');
  const legacy=compression===32766||compression===32809;
  if (legacy&&(samples!==1||depth!==(compression===32766 ? 2 : 4))) {
    throw new Error('TIFF NeXT/ThunderScan requires one sample at its codec depth.');
  }
  const scanline=compression===32809 ? new Uint8Array(Math.ceil(columns/2)) : null;
  const fax=[2,3,4,32771].includes(compression);
  if (fax&&(depth!==1||samples!==1)) throw new Error('TIFF CCITT requires one bilevel sample per pixel.');
  for (let plane=0;plane<(singleSample ? 1 : planes);plane++) for (let block=0;block<blocks;block++) {
    const top=Math.floor(block/across)*blockHeight,left=block%across*columns;
    const rows=tiled ? blockHeight : Math.min(blockHeight,height-top),rowBytes=Math.ceil(columns*blockSamples*depth/8);
    const index=plane*blocks+block;
    let encoded=bounded(offsets[index],counts[index]);
    // The JPEG codec suppresses TIFF bit reversal; its markers and entropy
    // stream retain JPEG order even when FillOrder is2.
    if (fillOrder===2&&!fax&&compression!==7) encoded=reverseBitOrder(encoded);
    let bytes=compression===7 ? decodeJpegBlock(encoded,jpegTables,
      {columns,rows,samples:blockSamples,photometric}) : decodeStrip(encoded,compression,rows*rowBytes,
      {width:columns,rows,fillOrder,t4Options:value(292,0),scanline,readUnitBytes:tiled ? rows*rowBytes : rowBytes});
    if (predictor===2) undoHorizontalPredictor(bytes,{columns,rows,depth,samples:blockSamples,little});
    if (predictor===3) undoFloatingPredictor(bytes,{columns,rows,depth,samples:blockSamples});
    // Native blocks contain little-endian words before import. This also
    // precedes the gray16 tile byte skew and unaligned grayscale byte padding.
    if (!little&&depth>=16&&predictor!==3) {
      const words=Buffer.from(bytes.buffer,bytes.byteOffset,bytes.byteLength);
      if (depth===16) words.swap16();
      else if (depth===32) words.swap32();
      else words.swap64();
    }
    const grayStride=grayChannels*depth/8+samples-grayChannels;
    const grayRow=grayScanline ? new Uint8Array(Math.max(rowBytes,width*grayStride)) : null;
    const view=grayRow ? new DataView(grayRow.buffer) : new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);
    const visibleColumns=Math.min(columns,width-left),visibleRows=Math.min(blockHeight,height-top);
    const stride=tiled&&depth===16&&samples===1&&[0,1].includes(photometric) ? visibleColumns*2+(columns-visibleColumns) : rowBytes;
    const flipX=delegate&&(orientation===1||orientation===2),flipY=delegate&&(orientation===2||orientation===3);
    const gray16Quantum=tiled&&depth===16&&[0,1].includes(photometric);
    const importedChannels=grayScanline ? grayChannels : blockSamples;
    for (let row=0;row<rows;row++) {
      if (grayRow) grayRow.set(bytes.subarray(row*rowBytes,(row+1)*rowBytes));
      for (let index=0;index<columns*importedChannels;index++) {
        const bit=index*depth,channel=index%importedChannels,column=Math.floor(index/importedChannels);
        const offset=grayRow ? column*grayStride+channel*depth/8 : row*stride+Math.floor(bit/8);
        let sample;
        if (depth<8) sample=(bytes[offset]>>>(8-depth-bit%8))&(range-1);
        else if (format===3) sample=depth===32 ? view.getFloat32(offset,true) : view.getFloat64(offset,true);
        else if (depth===8) sample=view.getUint8(offset);
        else if (depth===16) sample=view.getUint16(offset,true);
        else if (depth===32) sample=view.getUint32(offset,true);
        else throw new Error('Unsupported TIFF integer sample depth.');
        // Orientation is applied inside each visible tile, including partial
        // edge tiles. Scanline import preserves file order instead.
        const x=left+(flipX ? visibleColumns-1-column : column),y=top+(flipY ? visibleRows-1-row : row);
        const component=planar===2&&!grayScanline ? plane : channel;
        if (x>=left&&y>=top&&x<width&&y<height) output[(y*width+x)*samples+component]=
          photometric===3&&component===0 ? sample : sampleQuantum(sample,depth,format,gray16Quantum);
      }
    }
  }
  return {values:output,tiled};
}

/** The original sample importer normalizes each component to Q8 before
 * disassociating alpha. Raw CMYK ink stays separate from RGB normalization. */
export function decodeTiffPixels(encoded) {
  const directory=readDirectory(encoded),{fields}=directory,value=(tag,fallback)=>fields.get(tag)?.[0]??fallback;
  const width=value(256,0),height=value(257,0),pixels=width*height;
  if (!Number.isInteger(width)||!Number.isInteger(height)||width<1||height<1||pixels>2147483647) throw new Error('TIFF requires positive dimensions with an int32 pixel count.');
  const samples=value(277,1),depths=fields.get(258)??[1],depth=depths[0],format=value(339,1);
  if (!depths.every(item=>item===depth)||![1,2,4,8,16,32,64].includes(depth)) throw new Error('Unsupported TIFF sample depths.');
  if (![1,2,3].includes(format)||format===3&&![32,64].includes(depth)) throw new Error('Unsupported TIFF sample format.');
  const photometric=value(262,1),cmyk=photometric===5;
  if (![0,1,2,3,5].includes(photometric)&&!(photometric===6&&value(259,1)===7)) throw new Error('Unsupported TIFF photometric interpretation.');
  if (photometric===6&&samples!==3) throw new Error('TIFF JPEG YCbCr requires three samples.');
  const generic=photometric===6&&!fields.has(322);
  // SetQuantumDepth allocates5*width+1 bytes at Q8; the generic delegate
  // writes4*width*height. Keep undefined native heap writes outside ownership.
  if (generic&&pixels*4>width*5+1) throw new Error('Original TIFF generic reader overruns its quantum buffer for this raster.');
  const extras=fields.get(338)??[],hasAlpha=extras.length ? samples>3||extras[0]===1||extras[0]===2 : photometric===2&&samples===4;
  const components=cmyk ? 4 : photometric===2||photometric===6 ? 3 : 1;
  if (samples<components+Number(hasAlpha)) throw new Error('TIFF image has too few samples for its channels.');
  const bgra=new Uint8Array(pixels*4),black=cmyk ? new Uint8Array(pixels) : null;
  const palette=fields.get(320),paletteColors=2**depth;
  if (photometric===3&&(!palette||palette.length!==paletteColors*3)) throw new Error('TIFF palette is incomplete.');
  const paletteMaximum=palette?.some(item=>item>255) ? 65535 : 255;
  const {values,tiled}=readSamples(directory,{width,height,samples,depth,format,hasAlpha});
  // Two-sample Q8 grayscale uses IndexQuantum. That importer ignores
  // MINISWHITE above one bit; direct GrayQuantum inverts8/16-bit samples.
  const indexedGray=!tiled&&samples===2&&depth<=8;
  const invertGray=photometric===0&&(tiled||!hasAlpha&&(depth===1||!indexedGray&&[8,16].includes(depth)));
  for (let pixel=0;pixel<pixels;pixel++) {
      const target=pixel*4,index=pixel*samples,alpha=hasAlpha ? values[index+components] : 255;
      let red,green,blue;
      if (photometric===3) {
        const color=values[index];
        red=Math.floor(palette[color]*255/paletteMaximum+0.5);
        green=Math.floor(palette[color+paletteColors]*255/paletteMaximum+0.5);
        blue=Math.floor(palette[color+paletteColors*2]*255/paletteMaximum+0.5);
      } else if (photometric===2||photometric===6||cmyk) {
        red=values[index];green=values[index+1];blue=values[index+2];
        if (cmyk) black[pixel]=values[index+3];
      } else {
        red=green=blue=invertGray ? 255-values[index] : values[index];
      }
      if (tiled&&cmyk) {
        // TIFFReadRGBATile converts ink to RGB, but the original retains the
        // CMYK colorspace and its zero black plane before the source setter.
        const light=255-black[pixel];
        red=Math.floor((255-red)*light/255);green=Math.floor((255-green)*light/255);blue=Math.floor((255-blue)*light/255);
        black[pixel]=0;
      } else if (tiled&&hasAlpha&&extras[0]===2) {
        red=Math.floor((red*alpha+127)/255);green=Math.floor((green*alpha+127)/255);blue=Math.floor((blue*alpha+127)/255);
      } else if (!tiled&&extras[0]===1) {
        const scale=alpha ? 1/(alpha*(1/255)) : 1;
        const disassociate=channel=>Math.min(255,Math.floor(scale*channel+0.5));
        red=disassociate(red);green=disassociate(green);blue=disassociate(blue);
      }
      bgra[target]=blue;bgra[target+1]=green;bgra[target+2]=red;bgra[target+3]=alpha;
  }
  if (generic) {
    // The original passes an RGBA byte array to a byte-valued importer. Its
    // red-only output consumes the first pixelCount bytes, not packed words.
    const rgba=new Uint8Array(pixels*4);
    for (let pixel=0;pixel<pixels;pixel++) {
      const offset=pixel*4;
      rgba[offset]=bgra[offset+2];rgba[offset+1]=bgra[offset+1];rgba[offset+2]=bgra[offset];rgba[offset+3]=255;
    }
    for (let pixel=0;pixel<pixels;pixel++) {
      const offset=pixel*4;bgra[offset]=0;bgra[offset+1]=0;bgra[offset+2]=rgba[pixel];bgra[offset+3]=255;
    }
  }
  return {width,height,bgra,hasAlpha,colorSpace:cmyk ? 12 : 1,black};
}

export function decodeTiff(encoded) {
  const {width,height,bgra,hasAlpha,colorSpace,black}=decodeTiffPixels(encoded);
  if (colorSpace===12) normalizeCmykPixels(bgra,black);
  return {width,height,bgra,hasAlpha,format:'tiff'};
}
