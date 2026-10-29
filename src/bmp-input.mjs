import {imageSize} from './pixel-input.mjs';

/** Original bitfield expansion promotes to16 bits before Q8 rounding. The
 *16-bit reader replicates5-bit RGB and6-bit green;32-bit replicates only8. */
function maskQuantum(mask,bits,channel) {
  const shift=mask ? Math.clz32(mask) : 0;
  const width=Math.clz32(~(mask<<shift));
  if (width===32) throw new Error('Unsupported BMP full-word mask.');
  return pixel=>{
    let word=((pixel&mask)<<shift)>>>16;
    if (bits===16) {
      if (width===5&&channel!==3) word|=(word&0xe000)>>>5;
      if (width===6&&channel===1) word|=(word&0xc000)>>>6;
      if (width<=8) word|=(word&0xff00)>>>8;
    } else if (width===8) word|=word>>>8;
    return Math.floor((word+128)/257);
  };
}

/** RLE stores one palette byte per output pixel. Row escapes and deltas
 * address this tight raster; BMP scanline padding applies only to raw data. */
function decodeRle(encoded,offset,width,height,bits) {
  const indices=new Uint8Array(width*height);
  const read=()=>{
    if (offset>=encoded.length) throw new Error('BMP RLE command is incomplete.');
    return encoded[offset++];
  };
  let x=0,y=0,position=0;
  while (y<height&&position<indices.length) {
    const count=read(),value=read();
    if (count) {
      const pixels=Math.min(count,indices.length-position);
      for (let index=0;index<pixels;index++) indices[position++]=bits===8 ? value : index&1 ? value&15 : value>>>4;
      x+=pixels;
    } else if (value===1) break;
    else if (value===0) {x=0;y++;position=y*width;}
    else if (value===2) {x+=read();y+=read();position=y*width+x;}
    else {
      const pixels=Math.min(value,indices.length-position);
      let byte=0;
      for (let index=0;index<pixels;index++) {
        if (bits===8||!(index&1)) byte=read();
        indices[position++]=bits===8 ? byte : index&1 ? byte&15 : byte>>>4;
      }
      x+=pixels;
      if ((bits===8 ? pixels : Math.ceil(pixels/2))&1) read();
    }
  }
  return indices;
}

/** Decode the first BMP raster. Header/palette bounds belong to this input
 * boundary; row import consumes only validated storage and owned pixels. */
export function decodeBmp(encoded) {
  let fileHeader=0;
  while (encoded.length>=fileHeader+2&&encoded.toString('ascii',fileHeader,fileHeader+2)==='BA') {
    if (fileHeader+14>encoded.length) throw new Error('BMP array header is incomplete.');
    fileHeader+=14;
  }
  if (encoded.length<fileHeader+26) throw new Error('BMP header is incomplete.');
  if (!['BM','CI'].includes(encoded.toString('ascii',fileHeader,fileHeader+2))) throw new Error('BMP bitmap identifier is missing.');
  const pixelOffset=encoded.readUInt32LE(fileHeader+10),headerSize=encoded.readUInt32LE(fileHeader+14),core=headerSize===12;
  if (!core&&headerSize<40) throw new Error('This BMP header version is not implemented.');
  if (fileHeader+14+headerSize>encoded.length) throw new Error('BMP information header is incomplete.');
  const width=core ? encoded.readInt16LE(fileHeader+18) : encoded.readInt32LE(fileHeader+18);
  const signedHeight=core ? encoded.readInt16LE(fileHeader+20) : encoded.readInt32LE(fileHeader+22),height=Math.abs(signedHeight);
  const pixels=imageSize(width,height),planes=encoded.readUInt16LE(fileHeader+(core ? 22 : 26)),bits=encoded.readUInt16LE(fileHeader+(core ? 24 : 28));
  const compression=core ? 0 : encoded.readUInt32LE(fileHeader+30),used=core ? 0 : encoded.readUInt32LE(fileHeader+46);
  if (planes!==1||![1,4,8,16,24,32].includes(bits)||compression>3||compression===3&&bits<16||
      compression===1&&bits!==8||compression===2&&bits!==4) {
    throw new Error('This BMP pixel encoding is not implemented.');
  }
  let masks=bits===16 ? [0x7c00,0x3e0,0x1f,0] : [0xff0000,0xff00,0xff,0];
  // The legacy reader treats every header larger than40 as extended. For a
  //52-byte header the alpha field therefore comes from the following bytes.
  if (headerSize>40&&encoded.length<fileHeader+70) throw new Error('BMP extended mask fields are incomplete.');
  const alphaMask=headerSize>40 ? encoded.readUInt32LE(fileHeader+66) : 0,hasAlpha=alphaMask!==0;
  if (compression===3) {
    if (encoded.length<fileHeader+66) throw new Error('BMP bitfield masks are incomplete.');
    masks=[0,1,2].map(index=>encoded.readUInt32LE(fileHeader+54+index*4));masks.push(alphaMask);
  }
  // BA moves the bitmap header, but the original keeps the color-table and
  // pixel offsets relative to the array start. Preserve that addressing.
  const paletteColors=bits<16 ? used||2**bits : 0,entryBytes=core||headerSize===64 ? 3 : 4,paletteStart=14+headerSize;
  if (paletteColors>2**bits||paletteStart+paletteColors*entryBytes>encoded.length) throw new Error('BMP color table is incomplete.');
  const stride=Math.ceil(width*bits/32)*4;
  const rle=compression===1||compression===2;
  if (pixelOffset<fileHeader+14+headerSize||pixelOffset>encoded.length||!rle&&pixelOffset+stride*height>encoded.length) throw new Error('BMP pixel data is incomplete.');
  const indices=rle ? decodeRle(encoded,pixelOffset,width,height,bits) : null;
  const bgra=new Uint8Array(pixels*4),quantums=bits===16||bits===32 ? masks.map((mask,channel)=>maskQuantum(mask,bits,channel)) : null;
  for (let y=0;y<height;y++) {
    const fileRow=signedHeight<0 ? y : height-y-1,row=pixelOffset+fileRow*stride;
    for (let x=0;x<width;x++) {
      const target=(y*width+x)*4;
      if (bits<=8) {
        const bit=x*bits,color=indices ? indices[fileRow*width+x] : (encoded[row+(bit>>>3)]>>>(8-bits-bit%8))&((1<<bits)-1);
        if (color>=paletteColors) throw new Error('BMP palette index is outside its color table.');
        const source=paletteStart+color*entryBytes;
        bgra[target]=encoded[source];bgra[target+1]=encoded[source+1];bgra[target+2]=encoded[source+2];bgra[target+3]=255;
      } else if (bits===24) {
        const source=row+x*3;
        bgra[target]=encoded[source];bgra[target+1]=encoded[source+1];bgra[target+2]=encoded[source+2];bgra[target+3]=255;
      } else {
        const pixel=bits===16 ? encoded.readUInt16LE(row+x*2) : encoded.readUInt32LE(row+x*4);
        bgra[target]=quantums[2](pixel);bgra[target+1]=quantums[1](pixel);bgra[target+2]=quantums[0](pixel);
        bgra[target+3]=hasAlpha ? 255-quantums[3](pixel) : 255;
      }
    }
  }
  return {width,height,bgra,hasAlpha,format:'bmp'};
}
