import {readFileSync} from 'node:fs';
import {imageSize,validateBgra} from './pixel-input.mjs';
import {squareStroke} from './vendor/qt-bezier/square-stroke.mjs';
import {readVectorOutput,walkVectorOutput,reverseCubicPoints} from './vector-output.mjs';

let coverageKernel;
// Packed channel multiplication uses this integer approximation to /255.
// Exact rounding differs at products such as 239*247 and changes native bytes.
const divide255=value=>(value+(value>>>8)+128)>>>8;
const divide255Simd=value=>{const biased=value+128;return (biased+(biased>>>8))>>>8;};

function readColor(color) {
  validateBgra(color);
  if (color.length!==4) throw new Error('Raster colors need one BGRA color.');
  return color;
}

function mappedCommands({commands,stroke},transform) {
  if (!Array.isArray(commands)) throw new Error('Raster paths need an array of M/L/C/Z commands.');
  if (stroke!==undefined && typeof stroke!=='boolean') throw new Error('Raster stroke must be a boolean.');
  if (stroke && (commands.length!==2 || commands[0]?.[0]!=='M' || !['L','C'].includes(commands[1]?.[0])))
    throw new Error('A cosmetic stroke needs one M followed by one L or C edge.');
  return commands.map(command=>{
    if (!Array.isArray(command)) throw new Error('Raster commands must be arrays.');
    const [kind,...values]=command;
    if (!['M','L','C','Z'].includes(kind) || values.length!==(kind==='C'?6:kind==='Z'?0:2))
      throw new Error('Choose M/L with two coordinates, C with six, or Z with none.');
    return [kind,...values.map((value,index)=>{
      if (!Number.isFinite(value)) throw new Error('Raster coordinates must be finite numbers.');
      const mapped=value*transform[index%2]+transform[2+index%2];
      if (!Number.isFinite(mapped) || Math.abs(mapped)>32767)
        throw new Error('Raster coordinates exceed the current scan converter range.');
      return mapped;
    })];
  });
}

function pathOutline(commands) {
  const points=[],tags=[],contours=[];
  let start=-1;
  const close=()=>{
    if (start<0) return;
    if (points.at(-2)!==points[start*2] || points.at(-1)!==points[start*2+1]) {
      points.push(points[start*2],points[start*2+1]);tags.push(1);
    }
    contours.push(tags.length-1);start=-1;
  };
  const point=(x,y,tag)=>{
    points.push(Math.trunc(x*64),Math.trunc(y*64));tags.push(tag);
  };
  for (const command of commands) {
    const [kind,...values]=command;
    if (kind==='M') {close();start=tags.length;point(...values,1);}
    else {
      if (start<0) throw new Error('Start each raster subpath with M.');
      if (kind==='L') point(...values,1);
      else if (kind==='C') {point(values[0],values[1],2);point(values[2],values[3],2);point(values[4],values[5],1);}
      else close();
    }
  }
  close();
  return {points:Int32Array.from(points),tags:Uint8Array.from(tags),contours:Int32Array.from(contours)};
}

function* pathCoverage(width,height,outline,evenOdd,workspace) {
  coverageKernel??=new WebAssembly.Instance(new WebAssembly.Module(
    readFileSync(new URL('./bitmap-raster.wasm',import.meta.url)))).exports;
  const {points,tags,contours}=outline,pixels=width*height;
  while (true) {
    let end=Number(coverageKernel.__heap_base.value);
    const reserve=bytes=>{const at=Math.ceil(end/8)*8;end=at+bytes;return at;};
    const pointsAt=reserve(points.byteLength),tagsAt=reserve(tags.length),contoursAt=reserve(contours.byteLength);
    const pixelsAt=reserve(pixels),poolAt=reserve(workspace.poolSize);
    if (end>0x7fffffff) throw new Error('Raster coverage exceeds WebAssembly memory.');
    const memory=coverageKernel.memory;
    if (end>memory.buffer.byteLength) memory.grow(Math.ceil((end-memory.buffer.byteLength)/65536));
    const bytes=new Uint8Array(memory.buffer);
    bytes.set(new Uint8Array(points.buffer),pointsAt);bytes.set(tags,tagsAt);
    bytes.set(new Uint8Array(contours.buffer),contoursAt);bytes.fill(0,pixelsAt,pixelsAt+pixels);bytes.fill(0,poolAt,poolAt+workspace.poolSize);
    const status=coverageKernel.ov_coverage(width,height,pointsAt,tagsAt,tags.length,contoursAt,contours.length,
      evenOdd?1:0,pixelsAt,poolAt,workspace.poolSize);
    if (status!==0 && status!==-6) throw new Error(`Raster coverage failed with status ${status}.`);
    // QPainter has already composed successful bands when it grows the pool.
    // Yield each attempt so a retry preserves those observable partial writes.
    yield Uint8Array.from(bytes.subarray(pixelsAt,pixelsAt+pixels));
    if (status===0) return;
    workspace.poolSize*=2;
    if (workspace.poolSize>1024*1024) throw new Error('Raster coverage exceeds the one MiB pool limit.');
  }
}

