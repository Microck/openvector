import {formatFixedOne} from './numeric-text.mjs';

export const bitmapScaleButtons=Object.freeze([1,2,4]);
export const bitmapScaleMore=Object.freeze([5,6,8,10,12,16,20,24]);

const modes=['basic','advanced'],axes=['width','height'];
const clamp=value=>Math.max(1,Math.min(10000,value));

// The native x87 conversion truncates to signed int64, then keeps its low
// signed int32. Invalid conversions produce the integer-indefinite low word.
function dimension(value) {
  if(!Number.isFinite(value) || value<-(2**63) || value>=2**63)return 0;
  return Number(BigInt.asIntN(32,BigInt(Math.trunc(value))));
}

/** Work on a small draft so a correction cycle never corrupts the owner.
 * A corrected axis/value determines its following state, so repeating one
 * proves that this command cannot settle through the native call sequence. */
function settle(state,mode,field,value) {
  const pane=state[mode],seen=new Set();
  function resolve(axis,requested) {
    pane[axis]=clamp(requested);
    if(pane[axis]!==requested) {
      const correction=axis+':'+pane[axis];
      if(seen.has(correction))throw new Error('Bitmap dimensions cannot settle within 1 to 10000 pixels. Use the CLI bitmap scale option for this aspect ratio.');
      seen.add(correction);update(axis,pane[axis]);
    }
  }
  function update(axis,requested) {
    if(axis==='scale') {
      if(state.scale!==requested)state.scale=requested;
      resolve('width',dimension(state.original.width*state.scale+0.5));
      resolve('height',dimension(state.original.height*state.scale+0.5));
    } else {
      const scale=requested/state.original[axis];
      if(state.scale!==scale)state.scale=scale;
      const other=axis==='width'?'height':'width';
      resolve(other,dimension(state.original[other]*state.scale+0.5));
    }
  }
  if(field!=='scale') {
    value=clamp(value);pane[field]=value;
  }
  update(field,value);pane.percent=formatFixedOne(state.scale*100);
}

/** Native bitmap fields are separate presentations of a shared model scale.
 * A model-only scale update deliberately leaves both field presentations. */
export class BitmapSize {
  #state={scale:1,original:{width:150,height:250},basic:{width:150,height:250,percent:'100.0'},advanced:{width:150,height:250,percent:'100.0'}};
  constructor(scale=1) {
    // Preference restoration runs the size handlers against the constructor's
    // placeholder image, unlike later model-only setScale assignments.
    for(const mode of modes)settle(this.#state,mode,'scale',scale);
  }
  get scale() {return this.#state.scale;}
  snapshot() {return structuredClone(this.#state);}
  setScale(value) {
    if(typeof value!=='number')throw new Error('Bitmap scale must be numeric.');
    if(this.#state.scale!==value)this.#state.scale=value;
    return this.snapshot();
  }
  sourceChanged(image) {
    if(image===null)return this.snapshot();
    if(!image || !axes.every(axis=>Number.isInteger(image[axis]) && image[axis]>0 && image[axis]<=2147483647))
      throw new Error('Bitmap source dimensions must be positive signed integers.');
    const next=structuredClone(this.#state);next.original={width:image.width,height:image.height};
    for(const mode of modes)settle(next,mode,'scale',next.scale);
    this.#state=next;return this.snapshot();
  }
  command(input) {
    if(!input || typeof input!=='object' || Array.isArray(input) || !modes.includes(input.mode))
      throw new Error('Choose basic or advanced bitmap size controls.');
    let {field,value}=input;
    if(input.command==='preset' || input.command==='view-scale') {
      if(Object.keys(input).length!==3 || !Number.isFinite(value) || value<=0 ||
          (input.command==='preset' && !bitmapScaleButtons.includes(value) && !bitmapScaleMore.includes(value)))
        throw new Error('Choose a bitmap scale preset or a positive finite view scale.');
      field='scale';
    } else if(input.command==='commit') {
      if(Object.keys(input).length!==4 || !['scale',...axes].includes(field) || typeof value!=='number' ||
          (field!=='scale' && (!Number.isInteger(value) || value< -2147483648 || value>2147483647)))
        throw new Error('Commit a numeric scale or signed integer width/height.');
    } else if(input.command==='step') {
      if(Object.keys(input).length!==5 || !axes.includes(field) || ![-1,1].includes(input.direction) || !['small','large'].includes(input.size))
        throw new Error('Choose a width/height, direction -1 or 1, and small or large step.');
      const increment=input.size==='small'?5:25,current=this.#state[input.mode][field];
      value=input.direction===1?(Math.trunc(current/increment)+1)*increment:
        10000-(Math.trunc((10000-current)/increment)+1)*increment;
    } else throw new Error('Choose commit, step, preset or view-scale for bitmap sizing.');
    const next=structuredClone(this.#state);settle(next,input.mode,field,value);
    this.#state=next;return this.snapshot();
  }
}

/** Text is a GUI producer concern. The owner receives the same typed values
 * as the native integer and double slots, including zero on parse failure. */
export function readBitmapSizeText(input,numeric) {
  if(!input || typeof input!=='object' || Array.isArray(input) || Object.keys(input).length!==4 ||
      input.command!=='commit' || !['width','height','percent'].includes(input.field) || !Object.hasOwn(input,'text'))
    throw new Error('Commit width, height or percent text with a bitmap mode.');
  const percent=input.field==='percent';
  const parsed=percent?numeric.percent(input.text):numeric.integer(input.text);
  return {command:'commit',mode:input.mode,field:percent?'scale':input.field,value:parsed.value};
}
