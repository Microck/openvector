const f32 = Math.fround;
const floatByteScale = f32(1 / 255);
const int32Max = 2147483647;

export function readRasterInput(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected a region initialization object.');
  const { width, height, bgra } = input;
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1 ||
      width > int32Max || height > int32Max || width * height > int32Max) {
    throw new Error('Region dimensions must be positive int32 values with an int32 pixel count.');
  }
  if ((!Array.isArray(bgra) && !(bgra instanceof Uint8Array)) || bgra.length !== width * height * 4) {
    throw new Error('Region pixels must contain exactly width * height BGRA byte pixels.');
  }
  if (Array.isArray(bgra)) {
    for (const channel of bgra) {
      if (!Number.isInteger(channel) || channel < 0 || channel > 255) {
        throw new Error('Region pixel channels must be integers from 0 to 255.');
      }
    }
  }
  return { width, height, bgra };
}

export function readRegionInput(input) {
  const { width, height, bgra } = readRasterInput(input);
  const { regions } = input;
  if ((!Array.isArray(regions) && !(regions instanceof Int32Array)) || regions.length !== width * height) {
    throw new Error('Optimizer needs one region label per pixel.');
  }
  for (const region of regions) {
    if (!Number.isInteger(region) || region < 0 || region > int32Max) {
      throw new Error('Optimizer labels must be nonnegative int32 values.');
    }
  }
  return { width, height, bgra, regions };
}

/** Rebuild validated labels and statistics without copying caller pixel storage. */
export function rebuildConnectedRegions({ width, bgra, regions }, previousStatistics = null) {
  const previous = regions;
  const indices = new Int32Array(previous.length).fill(-1);
  const queue = new Int32Array(previous.length);
  let regionCount = 0;
  const colorIndices = [];
  for (let seed = 0; seed < previous.length; seed++) {
    if (indices[seed] >= 0) continue;
    let head = 0;
    let tail = 1;
    queue[0] = seed;
    indices[seed] = regionCount;
    colorIndices.push(previousStatistics === null ? -1 : previousStatistics[previous[seed]].colorIndex);
    const visit = pixel => {
      if (indices[pixel] < 0 && previous[pixel] === previous[seed]) {
        indices[pixel] = regionCount;
        queue[tail++] = pixel;
      }
    };
    while (head < tail) {
      const pixel = queue[head++];
      const column = pixel % width;
      if (column > 0) visit(pixel - 1);
      if (column + 1 < width) visit(pixel + 1);
      if (pixel >= width) visit(pixel - width);
      if (pixel + width < previous.length) visit(pixel + width);
    }
    regionCount++;
  }
  const statistics = colorIndices.map(colorIndex => ({ count: 0, sums: new Float32Array(4), colorIndex }));
  // Statistics must follow row-major order, independent of flood traversal.
  // Summing during the traversal would change float32 cancellation and ties.
  for (let pixel = 0; pixel < indices.length; pixel++) {
    const statistic = statistics[indices[pixel]];
    statistic.count++;
    for (let channel = 0; channel < 4; channel++) {
      statistic.sums[channel] += f32(bgra[pixel * 4 + channel] * floatByteScale);
    }
  }
  return { indices, statistics };
}

/** Deduplicate engine BGRA bytes in encounter order and retain their identifiers. */
export function paletteFromByteColors(colors) {
  const palette = [];
  const indices = [];
  const lookup = new Map();
  for (const bytes of colors) {
    const key = ((bytes[3] << 24) | (bytes[2] << 16) | (bytes[1] << 8) | bytes[0]) >>> 0;
    let color = lookup.get(key);
    if (color === undefined) {
      color = palette.length / 4;
      lookup.set(key, color);
      for (const channel of bytes) palette.push(f32(channel * floatByteScale));
    }
    indices.push(color);
  }
  return { palette: Float32Array.from(palette), colorIndices: Int32Array.from(indices) };
}

