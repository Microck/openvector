import { readPaletteColors } from './pixel-input.mjs';

// Constructor patterns recovered from the original boundary tracer. The first
// matching axis mask wins; replacement bits select the retained grid samples.
const staircasePatterns = [[5, 0, 33], [9, 170, 585], [9, 146, 585],
  [13, 4369, 8481], [10, 132, 1057], [10, 330, 1057]];
const corners = [[0, 0], [1, 0], [1, 1], [0, 1]];
const quadrants = [[-1, -1], [-1, 0], [0, 0], [0, -1]];
const directions = [[-1, 0], [0, 1], [1, 0], [0, -1]];

function readContourInput(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected a completed segmentation object.');
  const { width, height, indices, counts, colorIndices, antiAlias } = input;
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 2 || height < 2 ||
      width > 2147483647 || height > 2147483647 || width * height > 2147483647) {
    throw new Error('Contour extraction needs int32 dimensions of at least 2 by 2.');
  }
  if (typeof antiAlias !== 'boolean') throw new Error('Contour extraction requires antiAlias: true or false.');
  const integerArray = value => Array.isArray(value) || value instanceof Int32Array;
  if (!integerArray(indices) || indices.length !== width * height || !integerArray(counts) ||
      counts.length === 0 || counts.length > indices.length || !integerArray(colorIndices) || colorIndices.length !== counts.length) {
    throw new Error('Contour extraction needs pixel labels, region counts and one color identifier per region.');
  }
  const palette = readPaletteColors(input.palette);
  const observed = new Int32Array(counts.length);
  let nextRegion = 0;
  for (const region of indices) {
    if (!Number.isInteger(region) || region < 0 || region >= counts.length) throw new Error('Contour labels must identify existing regions.');
    if (observed[region] === 0 && region !== nextRegion++) throw new Error('Contour regions must follow first row-major encounter order.');
    observed[region]++;
  }
  for (let region = 0; region < counts.length; region++) {
    if (counts[region] !== observed[region] || counts[region] === 0) throw new Error('Contour counts must match nonempty pixel regions.');
    if (!Number.isInteger(colorIndices[region]) || colorIndices[region] < 0 || colorIndices[region] >= palette.length / 4) {
      throw new Error('Contour colors must identify existing palette entries.');
    }
  }
  // A disconnected identifier has several boundary cycles, while the native
  // stage stores one outer contour per connected region. Reject it at entry.
  const visited = new Uint8Array(indices.length);
  const regionSeen = new Uint8Array(counts.length);
  const queue = new Int32Array(indices.length);
  for (let start = 0; start < indices.length; start++) {
    if (visited[start]) continue;
    const region = indices[start];
    if (regionSeen[region]) throw new Error('Contour regions must be four-connected.');
    regionSeen[region] = 1;
    let tail = 1;
    queue[0] = start;
    visited[start] = 1;
    for (let head = 0; head < tail; head++) {
      const pixel = queue[head], x = pixel % width, y = Math.floor(pixel / width);
      for (const [dx, dy] of directions) {
        const column = x + dx, row = y + dy;
        if (column < 0 || column >= width || row < 0 || row >= height) continue;
        const neighbor = row * width + column;
        if (!visited[neighbor] && indices[neighbor] === region) {
          visited[neighbor] = 1;
          queue[tail++] = neighbor;
        }
      }
    }
  }
  return { width, height, indices, counts, colorIndices, palette, antiAlias };
}

function buildGridNodes(width, height, labelAt) {
  const grid = new Int32Array((width + 1) * (height + 1)).fill(-1);
  const nodes = [];
  const add = (x, y) => {
    grid[y * (width + 1) + x] = nodes.length;
    const upperLeft = labelAt(x - 1, y - 1), upperRight = labelAt(x, y - 1);
    const lowerLeft = labelAt(x - 1, y), lowerRight = labelAt(x, y);
    const equalPairs = Number(upperLeft === upperRight) + Number(upperLeft === lowerLeft) + Number(upperLeft === lowerRight) +
      Number(upperRight === lowerLeft) + Number(upperRight === lowerRight) + Number(lowerLeft === lowerRight);
    const corner = (x === 0 || x === width) && (y === 0 || y === height);
    nodes.push({ x, y, kind:corner || equalPairs < 2 ? 3 : 0, flags:x === 0 || x === width || y === 0 || y === height ? 128 : 0 });
  };
  for (let y = 0; y <= height; y++) add(0, y);
  for (let y = 0; y <= height; y++) add(width, y);
  for (let x = 1; x < width; x++) add(x, 0);
  for (let x = 1; x < width; x++) add(x, height);
  for (let y = 1; y < height; y++) {
    for (let x = 1; x < width; x++) {
      const label = labelAt(x - 1, y - 1);
      if (label !== labelAt(x, y - 1) || label !== labelAt(x - 1, y) || label !== labelAt(x, y)) add(x, y);
    }
  }
  return { grid, nodes };
}

