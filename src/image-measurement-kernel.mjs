import { contourAngleStep } from './arctangent.mjs';

const F = Math.fround;
const byteScale = F(1 / 255);
const kernels = new WeakMap();

// Numeric fields from original model+0x78 through model+0xf7. Identifiers and
// double weights overlap the count/background words when enough fragments
// arrive. One record preserves those writes; separate growable arrays do not.
export class PixelFragmentScratch {
  #view = new DataView(new ArrayBuffer(0x80));
  get count() { return this.#view.getInt32(0x78, true); }
  set count(value) { this.#view.setInt32(0x78, value, true); }
  get background() { return this.#view.getInt32(0x7c, true); }
  set background(value) { this.#view.setInt32(0x7c, value, true); }
  identifier(index) { return this.#view.getInt32(index * 4, true); }
  writeIdentifier(index, value) { this.#view.setInt32(index * 4, value, true); }
  area(index) { return this.#view.getFloat64(0x28 + index * 8, true); }
  writeArea(index, value) { this.#view.setFloat64(0x28 + index * 8, value, true); }
  find(identifier) {
    const count = this.count;
    for (let index = 0; index < count; index++) if (this.identifier(index) === identifier) return index;
    return -1;
  }
}

function appendRecord(model, lists, column, row, region, sample, flags, x, y, turn) {
  if (column < 0 || row < 0 || column >= model.width || row >= model.height) return;
  let perimeter = -1;
  if (flags & 1) {
    if (!(flags & 0x100)) perimeter = y === row ? F(x - column) : F(3 - (x - column));
    else perimeter = x === column ? F(4 - (y - row)) : F((y - row) + 1);
  }
  const record = { flags, sample, region, x:F(x), y:F(y), perimeter, turn:F(turn), next:null };
  const list = lists[row * model.width + column];
  if (list.tail) list.tail.next = record;
  else list.head = record;
  list.tail = record;
}

function traceEdge(model, lists, positions, region, sample, turn) {
  const contour = model.regions[region].contour;
  const first = contour[sample] * 2, last = contour[(sample + 1) % contour.length] * 2;
  const x0 = positions[first], y0 = positions[first + 1], x1 = positions[last], y1 = positions[last + 1];
  const dx = x1 - x0, dy = y1 - y0;
  let gridX = Math.floor(x0), gridY = Math.floor(y0), column = gridX, row = gridY, flags;
  if (x0 === gridX || y0 === gridY) {
    if (x0 === gridX) {
      column += dx < 0 ? -1 : 0;
      if (y0 === gridY) { row += dy < 0 ? -1 : 0; flags = 0x187; }
      else flags = 0x105;
    } else { row += dy < 0 ? -1 : 0; flags = 0x85; }
  } else flags = 4;
  appendRecord(model, lists, column, row, region, sample, flags, x0, y0, turn);
  let currentX = x0, currentY = y0;
  while (true) {
    let fractionX = 2, fractionY = 2;
    if (dx > 0) { gridX = Math.floor(currentX) + 1; fractionX = (gridX - x0) / dx; }
    else if (dx < 0) { gridX = Math.ceil(currentX) - 1; fractionX = (gridX - x0) / dx; }
    if (dy > 0) { gridY = Math.floor(currentY) + 1; fractionY = (gridY - y0) / dy; }
    else if (dy < 0) { gridY = Math.ceil(currentY) - 1; fractionY = (gridY - y0) / dy; }
    if (fractionX >= 1 && fractionY >= 1) break;
    let offsetX, offsetY;
    if (fractionY <= fractionX) {
      offsetX = 0; offsetY = -1; flags = 0x80;
      gridX = fractionY * dx + x0;
    } else {
      offsetX = -1; offsetY = 0; flags = 0x100;
      gridY = fractionX * dy + y0;
    }
    // The original temporarily selects x87 truncation mode for these casts.
    column = Math.trunc(gridX); row = Math.trunc(gridY);
    if (gridX === column && gridY === row) {
      appendRecord(model, lists, column, row, region, sample, 0x183, gridX, gridY, turn);
      offsetX = column - 1;
      appendRecord(model, lists, offsetX, row, region, sample, 0x183, gridX, gridY, turn);
      row--;
      appendRecord(model, lists, column, row, region, sample, 0x183, gridX, gridY, turn);
      flags = 0x183;
    } else {
      flags |= 1;
      appendRecord(model, lists, column, row, region, sample, flags, gridX, gridY, turn);
      row += offsetY; offsetX += column;
    }
    appendRecord(model, lists, offsetX, row, region, sample, flags, gridX, gridY, turn);
    currentX = gridX; currentY = gridY;
  }
  gridX = Math.floor(x1); gridY = Math.floor(y1);
  if (x1 === gridX || y1 === gridY) {
    const offsetX = dx <= 0 ? 0 : -1, offsetY = dy <= 0 ? 0 : -1;
    column = Math.trunc(x1); row = Math.trunc(y1);
    if (x1 === gridX) {
      if (y1 === gridY) appendRecord(model, lists, column + offsetX, row + offsetY, region, sample, 0x197, x1, y1, turn);
      else appendRecord(model, lists, column + offsetX, row, region, sample, 0x115, x1, y1, turn);
    } else appendRecord(model, lists, column, row + offsetY, region, sample, 0x95, x1, y1, turn);
  }
}

function insertCorner(first, following, column, row, position) {
  const corner = (position + 4) % 4;
  const record = { flags:0x18b, sample:-2, region:first.region, x:column + (corner === 1 || corner === 2 ? 1 : 0),
    y:row + (corner === 2 || corner === 3 ? 1 : 0), perimeter:corner, turn:0, next:following };
  first.next = record;
  return record;
}

function connectPerimeter(first, following, column, row) {
  // Stored turns are float32, but native subtraction and wrap use double.
  // Rounding the wrap to float32 can reverse the selected pixel perimeter.
  let turn = following.turn - first.turn;
  if (following.sample < first.sample) turn += 2 * Math.PI;
  if (turn < 0) {
    let end = Math.floor(following.perimeter);
    if (first.perimeter < following.perimeter) end -= 4;
    for (let corner = Math.trunc(first.perimeter - 5) + 4; end < corner; corner--) {
      first = insertCorner(first, following, column, row, corner);
    }
  } else {
    let end = Math.ceil(following.perimeter);
    if (following.perimeter < first.perimeter) end += 4;
    for (let corner = Math.trunc(first.perimeter + 1); corner < end; corner++) {
      first = insertCorner(first, following, column, row, corner);
    }
  }
  return first;
}

function clipPixel(model, list, column, row) {
  let current = list.head;
  while (current) {
    let start = current;
    while (current && (!current.next || current.region !== current.next.region)) {
      current = current.next;
      if (list.head === start) { list.head = current; start = current; }
    }
    if (!current) break;
    const region = current.region, count = model.regions[region].contour.length;
    start = current;
    let last = current;
    while (last.next && last.next.region === region) {
      const following = last.next, commonFlags = following.flags & last.flags;
      if ((commonFlags & 1) && last.sample !== following.sample &&
          ((last.sample + 1) % count !== following.sample || last.perimeter !== following.perimeter) &&
          (!(commonFlags & 2) || (last.x !== following.x && last.y !== following.y))) {
        last = connectPerimeter(last, following, column, row);
      }
      last = last.next;
    }
    const followingRegion = last.next;
    if ((last.flags & start.flags & 1) && ((last.sample + 1) % count !== start.sample || last.perimeter !== start.perimeter)) {
      last = connectPerimeter(last, start, column, row);
      last.next = followingRegion;
    }
    current = last.next;
  }
  // Native 0x4852d0 removes every region run with fewer than three records.
  let previous = null;
  current = list.head;
  while (current) {
    let last = current, count = 1;
    while (last.next && last.next.region === current.region) { last = last.next; count++; }
    if (count < 3) {
      if (current === list.head) list.head = last.next;
      else previous.next = last.next;
    }
    // Retain the native predecessor update even after removing a short run.
    // Testing head identity above is essential for consecutive leading runs.
    previous = last;
    current = last.next;
  }
}

function tracePixels(model, positions) {
  const lists = Array.from({ length:model.width * model.height }, () => ({ head:null, tail:null }));
  for (const [region, { contour }] of model.regions.entries()) {
    let previousAngle = contourAngleStep(positions[contour[1] * 2 + 1] - positions[contour[0] * 2 + 1],
      positions[contour[1] * 2] - positions[contour[0] * 2], 0).angle;
    let turn = 0;
    for (let sample = 0; sample <= contour.length; sample++) {
      if (sample > 0) {
        const first = contour[sample % contour.length] * 2, last = contour[(sample + 1) % contour.length] * 2;
        const step = contourAngleStep(positions[last + 1] - positions[first + 1], positions[last] - positions[first], previousAngle);
        let difference = step.difference;
        if (difference > Math.PI) difference -= 2 * Math.PI;
        else if (difference < -Math.PI) difference += 2 * Math.PI;
        turn = difference + turn;
        previousAngle = step.angle;
      }
      if (sample < contour.length) traceEdge(model, lists, positions, region, sample, turn);
    }
    if (Math.abs(turn - 2 * Math.PI) > 0.001) model.invalidRegions.add(region);
  }
  for (let pixel = 0; pixel < lists.length; pixel++) if (lists[pixel].head) {
    clipPixel(model, lists[pixel], pixel % model.width, Math.floor(pixel / model.width));
  }
  return lists;
}

function colorPixel(model, list, pixel) {
  const scratch = model.fragmentScratch;
  scratch.count = 0;
  scratch.background = -1;
  const offset = pixel * 4;
  model.colors.fill(0, offset, offset + 4);
  let total = 0, negativePolygon = false, current = list.head;
  while (current) {
    const start = current, region = model.regions[start.region];
    let twiceArea = 0;
    while (current.next && current.next.region === start.region) {
      const following = current.next;
      twiceArea = (following.y * current.x - current.y * following.x) + twiceArea;
      current = following;
    }
    const area = ((start.y * current.x - current.y * start.x) + twiceArea) * 0.5;
    total += area;
    const roundedArea = F(area);
    for (let channel = 0; channel < 4; channel++) model.colors[offset + channel] = F(F(roundedArea * region.color[channel]) + model.colors[offset + channel]);
    scratch.writeIdentifier(scratch.count, start.region);
    scratch.writeArea(scratch.count, area);
    if (area < 0) {
      model.flags[pixel] |= 1;
      if (region.contour.length > 4) negativePolygon = true;
    }
    scratch.count = scratch.count + 1;
    current = current.next;
  }
  const originalCount = scratch.count;
  if (!negativePolygon && Math.abs(total - 1) > 0.001) {
    for (let index = 0; index < originalCount; index++) {
      const parent = model.regions[scratch.identifier(index)].parent;
      if (parent >= 0 && scratch.find(parent) < 0 && scratch.background < 0) scratch.background = parent;
    }
    if (scratch.background >= 0) {
      total += 1;
      for (let channel = 0; channel < 4; channel++) model.colors[offset + channel] = F(model.regions[scratch.background].color[channel] + model.colors[offset + channel]);
      scratch.writeIdentifier(scratch.count, scratch.background);
      scratch.writeArea(scratch.count, 1);
      scratch.count = scratch.count + 1;
    }
    for (let index = 0; index < originalCount; index++) {
      const parent = model.regions[scratch.identifier(index)].parent;
      const parentIndex = scratch.find(parent);
      const colorRegion = parentIndex >= 0 ? parent : parent < 0 ? scratch.background : -1;
      if (colorRegion >= 0) {
        const roundedArea = F(scratch.area(index)), color = model.regions[colorRegion].color;
        for (let channel = 0; channel < 4; channel++) model.colors[offset + channel] = F(model.colors[offset + channel] - F(color[channel] * roundedArea));
        if (parentIndex >= 0) scratch.writeArea(parentIndex, scratch.area(parentIndex) - scratch.area(index));
        // The original background branch subtracts at count, not count-1.
        else scratch.writeArea(scratch.count, scratch.area(scratch.count) - scratch.area(index));
        total -= scratch.area(index);
      }
    }
    for (let index = 0; index < scratch.count; index++) if (scratch.area(index) < 0) { negativePolygon = true; break; }
  }
  if (negativePolygon || Math.abs(total - 1) > 0.001) {
    for (let index = 0; index < scratch.count; index++) model.invalidRegions.add(scratch.identifier(index));
    if (!negativePolygon) model.flags[pixel] |= 2;
    list.head = null;
    for (let channel = 0; channel < 4; channel++) model.colors[offset + channel] = F(model.bgra[offset + channel] * byteScale);
  }
  return scratch.background;
}

function imageGradient(model, lists, positions, backgrounds, gradient) {
  for (let pixel = 0; pixel < lists.length; pixel++) {
    const list = lists[pixel];
    if (!list.head) continue;
    const offset = pixel * 4;
    const residualB = F(model.colors[offset] - F(model.bgra[offset] * byteScale));
    const residualG = F(model.colors[offset + 1] - F(model.bgra[offset + 1] * byteScale));
    const residualR = F(model.colors[offset + 2] - F(model.bgra[offset + 2] * byteScale));
    const residualA = F(model.colors[offset + 3] - F(model.bgra[offset + 3] * byteScale));
    let current = list.head, start = current, regionId = -1, factor = 0;
    while (current) {
      if (current.region !== regionId) {
        regionId = current.region;
        const region = model.regions[regionId];
        let b = region.color[0], g = region.color[1], r = region.color[2], a = region.color[3];
        if (backgrounds[pixel] >= 0 && region.parent >= 0) {
          const parent = model.regions[region.parent].color;
          b = F(b - parent[0]); g = F(g - parent[1]); r = F(r - parent[2]); a = F(a - parent[3]);
        }
        factor = (((residualB * b + residualG * g) + residualR * r) + residualA * a) * 0.5;
        start = current;
      }
      const following = current.next && current.next.region === regionId ? current.next : start;
      const contour = model.regions[regionId].contour;
      addRecordGradient(gradient, positions, contour, current, following, factor, false);
      addRecordGradient(gradient, positions, contour, following, current, factor, true);
      current = current.next;
    }
  }
  return gradient;
}

function addRecordGradient(gradient, positions, contour, record, adjacent, factor, negate) {
  if (record.sample < 0) return;
  const sample = record.flags & 0x10 ? (record.sample + 1) % contour.length : record.sample;
  const first = contour[sample] * 2, last = contour[(sample + 1) % contour.length] * 2;
  if (!(record.flags & 1)) {
    gradient[first] = (negate ? -adjacent.y : adjacent.y) * factor + gradient[first];
    gradient[first + 1] = (negate ? adjacent.x : -adjacent.x) * factor + gradient[first + 1];
    return;
  }
  const variable = (!(record.flags & 0x80) && positions[first] !== positions[last]) || positions[first + 1] === positions[last + 1] ? 1 : 0;
  const fixed = variable === 0 ? 1 : 0;
  const fraction = (record[fixed === 0 ? 'x' : 'y'] - positions[last + fixed]) / (positions[first + fixed] - positions[last + fixed]);
  const slope = (positions[first + variable] - positions[last + variable]) / (positions[first + fixed] - positions[last + fixed]);
  const firstFixed = -(slope * fraction), lastVariable = 1 - fraction;
  const lastFixed = -((1 - fraction) * slope);
  const multiplier = variable === 0 ? (negate ? -adjacent.y : adjacent.y) : (negate ? adjacent.x : -adjacent.x);
  gradient[first] = (variable === 0 ? fraction : firstFixed) * multiplier * factor + gradient[first];
  gradient[first + 1] = (variable === 1 ? fraction : firstFixed) * multiplier * factor + gradient[first + 1];
  gradient[last] = (variable === 0 ? lastVariable : lastFixed) * multiplier * factor + gradient[last];
  gradient[last + 1] = (variable === 1 ? lastVariable : lastFixed) * multiplier * factor + gradient[last + 1];
}


// Public handles keep their mutable prepared storage private. Smoothing uses
// this trusted kernel after validating its public geometry boundary once.
export function registerImageKernel(handle, model) {
  model.invalidRegions = new Set();
  model.fragmentScratch = new PixelFragmentScratch();
  let lists, backgrounds, energy;
  const kernel = {
    nodeCount:model.nodeCount, width:model.width, height:model.height, regions:model.regions,
    energy(positions) {
      model.invalidRegions.clear();
      lists = tracePixels(model, positions);
      backgrounds = new Int32Array(lists.length).fill(-1);
      energy = 0;
      for (let pixel = 0; pixel < lists.length; pixel++) {
        if (lists[pixel].head) backgrounds[pixel] = colorPixel(model, lists[pixel], pixel);
        const offset = pixel * 4;
        const b = F(model.colors[offset] - F(model.bgra[offset] * byteScale));
        const g = F(model.colors[offset + 1] - F(model.bgra[offset + 1] * byteScale));
        const r = F(model.colors[offset + 2] - F(model.bgra[offset + 2] * byteScale));
        const a = F(model.colors[offset + 3] - F(model.bgra[offset + 3] * byteScale));
        energy = (((b * b + g * g) + r * r) + a * a) + energy;
      }
      energy *= 0.5;
      return energy;
    },
    addGradient(gradient, positions) {
      imageGradient(model, lists, positions, backgrounds, gradient);
    },
    invalidRegions() {
      return [...model.invalidRegions].sort((first, last) => first - last);
    },
    takeInvalidRegions() {
      const identifiers = kernel.invalidRegions();
      model.invalidRegions.clear();
      return identifiers;
    },
    snapshot(gradient) {
      const pixels = lists.map(list => {
        const records = [];
        for (let record = list.head; record; record = record.next) records.push([
          record.flags, record.sample, record.region, record.x, record.y, record.perimeter, record.turn,
        ]);
        return records;
      });
      return { energy, gradient, colors:model.colors.slice(), flags:model.flags.slice(), pixels };
    },
  };
  kernels.set(handle, kernel);
  return kernel;
}

export function readImageKernel(handle, geometry) {
  const kernel = kernels.get(handle);
  if (!kernel || kernel.nodeCount !== geometry.nodes.length || kernel.regions.length !== geometry.regions.length) {
    throw new Error('Image measurement code 1 needs a matching createContourImageModel instance.');
  }
  for (let index = 0; index < kernel.regions.length; index++) {
    const expected = kernel.regions[index], actual = geometry.regions[index];
    if (expected.pixelCount !== actual.pixelCount || expected.contour.length !== actual.contour.length ||
        expected.contour.some((node, sample) => node !== actual.contour[sample])) {
      throw new Error('Smoothing geometry must match the image model region contours and pixel counts.');
    }
  }
  return kernel;
}