/** Mutate projected labels using the native diagonal-contact topology rules. */
export function repairDiagonalMap({ width, height, bgra }, indices) {
  function labelAt(x, y) {
    if (x < 0 || x >= width || y < 0 || y >= height) return -1;
    return indices[y * width + x];
  }

  function protectedCorner(x, y, dx, dy) {
    const region = labelAt(x, y);
    return region === labelAt(x, y + dy) && region === labelAt(x + dx, y) &&
      region !== labelAt(x + dx, y + dy);
  }

  function diagonalLineCost(x, y, dx, dy) {
    if (x - dx < 0 || x - dx >= width || y - dy < 0 || y - dy >= height ||
        x + dx < 0 || x + dx >= width || y + dy < 0 || y + dy >= height) return 0;
    const first = ((y - dy) * width + x - dx) * 4;
    const middle = (y * width + x) * 4;
    const last = ((y + dy) * width + x + dx) * 4;
    let cost = 0;
    for (let channel = 0; channel < 4; channel++) {
      const difference = f32((bgra[first + channel] - 2 * bgra[middle + channel] +
        bgra[last + channel]) * floatByteScale);
      cost = f32(cost + f32(difference * difference));
    }
    return cost;
  }

  function pixelDistance(first, second) {
    let distance = 0;
    for (let channel = 0; channel < 4; channel++) {
      const difference = f32((bgra[first * 4 + channel] - bgra[second * 4 + channel]) * floatByteScale);
      // The original uses double-precision x87 products and accumulation here.
      distance += difference * difference;
    }
    return distance;
  }

  for (let y = 0; y + 1 < height; y++) {
    for (let x = 0; x + 1 < width; x++) {
      const topLeft = labelAt(x, y);
      const topRight = labelAt(x + 1, y);
      const bottomLeft = labelAt(x, y + 1);
      const bottomRight = labelAt(x + 1, y + 1);
      if (topLeft === bottomLeft || topLeft === topRight || bottomRight === bottomLeft || bottomRight === topRight) continue;
      let mainDiagonal = topLeft === bottomRight;
      const otherDiagonal = topRight === bottomLeft;
      if (!mainDiagonal && !otherDiagonal) continue;
      if (mainDiagonal && otherDiagonal) {
        let mainCost = 0, otherCost = 0;
        for (let row = y; row <= y + 1; row++) {
          for (let column = x; column <= x + 1; column++) {
            mainCost = f32(mainCost + diagonalLineCost(column, row, 1, 1));
            otherCost = f32(otherCost + diagonalLineCost(column, row, 1, -1));
          }
        }
        if (otherCost < mainCost) mainDiagonal = false;
        else if (otherCost <= mainCost) continue;
      }
      const firstX = mainDiagonal ? x + 1 : x;
      const secondX = mainDiagonal ? x : x + 1;
      const dx = mainDiagonal ? 1 : -1;
      const firstProtected = protectedCorner(firstX, y, dx, -1);
      const secondProtected = protectedCorner(secondX, y + 1, -dx, 1);
      if (firstProtected && secondProtected) continue;
      const first = y * width + firstX;
      const second = (y + 1) * width + secondX;
      let replacement = second;
      if (!firstProtected && secondProtected) replacement = first;
      else if (!firstProtected) {
        const firstTarget = (mainDiagonal ? y + 1 : y) * width + x + 1;
        const secondTarget = (mainDiagonal ? y : y + 1) * width + x;
        // The first candidate's sum is stored as float32. The second stays
        // double until the comparison, so apparent float32 ties can differ.
        const firstCost = f32(f32(pixelDistance(firstTarget, first)) + pixelDistance(secondTarget, first));
        const secondCost = f32(pixelDistance(firstTarget, second)) + pixelDistance(secondTarget, second);
        if (secondCost > firstCost) replacement = first;
      }
      indices[replacement] = mainDiagonal ? topLeft : bottomLeft;
    }
  }
}
