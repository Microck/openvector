import {readRasterInput} from './region-data.mjs';
import {readPaletteColors} from './pixel-input.mjs';
import {streamGeneralDecimal} from './decimal-format.mjs';
import {fixedDouble,roundSignificand,binaryDouble,precision,unit,coordinateInt32} from './extended-arithmetic.mjs';
import {readEditorGesture} from './editor-input.mjs';

const extended = value => roundSignificand(value,64);
const multiply = (first,second) => extended(first*second/unit);
const storeDouble = value => binaryDouble(value,-Number(precision));
const storeFloat = value => storeDouble(roundSignificand(value,24));
const tools = ['pan','pencil','fill','zap','picker'];

// The native sampler truncates extended expressions, but spills the X product
// to double before adding its origin. A double-only line changes boundary pixels.
function strokeSamples(from,to,paint) {
  const [startX,startY] = to,[endX,endY] = from;
  if (Math.trunc(startX) === Math.trunc(endX) && Math.trunc(startY) === Math.trunc(endY))
    return paint(Math.trunc(startX),Math.trunc(startY));
  const xOrigin = fixedDouble(startX),yOrigin = fixedDouble(startY);
  const dx = extended(fixedDouble(endX)-xOrigin),dy = extended(fixedDouble(endY)-yOrigin);
  const square = extended(multiply(dx,dx)+multiply(dy,dy));
  const radicand = square*unit;
  let root = 1n << BigInt(Math.ceil(radicand.toString(2).length/2));
  for (;;) {
    const next = (root+radicand/root)/2n;
    if (next >= root) break;
    root = next;
  }
  const samples = Number(extended(extended(root)*4n)/unit);
  // With one sample, both 0/0 expressions convert to x87's int64 indefinite;
  // the native painter receives its low int32 word, zero for both coordinates.
  if (samples === 1) return paint(0,0);
  let changed = false,previousX = -1,previousY = -1;
  for (let sample = 0; sample < samples; sample++) {
    const ratio = extended(BigInt(sample)*unit/BigInt(samples-1));
    const xProduct = fixedDouble(storeDouble(multiply(dx,ratio)));
    const x = Number(extended(xProduct+xOrigin)/unit);
    const y = Number(extended(multiply(dy,ratio)+yOrigin)/unit);
    if (x !== previousX || y !== previousY) changed = paint(x,y) || changed;
    previousX = x;previousY = y;
  }
  return changed;
}

function readCommand(input) {
  if (!input || !['fill','pencil','zap'].includes(input.tool)) throw new Error('Edit tool must be fill, pencil or zap.');
  const {tool,commit = true} = input;
  if (typeof commit !== 'boolean') throw new Error('Edit commit must be boolean.');
  const color = input.color ?? (tool === 'zap' ? [0,0,0,1] : undefined);
  const bgra = readPaletteColors(color);
  if (bgra.length !== 4) throw new Error('Edit color must contain four normalized BGRA channels.');
  let x = 0,y = 0,coordinates = [0,0,0,0];
  if (tool === 'pencil') {
    for (const endpoint of [input.from,input.to]) {
      if (!Array.isArray(endpoint) || endpoint.length !== 2 || endpoint.some(value => !Number.isFinite(value)))
        throw new Error('Pencil from and to must each contain two finite coordinates.');
    }
    coordinates = [...input.from,...input.to];
  } else {
    ({x,y} = input);
    if (![x,y].every(value => Number.isInteger(value) && value >= -2147483648 && value <= 2147483647))
      throw new Error('Edit x and y must be signed int32 coordinates.');
  }
  const kind = tool === 'pencil' ? 3 : tool === 'fill' ? 5 : 6;
  const argb = Array.from(bgra).reverse();
  return {record:toolRecord(kind,x,y,coordinates,argb),commit};
}

function toolRecord(kind,x,y,coordinates,argb) {
  const key = `${kind} ${x} ${y} ${coordinates.map(streamGeneralDecimal).join(' ')} [${argb.map(streamGeneralDecimal).join(', ')}]`;
  return {kind,x,y,coordinates,argb,key};
}

function packedColor(bytes,offset) {
  return ((bytes[offset+3]<<24) | (bytes[offset+2]<<16) | (bytes[offset+1]<<8) | bytes[offset]) >>> 0;
}

