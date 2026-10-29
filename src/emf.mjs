import { readVectorOutput, walkVectorOutput, transformEmfPoint, createEmfDeviceMapper } from './vector-output.mjs';

// The original passes a singly terminated label to GDI. Its recorded UTF-16
// description continues into adjacent static data. These are observed output
// metadata bytes; the independent writer never reads the original executable.
const description = Uint8Array.from(('430072006500610074006500640020007700690074006800200056006500630074006f00720020004d0061006700690063000000' +
  'c086470080734700e0854700a089470000864700b08b4700d08d4700608f4700708647006e7000006e657770617468002f6c77207b207365746c696e657769647468207d2064656600000000')
  .match(/../g), value => Number.parseInt(value, 16));
const emptyBounds = [0, 0, -1, -1];

function record(type, ...values) {
  const bytes = new Uint8Array(8 + values.length * 4), view = new DataView(bytes.buffer);
  view.setUint32(0, type, true); view.setUint32(4, bytes.length, true);
  for (const [index, value] of values.entries()) view.setUint32(8 + index * 4, value, true);
  return bytes;
}

function cubicRecord(points) {
  const compact = points.every(point => point.every(value => value >= -32768 && value <= 32767));
  if (!compact) return record(5, ...emptyBounds, 3, ...points.flat());
  const bytes = new Uint8Array(40), view = new DataView(bytes.buffer);
  bytes.set(record(88, ...emptyBounds, 3)); view.setUint32(4, 40, true);
  for (const [index, value] of points.flat().entries()) view.setInt16(28 + index * 2, value, true);
  return bytes;
}

function readDevice(input) {
  const device = input?.referenceDevice, owned = {};
  for (const name of ['pixels', 'millimeters', 'micrometers']) {
    const pair = device?.[name];
    if (!Array.isArray(pair) || pair.length !== 2 ||
        pair.some(value => !Number.isInteger(value) || value <= 0 || value > 0x7fffffff)) {
      throw new Error('EMF referenceDevice needs pixels, millimeters and micrometers pairs of positive int32 integers.');
    }
    owned[name] = [...pair];
  }
  return owned;
}

function emfHeader({device,bounds,frame,byteLength=0,recordCount=0,handles=1}) {
  const header = new Uint8Array(236);
  header.set(record(1, ...bounds, ...frame, 0x464d4520, 0x10000,
    byteLength, recordCount, handles, 64, 108, 0,
    ...device.pixels, ...device.millimeters, 0, 0, 0, ...device.micrometers));
  header.set(description,108);new DataView(header.buffer).setUint32(4,236,true);
  return header;
}

/** Collect the same positioned writes that an EMF file owner consumes. */
export function exportEmf(input) {
  const writes = [];
  let length = 0;
  serializeEmf(input,(bytes,position) => {
    writes.push({bytes,position});length = Math.max(length,position+bytes.length);
  });
  const output = new Uint8Array(length);
  for (const {bytes,position} of writes) output.set(bytes,position);
  return output;
}

/** Whole records stay buffered until the next record would overflow 16 KiB.
 * Success drains the tail and rewrites the header. An exception does neither.
 * Consumers receive owned initialized bytes and explicit physical offsets. */
export function serializeEmf(input, write) {
  if (typeof write !== 'function') throw new TypeError('EMF output writer must be a synchronous function.');
  const options = readVectorOutput(input, 'EMF'), device = readDevice(input);
  const { geometry, width, height, thinStroke } = options;
  const mapper = createEmfDeviceMapper(device), pending = new Uint8Array(16384);
  let used = 0, written = 0, recordCount = 1;
  const flush = () => {
    if (used === 0) return;
    write(pending.slice(0,used),written);written += used;used = 0;
  };
  const append = bytes => {
    if (used+bytes.length > pending.length) flush();
    pending.set(bytes,used);used += bytes.length;
  };
  const emit = (...records) => { for (const bytes of records) { append(bytes);recordCount++; } };
  append(emfHeader({device,bounds:emptyBounds,frame:emptyBounds}));
  const dimensions = transformEmfPoint({ height }, width, 0);
  emit(record(17, 3), record(9, ...dimensions), record(11, ...dimensions));
  let nextHandle = 1, pathBounds, fileBounds;
  const point = (x, y) => {
    const logical = transformEmfPoint(options, x, y), mapped = mapper.point(...logical);
    if (pathBounds === undefined) pathBounds = [...mapped, ...mapped];
    else {
      pathBounds[0] = Math.min(pathBounds[0], mapped[0]); pathBounds[1] = Math.min(pathBounds[1], mapped[1]);
      pathBounds[2] = Math.max(pathBounds[2], mapped[0]); pathBounds[3] = Math.max(pathBounds[3], mapped[1]);
    }
    return logical;
  };
  const move = (x, y, startKind) => { if (startKind) emit(record(27, ...point(x, y))); };
  walkVectorOutput(geometry, options, {
    acceptColor:color => color[3] > 50,
    beginGroup() { emit(record(70, 28, 0x43494447, 2, 0, 0, ...dimensions, 0)); },
    endGroup() { emit(record(70, 8, 0x43494447, 3)); },
    beginShape(color) {
      const rgb = color[2] | color[1] << 8 | color[0] << 16;
      if (thinStroke) {
        emit(record(38, nextHandle, 0, 0, 0, rgb), record(37, nextHandle++));
      } else emit(record(37, 0x80000008));
      emit(record(39, nextHandle, 0, rgb, 0), record(37, nextHandle++), record(59));
      pathBounds = undefined;
    },
    endShape() {
      emit(record(60), record(62, ...pathBounds));
      if (fileBounds === undefined) fileBounds = [...pathBounds];
      else {
        fileBounds[0] = Math.min(fileBounds[0], pathBounds[0]); fileBounds[1] = Math.min(fileBounds[1], pathBounds[1]);
        fileBounds[2] = Math.max(fileBounds[2], pathBounds[2]); fileBounds[3] = Math.max(fileBounds[3], pathBounds[3]);
      }
    },
    line(start, end, startKind) {
      move(start.x, start.y, startKind); emit(record(54, ...point(end.x, end.y)));
    },
    cubic(curve, startKind) {
      move(curve[0], curve[1], startKind);
      emit(cubicRecord([point(curve[2], curve[3]), point(curve[4], curve[5]), point(curve[6], curve[7])]));
    },
  });
  if (nextHandle > 0xffff) throw new Error('EMF needs fewer than 65535 drawing objects.');
  emit(record(14, 0, 16, 20));
  const bounds = fileBounds ?? emptyBounds;
  const header = emfHeader({device,bounds,frame:mapper.frame(bounds),byteLength:written+used,recordCount,handles:nextHandle});
  flush();write(header,0);
}
