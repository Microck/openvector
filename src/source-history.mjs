import { binaryParts } from './extended-arithmetic.mjs';

const fileCapacity = 3;
const directoryCapacity = 4;

// QString's one-decimal formatter rounds the exact binary value to nearest
// with even ties. Decimal string rounding would lose cases such as 1.05.
function oneDecimal(value) {
  const {significand,exponent} = binaryParts(value);
  let numerator = significand*10n,denominator = 1n;
  if (exponent < 0) denominator <<= BigInt(-exponent);
  else numerator <<= BigInt(exponent);
  let rounded = numerator/denominator;
  const remainder = numerator%denominator*2n;
  if (remainder > denominator || remainder === denominator && (rounded&1n)) rounded++;
  return `${rounded/10n}.${rounded%10n}`;
}

/** Original file metadata uses decimal units and deliberately names its
 * third scaled unit TB. Keep both the strict threshold and rounding order. */
export function formatFileSize(bytes) {
  if (!Number.isSafeInteger(bytes) || bytes < 0) throw new Error('File size must be a nonnegative safe integer.');
  const units = ['B','kB','MB','TB'];
  let value = bytes,index = 0;
  while (value > 1000 && index < 3) {value *= 0.001;index++;}
  return `${oneDecimal(value)} ${units[index]}`;
}

/** Own the three visible recent-file rows. Disk access, persistence and
 * thumbnail pixels belong to the source host; this owner never rewrites paths. */
export class SourceHistory {
  #files = [];

  static fromSnapshot(snapshot) {
    if (snapshot?.capacity !== fileCapacity || !Array.isArray(snapshot.files) || snapshot.files.length > fileCapacity)
      throw new Error('Invalid recent-file history.');
    const history = new SourceHistory(),names = new Set();
    history.#files = snapshot.files.map(entry=>{
      if (typeof entry?.filename !== 'string' || entry.filename.length === 0 || names.has(entry.filename) ||
        typeof entry.description !== 'string' || !Array.isArray(entry.thumbnailDimensions) || entry.thumbnailDimensions.length !== 2)
        throw new Error('Invalid recent-file entry.');
      const dimensions=entry.thumbnailDimensions;
      if (dimensions.some(value=>!Number.isInteger(value) || value<0 || value>32) ||
        (dimensions.includes(0) ? dimensions.some(value=>value!==0) : Math.max(...dimensions)!==32))
        throw new Error('Invalid recent-file thumbnail dimensions.');
      names.add(entry.filename);
      return {filename:entry.filename,description:entry.description,thumbnailDimensions:[...dimensions]};
    });
    return history;
  }

  static thumbnailDimensions(width,height) {
    for (const dimension of [width,height])
      if (!Number.isInteger(dimension) || dimension < 1 || dimension > 0x7fffffff)
        throw new Error('Image dimensions must be positive signed 32-bit integers.');
    // QSize truncates the short dimension; an empty size yields a null pixmap.
    const dimensions=width>=height ? [32,Math.floor(height*32/width)] : [Math.floor(width*32/height),32];
    if (dimensions.includes(0)) dimensions.fill(0);
    return dimensions;
  }

  snapshot() {
    return {capacity:fileCapacity,files:this.#files.map(entry=>({...entry,thumbnailDimensions:[...entry.thumbnailDimensions]}))};
  }

  add({filename,width,height,bytes}) {
    if (typeof filename !== 'string' || filename.length === 0) throw new Error('Choose a filename for the recent image.');
    const thumbnailDimensions = SourceHistory.thumbnailDimensions(width,height);
    const description = `${width}x${height}, ${formatFileSize(bytes)}`;
    const entry = {filename,description,thumbnailDimensions};
    this.#files = [entry,...this.#files.filter(previous=>previous.filename!==filename)].slice(0,fileCapacity);
    return this.snapshot();
  }

  remove(filename) {
    if (typeof filename !== 'string') throw new Error('Recent image filename must be a string.');
    this.#files = this.#files.filter(entry=>entry.filename!==filename);
    return this.snapshot();
  }

  clear() {this.#files=[];return this.snapshot();}
}

/** The directory list has its own capacity and survives clearing file rows.
 * The source host supplies directory spelling; this owner never probes disk. */
export class SourceDirectories {
  #directories=[];

  static fromSnapshot(snapshot) {
    if(snapshot?.capacity!==directoryCapacity || !Array.isArray(snapshot.directories) || snapshot.directories.length>directoryCapacity)
      throw new Error('Invalid recent-directory history.');
    const history=new SourceDirectories();
    for(const directory of snapshot.directories) {
      if(typeof directory!=='string' || directory.length===0 || history.#directories.includes(directory))
        throw new Error('Invalid recent-directory history.');
      history.#directories.push(directory);
    }
    return history;
  }

  add(directory) {
    if(typeof directory!=='string' || directory.length===0)throw new Error('Choose a recent image directory.');
    this.#directories=[directory,...this.#directories.filter(previous=>previous!==directory)].slice(0,directoryCapacity);
    return this.snapshot();
  }

  snapshot() {return {capacity:directoryCapacity,directories:[...this.#directories]};}
  clear() {this.#directories=[];return this.snapshot();}
}