function zapDistance(first,second) {
  const differences = [];
  // R/G/B are spilled doubles; alpha remains extended through its product.
  for (const shift of [16,8,0]) differences.push(fixedDouble((((second>>>shift)&255)-((first>>>shift)&255))*(1/255)));
  differences.push(multiply(fixedDouble((second>>>24)-(first>>>24)),fixedDouble(1/255)));
  let sum = 0n;
  for (const difference of differences) sum = extended(sum+multiply(difference,difference));
  return sum;
}

/** Owned displayed-color bitmap and native replay histories. This internal
 * component is independent of both the original program and its Qt widgets. */
export class SegmentationEditor {
  #width;#height;#base;#pixels;#source;#background;#backgroundMode;
  #undo = [];#redo = [];
  #tool = 'pan';#previousTool = 'pan';#color;

  constructor(input) {
    const {width,height,bgra} = readRasterInput(input);
    readRasterInput({width,height,bgra:input.source});
    const background = readPaletteColors(input.background);
    if (background.length !== 4) throw new Error('Editor background must contain four BGRA channels.');
    this.#width = width;this.#height = height;
    this.#base = Uint8Array.from(bgra);this.#pixels = this.#base.slice();
    this.#source = Uint8Array.from(input.source);
    this.#color = readEditorGesture({phase:'color',color:input.color},tools).color;
    this.#background = Array.from(background,value => Math.round(value*65535)/65535);
    this.#backgroundMode = input.backgroundMode;
  }

