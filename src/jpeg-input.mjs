// Numerical code adapted from IJG jpeg-6b, copyright 1991-1998 Thomas G. Lane.
// See jpeg-licenses.txt for the unchanged IJG README and modification notices.
import {readJpegCoefficients} from './jpeg-coefficients.mjs';
import {normalizeCmykPixels} from './pixel-input.mjs';

const idctRange=Uint8Array.from({length:1024},(_,index)=>index<128 ? index+128 : index<512 ? 255 : index<896 ? 0 : index-896);
const clamp=value=>Math.max(0,Math.min(255,value));

/** The shared one-dimensional kernel keeps signed 32-bit products. Columns
 * retain two fractional bits; rows remove those bits and the factor of eight. */
function inverseTransform(values,output) {
  let z2=values[2],z3=values[6];
  const z1=Math.imul(z2+z3,4433);
  const even2=z1-Math.imul(z3,15137),even3=z1+Math.imul(z2,6270);
  const even0=(values[0]+values[4])<<13,even1=(values[0]-values[4])<<13;
  const t10=even0+even3,t11=even1+even2,t12=even1-even2,t13=even0-even3;
  let a=values[7],b=values[5],c=values[3],d=values[1];
  const p=a+d,q=b+c;
  z2=a+c;z3=b+d;
  const z5=Math.imul(z2+z3,9633);
  const oddP=-Math.imul(p,7373),oddQ=-Math.imul(q,20995);
  z2=-Math.imul(z2,16069)+z5;z3=-Math.imul(z3,3196)+z5;
  a=Math.imul(a,2446)+oddP+z2;b=Math.imul(b,16819)+oddQ+z3;
  c=Math.imul(c,25172)+oddQ+z2;d=Math.imul(d,12299)+oddP+z3;
  output[0]=t10+d;output[7]=t10-d;output[1]=t11+c;output[6]=t11-c;
  output[2]=t12+b;output[5]=t12-b;output[3]=t13+a;output[4]=t13-a;
}

function componentSamples(component,width,height,maxH,maxV) {
  const stride=component.blocksPerLine*8,rows=component.blocksPerColumn*8;
  const pixels=new Uint8Array(stride*rows),workspace=new Int32Array(64);
  const values=new Float64Array(8),transformed=new Float64Array(8);
  for (let y=0;y<component.blocksPerColumn;y++) for (let x=0;x<component.blocksPerLine;x++) {
    const coefficients=component.blocks[y][x],quantization=component.quantizationTable;
    for (let column=0;column<8;column++) {
      for (let row=0;row<8;row++) values[row]=coefficients[column+row*8]*quantization[column+row*8];
      inverseTransform(values,transformed);
      for (let row=0;row<8;row++) workspace[column+row*8]=(transformed[row]+1024)>>11;
    }
    for (let row=0;row<8;row++) {
      for (let column=0;column<8;column++) values[column]=workspace[row*8+column];
      inverseTransform(values,transformed);
      const origin=(y*8+row)*stride+x*8;
      for (let column=0;column<8;column++) pixels[origin+column]=idctRange[((transformed[column]+131072)>>18)&1023];
    }
  }
  const horizontal=maxH/component.h,vertical=maxV/component.v;
  if (!Number.isInteger(horizontal) || !Number.isInteger(vertical)) throw new Error('Fractional JPEG sampling is unsupported.');
  const actualWidth=Math.ceil(width/horizontal),actualHeight=Math.ceil(height/vertical),output=new Uint8Array(width*height);
  const sample=(x,y)=>pixels[Math.max(0,Math.min(actualHeight-1,y))*stride+Math.max(0,Math.min(actualWidth-1,x))];
  for (let y=0;y<height;y++) for (let x=0;x<width;x++) {
    const column=Math.floor(x/horizontal),row=Math.floor(y/vertical);
    let value=sample(column,row);
    if (horizontal===2 && actualWidth>2 && vertical===1) {
      value=(value*3+sample(column+(x%2 ? 1 : -1),row)+(x%2 ? 2 : 1))>>2;
    } else if (horizontal===2 && actualWidth>2 && vertical===2) {
      const near=row+(y%2 ? 1 : -1),next=column+(x%2 ? 1 : -1);
      const current=sample(column,row)*3+sample(column,near),neighbor=sample(next,row)*3+sample(next,near);
      // Preserve IJG's alternating half-rounding bias in the combined filter.
      value=(current*3+neighbor+(x%2 ? 7 : 8))>>4;
    }
    output[y*width+x]=value;
  }
  return output;
}

/** JPEG owns coefficient reconstruction and sampling. Containers retain
 * their own interpretation of these owned component planes. */
export function readJpegSamples(encoded) {
  const frame=readJpegCoefficients(encoded),{width,height,maxH,maxV}=frame;
  const planes=frame.components.map(component=>componentSamples(component,width,height,maxH,maxV));
  return {width,height,planes,jfif:frame.jfif,adobe:frame.adobe,componentIds:frame.components.map(component=>component.id)};
}

/** Convert owned sample planes with IJG's integer rounding. A TIFF YCbCr
 * delegate selects this transform from its tag, independently of markers. */
export function convertJpegYCbCr(planes) {
  for (let pixel=0;pixel<planes[0].length;pixel++) {
    const y=planes[0][pixel],cb=planes[1][pixel]-128,cr=planes[2][pixel]-128;
    planes[0][pixel]=clamp(y+((91881*cr+32768)>>16));
    planes[1][pixel]=clamp(y+((-22554*cb-46802*cr+32768)>>16));
    planes[2][pixel]=clamp(y+((116130*cb+32768)>>16));
  }
}

/** Private original PixelPacket import, before the source setter converts
 * four-channel ink to RGB. The black plane is owned, separate from opacity. */
export function decodeJpegPixels(encoded) {
  const frame=readJpegSamples(encoded),{width,height,planes}=frame;
  if (![1,3,4].includes(planes.length)) throw new Error('Unsupported JPEG component count.');
  const bgra=new Uint8Array(width*height*4),cmyk=planes.length===4;
  const black=cmyk ? planes[3] : null;
  const directRgb=!frame.jfif && (frame.adobe ? frame.adobe.transformCode===0
    : frame.componentIds.join(',')==='82,71,66');
  const ycc=planes.length===3 && !directRgb || cmyk && frame.adobe?.transformCode===2;
  if (ycc) convertJpegYCbCr(planes);
  for (let pixel=0;pixel<width*height;pixel++) {
    let red=planes[0][pixel],green=planes.length===1 ? red : planes[1][pixel],blue=planes.length===1 ? red : planes[2][pixel];
    if (cmyk) {
      // JPEG YCCK first complements YCC->RGB; the native import complements
      // again. Plain CMYK imports always complement, including markerless files.
      if (!ycc) {red=255-red;green=255-green;blue=255-blue;}
      black[pixel]=255-black[pixel];
    }
    const offset=pixel*4;bgra[offset]=blue;bgra[offset+1]=green;bgra[offset+2]=red;bgra[offset+3]=255;
  }
  return {width,height,bgra,colorSpace:cmyk ? 12 : 1,black};
}

export function decodeJpeg(encoded) {
  const {width,height,bgra,colorSpace,black}=decodeJpegPixels(encoded);
  if (colorSpace===12) normalizeCmykPixels(bgra,black);
  return {width,height,bgra,hasAlpha:false,format:'jpeg'};
}
