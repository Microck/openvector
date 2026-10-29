import { binaryParts, binaryDouble, roundSignificand, coordinateInt32 } from './extended-arithmetic.mjs';
import { fixedDecimal, streamFixedDecimal, generalRgbDecimal } from './decimal-format.mjs';

// Internal output kernels receive completed fitted geometry. They read the
// shared graph and return owned path/order arrays without moving its nodes.
function colorOrder(regions) {
  const classes = [], byColor = new Map();
  for (const [region, shape] of regions.entries()) {
    const [blue, green, red, alpha] = shape.bgra;
    const key = blue | green << 8 | red << 16 | alpha << 24;
    let index = byColor.get(key);
    if (index === undefined) {
      index = classes.length; byColor.set(key, index); classes.push([]);
    }
    classes[index].push(region);
  }
  return classes.flatMap(group => group.map((region, index) => {
    let flag = 3;
    if (group.length === 1) flag = 2;
    else if (index === 0) flag = 0;
    else if (index === group.length - 1) flag = 1;
    return [region, flag];
  }));
}

export function assembleOutputShapes(geometry) {
  const { regions } = geometry;
  const children = Array.from({ length:regions.length }, () => []);
  for (const [region, shape] of regions.entries()) {
    if (shape.parent !== -1) children[shape.parent].push(region);
  }
  const holes = children.map((nested, parent) => {
    const runs = [];
    for (const child of nested) {
      const edges = regions[child].edges;
      let run = [];
      for (let edge = 0; edge < edges.length; edge++) {
        if (edges[edge][2] === parent) run.push([child, edge]);
        else if (run.length > 0) { runs.push(run); run = []; }
      }
      if (run.length > 0) runs.push(run);
    }
    const following = new Int32Array(runs.length).fill(-1);
    const byStart = new Map();
    for (const [index, run] of runs.entries()) {
      const [region, edge] = run[0];
      // Later runs replace earlier entries, preserving the native scan's
      // last-match rule at junctions while avoiding a quadratic scan.
      byStart.set(regions[region].edges[edge][3], index);
    }
    for (const [index, run] of runs.entries()) {
      const [region, edge] = run[run.length - 1];
      const candidate = byStart.get(regions[region].edges[edge][4]);
      if (candidate !== undefined) following[index] = candidate;
    }
    const paths = [];
    for (let index = 0; index < runs.length; index++) {
      if (following[index] < 0) continue;
      const path = [];
      let current = index;
      while (following[current] >= 0) {
        for (const pair of runs[current]) path.push(pair);
        const next = following[current];
        following[current] = -1; current = next;
      }
      paths.push(path);
    }
    return paths;
  });
  return { children, holes, colorOrder:colorOrder(regions) };
}

/** Original hidden-bit range rule, including the default visible tail. */
export function isOutputRegionVisible(hiddenSize, hiddenWords, region) {
  return region >= hiddenSize || ((1 << (region & 31)) & hiddenWords[region >>> 5]) === 0;
}

function regionContainsPoint({nodes,regions},region,x,y) {
  const contour = regions[region].contour;
  let previous = nodes[contour.at(-1)],inside = false;
  for (const node of contour) {
    const current = nodes[node];
    if ((previous.y > y) !== (current.y > y) &&
        x < (y-previous.y)*(current.x-previous.x)/(current.y-previous.y)+previous.x) inside = !inside;
    previous = current;
  }
  return inside;
}

/** Native 0x488460: test the label's live smoothed boundary first, then visit
 * distinct neighboring labels in the original expanding-ring order. Fitted
 * Bezier control points and visibility bits do not participate in hit testing. */
export function findOutputRegion({width,height,indices},geometry,point) {
  const [x,y] = point,[column,row] = point.map(coordinateInt32);
  if (column < 0 || column >= width || row < 0 || row >= height) return -1;
  const initial = indices[row*width+column];
  if (regionContainsPoint(geometry,initial,x,y)) return initial;
  const visited = new Set([initial]),limit = Math.max(width-column,column,height-row,row);
  for (let radius = 1; radius < limit; radius++) {
    let step = 1;
    for (let horizontal = -radius; horizontal <= radius; horizontal++) {
      for (let vertical = -radius; vertical <= radius; vertical += step) {
        const candidateX = column+horizontal,candidateY = row+vertical;
        if (candidateX < 0 || candidateX >= width || candidateY < 0 || candidateY >= height) continue;
        const region = indices[candidateY*width+candidateX];
        if (!visited.has(region)) {
          visited.add(region);
          if (regionContainsPoint(geometry,region,x,y)) return region;
        }
      }
      step = horizontal < radius ? radius*2 : 1;
    }
  }
  return -1;
}