  historyState() { return {undoCount:this.#undo.length,redoCount:this.#redo.length}; }

  historyCheckpoint() { return {undo:structuredClone(this.#undo),redo:structuredClone(this.#redo)}; }

  /** Restore trusted records retained by the document, preserving tool state. */
  restoreCheckpoint(checkpoint) {
    this.#undo = structuredClone(checkpoint.undo);this.#redo = structuredClone(checkpoint.redo);this.#replay();
    return this.snapshot();
  }

  snapshot() {
    return {width:this.#width,height:this.#height,bgra:this.#pixels.slice(),
      undo:structuredClone(this.#undo),redo:structuredClone(this.#redo),key:this.#undo.map(record => record.key).join(''),
      tool:this.#tool,color:this.#color.slice()};
  }

  apply(input) {
    const {record,commit} = readCommand(input);
    this.#applyRecord(record,commit);
    return this.snapshot();
  }

  #applyRecord(record,commit = true) {
    if (this.#execute(record) && commit) { this.#undo.push(record);this.#redo = []; }
  }

  /** View gestures use the document's retained pointer and produce the same
   * records as direct commands. No-op moves still advance that shared point. */
  gesture(input,pointer) {
    const event = readEditorGesture(input,tools);
    if (event.phase === 'select') {
      const next = event.tool === this.#tool ? 'pan' : event.tool;
      this.#previousTool = this.#tool;this.#tool = next;
    } else if (event.phase === 'color') this.#color = event.color;
    else if (this.#tool === 'picker') {
      if (event.phase === 'release' && this.#inBounds(...event.point)) {
        const [x,y] = event.point.map(Math.trunc),color = packedColor(this.#pixels,(y*this.#width+x)*4);
        this.#color = Float32Array.from([0,8,16,24],shift => ((color>>>shift)&255)/255);
        this.#tool = this.#previousTool;
      }
    } else if (event.buttons&1) {
      const [x,y] = event.point.map(coordinateInt32),argb = Array.from(this.#color).reverse();
      if (this.#tool === 'pencil' && (event.phase === 'press' || event.phase === 'move')) {
        if (event.phase === 'press') pointer.previousPoint = event.point;
        this.#applyRecord(toolRecord(3,x,y,[...event.point,...pointer.previousPoint],argb));
        pointer.previousPoint = event.point;
      } else if (event.phase === 'press' && (this.#tool === 'fill' || this.#tool === 'zap'))
        this.#applyRecord(toolRecord(this.#tool === 'fill' ? 5 : 6,x,y,[-1,-1,-1,-1],argb));
    }
    return this.snapshot();
  }

  endEditing() { this.#tool = 'pan'; }

  undo() {
    if (this.#undo.length) { this.#redo.unshift(this.#undo.pop());this.#replay(); }
    return this.snapshot();
  }

  redo() {
    if (this.#redo.length) { const record = this.#redo.shift();this.#execute(record);this.#undo.push(record); }
    return this.snapshot();
  }

  revert() {
    this.#pixels.set(this.#base);this.#redo = this.#undo.concat(this.#redo);this.#undo = [];
    return this.snapshot();
  }

  #replay() {
    this.#pixels.set(this.#base);
    for (const record of this.#undo) this.#execute(record);
  }

  #inBounds(x,y) { return x >= 0 && y >= 0 && x < this.#width && y < this.#height; }

  #write(pixel,color) {
    for (let channel = 0; channel < 4; channel++) this.#pixels[pixel*4+channel] = (color>>>(channel*8))&255;
  }

  // Collect before painting: fill and zap share the same four-connected
  // component, and zap also needs the boundary's distinct packed colors.
  #component(seed) {
    const queue = new Int32Array(this.#width*this.#height),seen = new Uint8Array(queue.length),neighbors = new Set();
    const source = packedColor(this.#pixels,seed*4);
    let head = 0,tail = 1;
    queue[0] = seed;seen[seed] = 1;
    const visit = pixel => {
      const color = packedColor(this.#pixels,pixel*4);
      if (color !== source) neighbors.add(color);
      else if (!seen[pixel]) { seen[pixel] = 1;queue[tail++] = pixel; }
    };
    while (head < tail) {
      const pixel = queue[head++],x = pixel%this.#width;
      if (x > 0) visit(pixel-1);
      if (x+1 < this.#width) visit(pixel+1);
      if (pixel >= this.#width) visit(pixel-this.#width);
      if (pixel+this.#width < queue.length) visit(pixel+this.#width);
    }
    return {pixels:queue.subarray(0,tail),neighbors:Array.from(neighbors).sort((first,second) => first-second)};
  }

  #sourceColor(pixel) {
    if (this.#backgroundMode !== 1 && this.#backgroundMode !== 2) return packedColor(this.#source,pixel*4);
    const alpha = this.#source[pixel*4+3]/255,remaining = 1-alpha;
    const backgroundAlpha = this.#background[3],bytes = new Uint8Array(4);
    bytes[3] = Math.trunc((remaining*backgroundAlpha+alpha)*255);
    for (let channel = 0; channel < 3; channel++) bytes[channel] = Math.trunc(
      ((this.#source[pixel*4+channel]/255)*alpha+remaining*this.#background[channel]*backgroundAlpha)*255);
    return packedColor(bytes,0);
  }

  #execute({kind,x,y,coordinates,argb}) {
    if (kind === 3) {
      const from = coordinates.slice(0,2),to = coordinates.slice(2);
      if (!this.#inBounds(...from) || !this.#inBounds(...to)) return false;
      const channels = argb.slice().reverse().map(value => Math.round(value*65535));
      return strokeSamples(from,to,(column,row) => {
        const offset = (row*this.#width+column)*4;
        const changed = channels.some((channel,index) => channel !== this.#pixels[offset+index]*257);
        if (changed) for (let channel = 0; channel < 4; channel++) this.#pixels[offset+channel] = channels[channel]>>>8;
        return changed;
      });
    }
    if (!this.#inBounds(x,y)) return false;
    const seed = y*this.#width+x;
    if (kind === 5) {
      const bytes = Uint8Array.from(argb.slice().reverse(),value => Math.round(value*65535)>>>8);
      const target = packedColor(bytes,0);
      if (target === packedColor(this.#pixels,seed*4)) return false;
      for (const pixel of this.#component(seed).pixels) this.#write(pixel,target);
      return true;
    }
    const {pixels,neighbors} = this.#component(seed);
    if (!neighbors.length) throw new Error('Zap has no neighboring color; native behavior for this case is not established.');
    for (const pixel of pixels) {
      const source = this.#sourceColor(pixel);
      let chosen = neighbors[0],minimum = storeFloat(zapDistance(chosen,source));
      // Native iteration includes the first entry again after storing the
      // initial minimum as float32. Removing it changes mixed-precision ties.
      for (const neighbor of neighbors) {
        const distance = zapDistance(neighbor,source);
        if (distance < fixedDouble(minimum)) { chosen = neighbor;minimum = storeFloat(distance); }
      }
      this.#write(pixel,chosen);
    }
    return true;
  }
}