/** Render fills and one-pixel square-capped edges to owned premultiplied BGRA.
 * Fitted scene assembly remains a separate operation. */
export function rasterizePaths({width,height,hasAlpha=false,background,transform=[1,1,0,0],paths}) {
  const pixels=imageSize(width,height);
  if (width>32767 || height>32767) throw new Error('Raster dimensions exceed the current scan converter range.');
  if (typeof hasAlpha!=='boolean') throw new Error('Raster hasAlpha must be a boolean.');
  if (!Array.isArray(transform) || transform.length!==4 || transform.some(value=>!Number.isFinite(value)))
    throw new Error('Raster transform needs four finite scale/translation values.');
  if (!Array.isArray(paths)) throw new Error('Raster paths must be an array.');
  const initial=readColor(background??(hasAlpha?[0,0,0,0]:[255,255,255,255]));
  if (!hasAlpha && initial[3]!==255) throw new Error('Opaque raster output needs an opaque background.');
  const bgra=new Uint8Array(pixels*4),fill=initial.map((value,index)=>index===3?value:divide255(value*initial[3]));
  // The reference RGB32 blend remains scalar. Its premultiplied ARGB32 blend
  // uses MMX/SSE, which biases before the high-byte correction instead.
  const multiplyChannel=hasAlpha?divide255Simd:divide255;
  const workspace={poolSize:8192};
  for (let pixel=0;pixel<pixels;pixel++) bgra.set(fill,pixel*4);
  for (const path of paths) {
    const color=readColor(path.color),rule=path.fillRule??'even-odd';
    if (rule!=='even-odd' && rule!=='nonzero') throw new Error('Raster fillRule must be even-odd or nonzero.');
    const commands=mappedCommands(path,transform);
    const outline=pathOutline(path.stroke?squareStroke(commands):commands);
    const premultiplied=color.map((value,index)=>index===3?value:divide255(value*color[3]));
    for (const coverage of pathCoverage(width,height,outline,!path.stroke && rule==='even-odd',workspace)) {
      for (let pixel=0;pixel<pixels;pixel++) {
        const amount=coverage[pixel];if (!amount) continue;
        const alpha=multiplyChannel(color[3]*amount),inverse=255-alpha,offset=pixel*4;
        for (let channel=0;channel<3;channel++)
          bgra[offset+channel]=multiplyChannel(premultiplied[channel]*amount)+multiplyChannel(bgra[offset+channel]*inverse);
        bgra[offset+3]=alpha+multiplyChannel(bgra[offset+3]*inverse);
      }
    }
  }
  return {width,height,bgra,hasAlpha};
}

/** Render completed fitted geometry to the native premultiplied image storage.
 * File-specific channel conversion belongs to bitmap-export. */
export function renderBitmap({geometry,width,height,scale=1,hasAlpha=false,hiddenRegions=[]}) {
  if (!Number.isFinite(scale) || scale<=0) throw new Error('Bitmap scale must be a positive finite number.');
  const options=readVectorOutput({geometry,width,height,hiddenRegions,layout:'cutouts'},'Bitmap');
  const fills=[];let current;
  walkVectorOutput(geometry,options,{
    acceptColor:()=>true,
    beginShape(color) {current={color,commands:[]};fills.push(current);},
    endShape() {},
    line(start,end,kind) {
      if (kind) current.commands.push(['M',start.x,start.y]);
      current.commands.push(['L',end.x,end.y]);
    },
    cubic(points,kind) {
      if (kind) current.commands.push(['M',...points.slice(0,2)]);
      current.commands.push(['C',...points.slice(2)]);
    },
  });
  const seams=[];
  if (!hasAlpha) {
    const curves=new Map(geometry.cubics.map(curve=>[curve.index,curve.points]));
    // The native scene preallocates cubic slots, then appends every line in
    // region traversal order. Shared cubic slots retain the last write.
    seams.length=geometry.cubicStorageSlots;
    for (const region of geometry.regions) {
      for (const edge of region.edges) {
        const neighbor=edge[2]<0?[...region.bgra.slice(0,3),0]:geometry.regions[edge[2]].bgra;
        const color=region.bgra.map((channel,index)=>Math.floor((channel+neighbor[index])/2));
        let commands;
        if (edge[1]===0) {
          const start=geometry.nodes[edge[3]],end=geometry.nodes[edge[4]];
          commands=[['M',start.x,start.y],['L',end.x,end.y]];
        } else {
          const points=curves.get(edge[0]),oriented=edge[1]===2?reverseCubicPoints(points):points;
          commands=[['M',...oriented.slice(0,2)],['C',...oriented.slice(2)]];
        }
        const path={color,commands,stroke:true};
        if (edge[1]===0) seams.push(path);else seams[edge[0]]=path;
      }
    }
  }
  return rasterizePaths({width:Math.floor(width*scale+.5),height:Math.floor(height*scale+.5),hasAlpha,
    transform:[scale,scale,0,0],paths:[...seams.filter(path=>path.color[3]>=250),...fills]});
}