/** Copy a cubic's four coordinate pairs in reverse order. */
export const reverseCubicPoints = points => [points[6], points[7], points[4], points[5], points[2], points[3], points[0], points[1]];

/** Native save progress uses the float32 segment at 0xa22ea0, not a
 * completed-row ratio. All 51 integer outcomes are separately captured. */
const exportProgressBegin = Math.fround(0.056559), exportProgressEnd = Math.fround(0.96322);
export function exportProgressPercent(phase) {
  return Math.trunc((exportProgressBegin + (exportProgressEnd - exportProgressBegin) * (phase / 50)) * 100 + 0.5);
}

/** Walk native exporter order without moving shared geometry. Cubic callbacks
 * receive oriented points, start kind, then the native direction flag. Start
 * kinds are 0 for continuation, 1 for an outer path and 2 for a hole. */
export function walkVectorOutput(geometry, { shapeMode, hiddenSize, hiddenWords, onProgress }, writer) {
  const { regions, nodes, cubics } = geometry;
  const curves = new Map(cubics.map(curve => [curve.index, curve.points]));
  const { holes, colorOrder } = assembleOutputShapes(geometry);
  const emitEdge = (region, edge, reversed, startKind) => {
    if (edge[1] === 0) {
      const contour = regions[region].contour;
      const start = nodes[contour[edge[0]]], end = nodes[contour[(edge[0] + 1) % contour.length]];
      writer.line(reversed ? end : start, reversed ? start : end, startKind);
    } else {
      const backwards = (edge[1] === 2) !== reversed, points = curves.get(edge[0]);
      writer.cubic(backwards ? reverseCubicPoints(points) : points, startKind, backwards);
    }
  };
  let previousPhase = -1;
  for (let row = 0; row < regions.length; row++) {
    if (onProgress && (row & 3) === 0) {
      const phase = Math.trunc(row * 50 / regions.length + 0.5);
      if (phase !== previousPhase) { previousPhase = phase;onProgress(exportProgressPercent(phase)); }
    }
    const region = shapeMode === 2 ? colorOrder[row][0] : row;
    const flag = shapeMode === 2 ? colorOrder[row][1] : 3;
    const shape = regions[region];
    if (!writer.acceptColor(shape.bgra)) continue;
    if (shapeMode === 2 && (flag === 0 || flag === 2)) writer.beginGroup(shape.bgra);
    if (isOutputRegionVisible(hiddenSize, hiddenWords, region)) {
      const hasHoles = shapeMode !== 0 && holes[region].length > 0;
      writer.beginShape(shape.bgra, hasHoles);
      for (const [index, edge] of shape.edges.entries()) emitEdge(region, edge, false, index === 0 ? 1 : 0);
      if (shapeMode !== 0) {
        for (const path of holes[region]) {
          for (let index = path.length - 1; index >= 0; index--) {
            const [child, edge] = path[index];
            emitEdge(child, regions[child].edges[edge], true, index === path.length - 1 ? 2 : 0);
          }
        }
      }
      writer.endShape(shape.bgra, hasHoles);
    }
    if (shapeMode === 2 && (flag === 1 || flag === 2)) writer.endGroup(shape.bgra);
  }
}

const layouts = ['stacked', 'cutouts', 'grouped-cutouts'];
const dimension = value => Number.isInteger(value) && value > 0 && value <= 0x7fffffff;
const identifier = (value, length) => Number.isInteger(value) && value >= 0 && value < length;

/** The in-memory API consumes the same ordered text as a file writer. Keeping
 * collection outside each serializer preserves bytes already emitted on throw. */
export function collectVectorText(serialize, input) {
  let text = '';
  serialize(input, chunk => { text += chunk; });
  return text.replaceAll('\n', '\r\n');
}

export function validateVectorTextWriter(write, format) {
  if (typeof write !== 'function') throw new TypeError(`${format} output writer must be a synchronous function.`);
}

