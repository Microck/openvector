// Original sliding-midpoint tree and capped nearest search. Contour samples,
// including shared-node duplicates, are the indexed items rather than nodes.
function coordinateRange(points, order, start, count, axis) {
  let minimum = points[order[start]][axis], maximum = minimum;
  for (let index = start + 1; index < start + count; index++) {
    const value = points[order[index]][axis];
    if (minimum <= value) { if (maximum < value) maximum = value; }
    else minimum = value;
  }
  return [minimum, maximum];
}

function partition(points, order, start, count, axis, cut) {
  const coordinate = index => points[order[start + index]][axis];
  const swap = (first, last) => {
    const saved = order[start + first];
    order[start + first] = order[start + last];
    order[start + last] = saved;
  };
  let left = 0, right = count - 1;
  while (true) {
    while (left < count && coordinate(left) < cut) left++;
    while (right >= 0 && coordinate(right) >= cut) right--;
    if (right < left) break;
    swap(left++, right--);
  }
  const below = left;
  right = count - 1;
  while (true) {
    while (left < count && coordinate(left) <= cut) left++;
    while (right >= below && coordinate(right) > cut) right--;
    if (right < left) return [below, left];
    swap(left++, right--);
  }
}

function buildTree(points, order, start, count, lower, upper) {
  if (count === 1) return { sample:order[start] };
  const widest = Math.max(upper[0] - lower[0], upper[1] - lower[1]);
  let axis = 0, spread = -1, selectedRange;
  for (let candidate = 0; candidate < 2; candidate++) if (widest * 0.999 <= upper[candidate] - lower[candidate]) {
    const [minimum, maximum] = coordinateRange(points, order, start, count, candidate);
    if (spread < maximum - minimum) {
      spread = maximum - minimum;
      axis = candidate;
      selectedRange = [minimum, maximum];
    }
  }
  const middle = (lower[axis] + upper[axis]) * 0.5;
  // Selecting the axis already scanned this immutable sample range.
  const [minimum, maximum] = selectedRange;
  let cut = minimum;
  if (minimum <= middle) cut = maximum < middle ? maximum : middle;
  const [below, equalEnd] = partition(points, order, start, count, axis, cut);
  let leftCount;
  if (middle < minimum) leftCount = 1;
  else if (maximum < middle) leftCount = count - 1;
  else leftCount = Math.max(below, Math.min(Math.trunc(count / 2), equalEnd));
  const oldLower = lower[axis], oldUpper = upper[axis];
  upper[axis] = cut;
  const left = buildTree(points, order, start, leftCount, lower, upper);
  upper[axis] = oldUpper;
  lower[axis] = cut;
  const right = buildTree(points, order, start + leftCount, count - leftCount, lower, upper);
  lower[axis] = oldLower;
  return { axis, cut, lower:oldLower, upper:oldUpper, left, right };
}

function nearestSamples(tree, points, query, capacity) {
  const nearest = [], approximation = (1 + 1e-12) * (1 + 1e-12);
  const maximum = () => nearest.length === capacity ? nearest[capacity - 1].distance : Number.MAX_VALUE;
  function visit(branch, boxDistance) {
    if (branch.sample !== undefined) {
      const point = points[branch.sample];
      const dx = query[0] - point[0];
      let distance = dx * dx + 0;
      if (maximum() < distance) return;
      const dy = query[1] - point[1];
      distance = dy * dy + distance;
      if (maximum() < distance) return;
      let index = nearest.length;
      // Existing equal distances precede the new sample. A new tied item at
      // the capacity boundary is discarded, preserving native visit order.
      while (index > 0 && nearest[index - 1].distance > distance) index--;
      nearest.splice(index, 0, { sample:branch.sample, distance });
      if (nearest.length > capacity) nearest.pop();
      return;
    }
    const difference = query[branch.axis] - branch.cut;
    if (difference >= 0) {
      visit(branch.right, boxDistance);
      const outside = Math.max(query[branch.axis] - branch.upper, 0);
      const otherDistance = (difference * difference - outside * outside) + boxDistance;
      if (approximation * otherDistance < maximum()) visit(branch.left, otherDistance);
    } else {
      visit(branch.left, boxDistance);
      const outside = Math.max(branch.lower - query[branch.axis], 0);
      const otherDistance = (difference * difference - outside * outside) + boxDistance;
      if (approximation * otherDistance < maximum()) visit(branch.right, otherDistance);
    }
  }
  // Every query is an indexed node, so it lies inside the root bounding box.
  visit(tree, 0);
  return nearest;
}

/** Build the original fixed contour neighborhoods before moving shared nodes. */
export function prepareContourNeighborhoods(input) {
  const geometry = input?.geometry;
  if (!geometry || !Array.isArray(geometry.nodes) || geometry.nodes.length < 1 ||
      !Array.isArray(geometry.regions) || geometry.regions.length < 1) {
    throw new Error('Contour neighborhoods need shared nodes and region contours.');
  }
  const points = [], samples = [];
  const flags = Uint8Array.from(geometry.nodes, node => {
    if (!node || !Number.isFinite(node.x) || !Number.isFinite(node.y) ||
        !Number.isInteger(node.flags) || node.flags < 0 || node.flags > 255) {
      throw new Error('Contour neighborhoods need finite node coordinates and byte flags.');
    }
    return node.flags;
  });
  for (const [region, entry] of geometry.regions.entries()) {
    const contour = entry?.contour;
    if ((!Array.isArray(contour) && !(contour instanceof Int32Array)) || contour.length < 3) {
      throw new Error('Contour neighborhoods need nonempty region contours.');
    }
    for (const [sample, node] of contour.entries()) {
      if (!Number.isInteger(node) || node < 0 || node >= geometry.nodes.length) {
        throw new Error('Contour neighborhoods must identify existing nodes.');
      }
      points.push([geometry.nodes[node].x, geometry.nodes[node].y]);
      samples.push([region, sample, node]);
    }
  }
  const capacity = Math.min(50, geometry.nodes.length);
  if (points.length < capacity) throw new Error('Contour neighborhoods need at least as many samples as requested neighbors.');
  const order = Int32Array.from(points, (point, index) => index);
  const lower = [], upper = [];
  for (let axis = 0; axis < 2; axis++) [lower[axis], upper[axis]] = coordinateRange(points, order, 0, points.length, axis);
  const tree = buildTree(points, order, 0, points.length, lower, upper);
  const nearbyEdges = Array.from(geometry.nodes, () => []);
  for (const { contour } of geometry.regions) for (const node of contour) {
    if (flags[node] & 1) continue;
    const query = [geometry.nodes[node].x, geometry.nodes[node].y];
    for (const { sample, distance } of nearestSamples(tree, points, query, capacity)) {
      const [otherRegion, otherSample, otherNode] = samples[sample];
      if (distance < 25 && otherNode !== node) nearbyEdges[node].push([otherRegion, otherSample]);
    }
    flags[node] |= 1;
  }
  return { nearbyEdges, flags };
}
