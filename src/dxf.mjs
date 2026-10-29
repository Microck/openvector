import { readVectorOutput, walkVectorOutput, transformDxfPoint, formatDxfCoordinate, reverseCubicPoints, collectVectorText, validateVectorTextWriter } from './vector-output.mjs';
import { writeDxfField, dxfHandle, openDxf, closeDxf } from './dxf-container.mjs';

const curveModes = ['splines', 'sparse-lines', 'dense-lines'];
const colorName = color => 'color#' + [color[2], color[1], color[0], color[3]]
  .map(channel => channel.toString(16).padStart(2, '0')).join('');

function readPalette(palette) {
  if (!(Array.isArray(palette) || palette instanceof Float32Array) || palette.length < 4 || palette.length % 4 !== 0) {
    throw new Error('DXF needs the original normalized float32 BGRA palette, with four channels per color.');
  }
  const owned = new Float32Array(palette.length);
  for (const [index, value] of palette.entries()) {
    if (!Number.isFinite(value) || value < 0 || value > 1) throw new Error('DXF palette channels must be finite numbers in [0,1].');
    owned[index] = value;
  }
  return owned;
}

// Native 0x479e10 stores each weighted point before adding from P0 through
// P3. Weight multiplication order and k*(1/29) preserve its double rounding.
function sampleCubic(points, sample) {
  const t = sample * 0.034482758620689655, u = 1 - t;
  const t2 = t * t, u2 = u * u;
  const weights = [u2 * u, (u2 * t) * 3, (u * t2) * 3, t2 * t];
  const coordinate = axis => ((points[axis] * weights[0] + points[axis + 2] * weights[1]) +
    points[axis + 4] * weights[2]) + points[axis + 6] * weights[3];
  return [coordinate(0), coordinate(1)];
}

function cubicLines(curve, backwards, mode, emitLine) {
  const points = backwards ? reverseCubicPoints(curve) : curve;
  const samples = Array.from({ length:30 }, (_, index) => sampleCubic(points, index));
  const keep = new Uint8Array(30); keep[0] = keep[29] = 1;
  const threshold = mode === 1 ? 0.002 : 0.0005;
  let accumulated = 0;
  for (let index = 1; index < 29; index++) {
    const beforeX = samples[index][0] - samples[index - 1][0], beforeY = samples[index][1] - samples[index - 1][1];
    const afterX = samples[index + 1][0] - samples[index][0], afterY = samples[index + 1][1] - samples[index][1];
    const x = afterX - beforeX, y = afterY - beforeY;
    accumulated = (x * x + y * y) + accumulated;
    // Unordered comparisons follow the native jbe path and drop the sample.
    if (accumulated > threshold) { keep[index] = 1; accumulated = 0; }
  }
  let start = [curve[0], curve[1]];
  // A flagged curve is sampled in reverse, then those samples are walked
  // backward. Emitted endpoints retain the dispatcher's oriented order.
  for (let index = backwards ? 28 : 1; backwards ? index >= 0 : index < 30; index += backwards ? -1 : 1) {
    if (keep[index]) { emitLine(start, samples[index]); start = samples[index]; }
  }
}

/** Serialize native DXF layers, entities and handles from a fitted document. */
export function exportDxf(input) {
  return collectVectorText(serializeDxf, input);
}

export function serializeDxf(input, write) {
  validateVectorTextWriter(write, 'DXF');
  const options = readVectorOutput(input, 'DXF'), palette = readPalette(input.palette);
  const { curveMode = 'dense-lines' } = input;
  const mode = curveModes.indexOf(curveMode);
  if (mode < 0) throw new Error('DXF curveMode must be splines, sparse-lines or dense-lines.');
  const { geometry, shapeMode } = options;
  const shapeWidth = String(geometry.regions.length).length;
  const shapeName = index => 'shape' + String(index).padStart(shapeWidth, '0');
  const layerNames = shapeMode === 2 ? Array.from({ length:palette.length / 4 }, (_, index) =>
    colorName(Array.from(palette.subarray(index * 4, index * 4 + 4), value => Math.trunc(value * 255 + 0.5)))) :
    Array.from({ length:geometry.regions.length }, (_, index) => shapeName(index));
  const field = (code, value) => writeDxfField(write, code, value);
  openDxf(layerNames, write);
  let nextHandle = 0x47 + layerNames.length, shapeIndex = 0, layer;
  const attributes = () => { field(8, layer); field(62, 256); field(370, -1); field(6, 'BYLAYER'); };
  const formattedPoints = new Map();
  const point = (x, y, xCode = 10) => {
    let column = formattedPoints.get(x);
    if (column === undefined) { column = new Map(); formattedPoints.set(x, column); }
    let formatted = column.get(y);
    if (formatted === undefined) {
      const transformed = transformDxfPoint(options, x, y);
      formatted = transformed.map(formatDxfCoordinate); column.set(y, formatted);
    }
    field(xCode, formatted[0]); field(xCode + 10, formatted[1]); field(xCode + 20, '0.0');
  };
  const line = (start, end) => {
    field(0, 'LINE'); field(5, dxfHandle(nextHandle++)); field(100, 'AcDbEntity');
    field(100, 'AcDbLine'); attributes(); point(...start); point(...end, 11);
  };
  walkVectorOutput(geometry, options, {
    acceptColor:color => color[3] > 50,
    // Native group and shape-end hooks emit no records in DXF.
    beginGroup() {},
    endGroup() {},
    beginShape(color) {
      layer = shapeMode === 2 ? colorName(color) : shapeName(shapeIndex++);
    },
    endShape() {},
    line(start, end) { line([start.x, start.y], [end.x, end.y]); },
    cubic(curve, _startKind, backwards) {
      if (mode !== 0) { cubicLines(curve, backwards, mode, line); return; }
      // Splines write the dispatcher's already oriented control points.
      field(0, 'SPLINE'); field(5, dxfHandle(nextHandle++)); attributes();
      field(100, 'AcDbEntity'); field(100, 'AcDbSpline'); field(70, 8); field(71, 3);
      field(72, 8); field(73, 4); field(74, 0);
      for (let index = 0; index < 8; index++) field(40, index < 4 ? '0.0' : '1.0');
      for (let index = 0; index < 8; index += 2) point(curve[index], curve[index + 1]);
    },
  });
  closeDxf(nextHandle, write);
}