function validateFittedGeometry(geometry, format) {
  if (!Array.isArray(geometry?.nodes) || geometry.nodes.length < 1 ||
      !Array.isArray(geometry.regions) || geometry.regions.length < 1 || !Array.isArray(geometry.cubics) ||
      !Number.isInteger(geometry.cubicStorageSlots) || geometry.cubicStorageSlots < 0 ||
      geometry.cubicStorageSlots > 0x7fffffff) throw new Error(`${format} needs a completed fitted graph.`);
  const { nodes, regions, cubics, cubicStorageSlots } = geometry;
  for (const node of nodes) {
    if (!Number.isFinite(node?.x) || !Number.isFinite(node.y)) throw new Error(`${format} needs finite node coordinates.`);
  }
  const curves = new Set();
  for (const curve of cubics) {
    if (!identifier(curve?.index, cubicStorageSlots) || curves.has(curve.index) ||
        !Array.isArray(curve.points) || curve.points.length !== 8 || curve.points.some(value => !Number.isFinite(value))) {
      throw new Error(`${format} needs unique initialized cubic IDs and eight finite coordinates per curve.`);
    }
    curves.add(curve.index);
  }
  for (const shape of regions) {
    if (!Array.isArray(shape?.bgra) || shape.bgra.length !== 4 ||
        shape.bgra.some(value => !Number.isInteger(value) || value < 0 || value > 255) ||
        !(shape.parent === -1 || identifier(shape.parent, regions.length)) ||
        !Array.isArray(shape.contour) || shape.contour.length < 3 ||
        shape.contour.some(node => !identifier(node, nodes.length)) ||
        !Array.isArray(shape.edges) || shape.edges.length < 1) {
      throw new Error(`${format} needs byte BGRA colors, existing parents, contours and fitted edges.`);
    }
    for (const edge of shape.edges) {
      if (!Array.isArray(edge) || edge.length !== 7 || ![0, 1, 2].includes(edge[1]) ||
          !(edge[2] === -1 || identifier(edge[2], regions.length)) ||
          !identifier(edge[5], shape.contour.length) || !identifier(edge[6], shape.contour.length) ||
          edge[3] !== shape.contour[edge[5]] || edge[4] !== shape.contour[edge[6]] ||
          (edge[1] === 0 ? edge[0] !== edge[5] || edge[6] !== (edge[5] + 1) % shape.contour.length : !curves.has(edge[0]))) {
        throw new Error(`${format} needs consistent fitted edge samples, shared node IDs and initialized curve references.`);
      }
    }
  }
}

/** Validate common export inputs once; writers read the caller graph only. */
export function readVectorOutput(input, format) {
  const { geometry, width, height, outputWidth = width, outputHeight = height,
    layout = 'stacked', thinStroke = false, hiddenRegions = [], onProgress } = input ?? {};
  if (onProgress !== undefined && typeof onProgress !== 'function') throw new Error(`${format} onProgress must be a function.`);
  if (!dimension(width) || !dimension(height) || !dimension(outputWidth) || !dimension(outputHeight) ||
      (input.outputWidth === undefined) !== (input.outputHeight === undefined)) {
    throw new Error(`${format} needs positive int32 source dimensions and both output dimensions or neither.`);
  }
  const shapeMode = layouts.indexOf(layout);
  if (shapeMode < 0 || typeof thinStroke !== 'boolean') {
    throw new Error(`${format} layout must be stacked, cutouts or grouped-cutouts; thinStroke must be true or false.`);
  }
  validateFittedGeometry(geometry, format);
  if (!Array.isArray(hiddenRegions) || hiddenRegions.some(region => !identifier(region, geometry.regions.length))) {
    throw new Error(`${format} hiddenRegions must list existing nonnegative integer region IDs.`);
  }
  const hiddenSize = hiddenRegions.length === 0 ? 0 : geometry.regions.length;
  const hiddenWords = new Uint32Array(Math.ceil(hiddenSize / 32));
  for (const region of hiddenRegions) hiddenWords[region >>> 5] |= 1 << (region & 31);
  return { geometry, width, height, outputWidth, outputHeight, shapeMode, thinStroke, hiddenSize, hiddenWords, onProgress };
}

// Divide a dyadic register value by an integer, rounding its significand to
// 53 bits before the final double store. Register exponents remain unbounded.
function divideCoordinate(numerator, exponent, denominator) {
  if (numerator === 0n) return 0;
  const negative = numerator < 0n;
  if (negative) numerator = -numerator;
  let highest = numerator.toString(2).length - denominator.toString(2).length;
  const belowPower = highest >= 0 ? numerator < denominator << BigInt(highest) :
    numerator << BigInt(-highest) < denominator;
  if (belowPower) highest--;
  const shift = highest - 52;
  if (shift > 0) denominator <<= BigInt(shift); else numerator <<= BigInt(-shift);
  let quotient = numerator / denominator;
  const remainder = numerator % denominator;
  if (remainder * 2n > denominator || (remainder * 2n === denominator && (quotient & 1n))) quotient++;
  return binaryDouble(negative ? -quotient : quotient, exponent + shift);
}

function transformOutputCoordinate(value, sourceSize, outputSize, flipped, divisor = sourceSize) {
  // Curve arithmetic can produce nonfinite values even from finite controls.
  // IEEE operations preserve the native signs and quiet signaling NaNs here.
  if (!Number.isFinite(value)) return ((flipped ? sourceSize - value : value) * outputSize) / divisor;
  const parts = binaryParts(Math.abs(value));
  let significand = value < 0 ? -parts.significand : parts.significand, exponent = parts.exponent;
  if (flipped) {
    const commonExponent = Math.min(exponent, 0);
    significand = roundSignificand((BigInt(sourceSize) << BigInt(-commonExponent)) -
      (significand << BigInt(exponent - commonExponent)), 53);
    exponent = commonExponent;
  }
  significand = roundSignificand(significand * BigInt(outputSize), 53);
  if (divisor === 0) return significand === 0n ? -Number.NaN : significand < 0n ? -Infinity : Infinity;
  if (significand === 0n) return (!flipped && Object.is(value, -0)) !== (divisor < 0) ? -0 : 0;
  if (divisor < 0) significand = -significand;
  return divideCoordinate(significand, exponent, BigInt(Math.abs(divisor)));
}