function traceRegion(region, startX, startY, width, height, grid, visited, labelAt) {
  const contour = [], neighbors = [];
  let x = startX, y = startY, previousInsideX = -1, previousInsideY = -1;
  while (true) {
    const current = y * (width + 1) + x;
    contour.push(grid[current]);
    const candidates = [];
    let closingOutside;
    for (let direction = 0; direction < 4; direction++) {
      const [dx, dy] = directions[direction];
      const nextX = x + dx, nextY = y + dy;
      if (nextX < 0 || nextX > width || nextY < 0 || nextY > height) continue;
      const next = nextY * (width + 1) + nextX;
      const [insideX, insideY] = quadrants[direction];
      const [outsideX, outsideY] = quadrants[(direction + 1) % 4];
      const outside = labelAt(x + outsideX, y + outsideY);
      if (grid[next] >= 0 && labelAt(x + insideX, y + insideY) === region && outside !== region) {
        // Native code records the outside label before rejecting a visited
        // endpoint. The closing edge contributes metadata without another node.
        closingOutside = outside;
        if (visited[next] !== region) candidates.push({ nextX, nextY, insideX:x + insideX, insideY:y + insideY, outside });
      }
    }
    if (candidates.length === 0) {
      visited[current] = region;
      neighbors.push(closingOutside);
      break;
    }
    let chosen;
    if (candidates.length < 2) {
      visited[current] = region;
      chosen = candidates[0];
      previousInsideX = chosen.insideX;
      previousInsideY = chosen.insideY;
    } else if (previousInsideX === -1 || previousInsideY === -1) {
      chosen = candidates[0];
      previousInsideX = chosen.insideX;
      previousInsideY = chosen.insideY;
    } else {
      chosen = candidates.find(candidate => candidate.insideX === previousInsideX && candidate.insideY === previousInsideY);
    }
    neighbors.push(chosen.outside);
    x = chosen.nextX;
    y = chosen.nextY;
    if (visited[y * (width + 1) + x] === region) break;
  }
  return { contour, boundary:neighbors.map(neighbor => [0, neighbor, 1]),
    neighbors:Array.from(new Set(neighbors.filter(neighbor => neighbor >= 0))).sort((a, b) => a - b) };
}

function reduceStaircases(nodes, regions) {
  for (const region of regions) {
    const { contour } = region, length = contour.length;
    if (length <= 19) continue;
    let runLength = 0, xBits = 0, yBits = 0, xDirection = 0, yDirection = 0;
    for (let step = 1; step < length + 31; step++) {
      const previous = nodes[contour[(step - 1) % length]], current = nodes[contour[step % length]];
      if ((previous.flags & 64) || (current.flags & 128) || previous.kind === 3 || current.kind === 3) {
        runLength = xBits = yBits = xDirection = yDirection = 0;
        continue;
      }
      if (step > 30) previous.flags |= 128;
      const dx = Math.floor(previous.x - current.x + 0.5), dy = Math.floor(previous.y - current.y + 0.5);
      if ((xDirection && dx && dx !== xDirection) || (yDirection && dy && dy !== yDirection)) runLength = xBits = yBits = 0;
      if (dx) xDirection = dx;
      if (dy) yDirection = dy;
      xBits = ((xBits << 1) | Number(dx !== 0)) >>> 0;
      yBits = ((yBits << 1) | Number(dy !== 0)) >>> 0;
      runLength = Math.min(runLength + 1, 31);
      if (runLength < 5) continue;
      for (const [patternLength, mask, replacement] of staircasePatterns) {
        const bits = (2 ** patternLength - 1) >>> 0;
        if (patternLength > runLength || (((xBits & bits) >>> 0) !== mask && ((yBits & bits) >>> 0) !== mask)) continue;
        let retained = replacement;
        for (let sample = 0; sample <= patternLength; sample++) {
          if (!(retained & 1)) nodes[contour[(step + length - sample) % length]].flags |= 192;
          retained >>= 1;
        }
        runLength = xBits = yBits = xDirection = yDirection = 0;
        break;
      }
    }
  }
}

function compactContours(nodes, regions) {
  const remap = new Int32Array(nodes.length).fill(-1);
  const kept = [];
  for (const [index, node] of nodes.entries()) {
    if (!(node.flags & 64)) { remap[index] = kept.length; kept.push(node); }
  }
  for (const region of regions) {
    const contour = [], boundary = [];
    let last = 0;
    for (const [sample, node] of region.contour.entries()) {
      if (remap[node] < 0) continue;
      contour.push(remap[node]);
      boundary.push([region.boundary[sample][0], region.boundary[sample][1], sample - last]);
      last = sample;
    }
    boundary[0][2] = region.contour.length - last;
    region.contour = contour;
    region.boundary = boundary;
  }
  return kept;
}

/** Recover shared boundaries before smoothing or curve fitting. */
export function extractContours(input) {
  const { width, height, indices, counts, colorIndices, palette, antiAlias } = readContourInput(input);
  const labelAt = (x, y) => x < 0 || x >= width || y < 0 || y >= height ? -1 : indices[y * width + x];
  const { grid, nodes } = buildGridNodes(width, height, labelAt);
  const visited = new Int32Array(grid.length).fill(-1);
  const regions = Array.from(counts, (pixelCount, region) => ({ pixelCount, colorIndex:colorIndices[region],
    bgra:Array.from(palette.subarray(colorIndices[region] * 4, colorIndices[region] * 4 + 4), channel => Math.trunc(channel * 255 + 0.5) & 255),
    parent:-1, contour:[], boundary:[], neighbors:[] }));
  for (let pixel = 0; pixel < indices.length; pixel++) {
    const region = indices[pixel];
    if (regions[region].contour.length) continue;
    const x = pixel % width, y = Math.floor(pixel / width);
    const [dx, dy] = corners.find(([offsetX, offsetY]) => grid[(y + offsetY) * (width + 1) + x + offsetX] >= 0);
    Object.assign(regions[region], traceRegion(region, x + dx, y + dy, width, height, grid, visited, labelAt));
  }
  for (const node of nodes) {
    if ((node.flags & 128) && node.kind !== 3) node.flags |= 64;
  }
  if (!antiAlias) reduceStaircases(nodes, regions);
  const compacted = compactContours(nodes, regions);
  for (const [region, entry] of regions.entries()) {
    entry.parent = entry.neighbors.find(neighbor => !regions[neighbor].neighbors.includes(region)) ?? -1;
  }
  return { nodes:compacted, regions };
}