/** DXF uses inches and signed int32 multiplication for its denominators. */
export function transformDxfPoint({ width, height, outputWidth, outputHeight }, x, y) {
  return [transformOutputCoordinate(x, width, outputWidth, false, Math.imul(width, 72)),
    transformOutputCoordinate(y, height, outputHeight, true, Math.imul(height, 72))];
}

/** Original EMF coordinates truncate through signed 64-bit storage, then use
 * its low 32 bits. Invalid/overflow stores have a zero low word. Products too
 * large for that storage do not need to be rounded into binary64 first. */
export function transformEmfPoint({ height }, x, y) {
  const integer = value => value >= -(2 ** 63) && value < 2 ** 63 ? Math.trunc(value) | 0 : 0;
  return [integer(x * 35.27777778), integer((height - y) * 35.27777778)];
}

/** GDI rounds logical inputs and matrix products to float32, converts to
 * signed 28.4 fixed point with ties away from zero, then rounds device pixels
 * with (fixed+8)>>4. Header conversion wraps bound*100 before MulDiv. */
export function createEmfDeviceMapper({ pixels, millimeters }) {
  const scales = pixels.map((value, axis) => Math.fround(value / (millimeters[axis] * 100)));
  const coordinate = (value, axis) => {
    const product = Math.fround(Math.fround(value) * (axis === 0 ? scales[0] : -scales[1]));
    const magnitude = Math.floor(Math.abs(product * 16) + 0.5);
    const fixed = product < 0 ? -magnitude : magnitude;
    if (!(fixed >= -0x80000000 && fixed <= 0x7fffffff)) {
      throw new Error('EMF device coordinates exceed signed 28.4 storage.');
    }
    return Math.floor((fixed + 8) / 16);
  };
  return {
    point:(x, y) => [coordinate(x, 0), coordinate(y, 1)],
    frame:bounds => bounds.map((bound, index) => {
      const axis = index & 1, product = BigInt(Math.imul(bound, 100)) * BigInt(millimeters[axis]);
      const magnitude = product < 0n ? -product : product, divisor = BigInt(pixels[axis]);
      let quotient = magnitude / divisor;
      if ((magnitude % divisor) * 2n >= divisor) quotient++;
      if (product < 0n) quotient = -quotient;
      return quotient < -0x80000000n || quotient > 0x7fffffffn ? -1 : Number(quotient);
    }),
  };
}

/** Match the raw CRT conversion and trailing-zero trimming. Check the raw
 * length first: the native stack buffer overflows before it trims the field. */
export function formatDxfCoordinate(value) {
  const field = fixedDecimal(value, 16);
  if (field.length >= 256) throw new Error('DXF coordinate exceeds the reference writer\'s 256-byte numeric field.');
  let end = field.length;
  const firstFraction = field.indexOf('.') + 1;
  while (end > firstFraction + 1 && field[end - 1] === '0') end--;
  return field.slice(0, end);
}

/** Scaled path operands and RGB share the native stream's fixed/general
 * state. Fixed header fields and coordinates set that flag for the file. */
export function createScaledOutputFormatter({ width, height, outputWidth, outputHeight }) {
  const xCoordinates = new Map(), yCoordinates = new Map();
  const fixedComponents = new Map(), generalComponents = new Map();
  let fixedFormatting = false;
  const coordinate = (value, flipped) => {
    const cache = flipped ? yCoordinates : xCoordinates;
    if (!cache.has(value)) cache.set(value, streamFixedDecimal(
      transformOutputCoordinate(value, flipped ? height : width, flipped ? outputHeight : outputWidth, flipped), 2));
    return cache.get(value);
  };
  return {
    fixed(value, precision) { fixedFormatting = true; return streamFixedDecimal(value, precision); },
    point(x, y) { fixedFormatting = true; return `${coordinate(x, false)} ${coordinate(y, true)}`; },
    component(byte) {
      const components = fixedFormatting ? fixedComponents : generalComponents;
      if (!components.has(byte)) {
        const value = Math.fround(byte * Math.fround(1 / 255));
        components.set(byte, fixedFormatting ? streamFixedDecimal(value, 3) : generalRgbDecimal(value));
      }
      return components.get(byte);
    },
  };
}
